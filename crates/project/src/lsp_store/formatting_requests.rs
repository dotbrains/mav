use super::*;

impl LocalLspStore {
    pub async fn format_ranges_via_lsp(
        this: &WeakEntity<LspStore>,
        buffer_handle: &Entity<Buffer>,
        ranges: &[Range<Anchor>],
        abs_path: &Path,
        language_server: &Arc<LanguageServer>,
        settings: &LanguageSettings,
        cx: &mut AsyncApp,
    ) -> Result<Vec<(Range<Anchor>, Arc<str>)>> {
        let capabilities = &language_server.capabilities();
        let range_formatting_provider = capabilities.document_range_formatting_provider.as_ref();
        if range_formatting_provider == Some(&OneOf::Left(false)) {
            anyhow::bail!(
                "{} language server does not support range formatting",
                language_server.name()
            );
        }

        let uri = file_path_to_lsp_url(abs_path)?;
        let text_document = lsp::TextDocumentIdentifier::new(uri);

        let request_timeout = cx.update(|app| {
            ProjectSettings::get_global(app)
                .global_lsp_settings
                .get_request_timeout()
        });
        let lsp_edits = {
            let mut lsp_ranges = Vec::new();
            this.update(cx, |_this, cx| {
                // TODO(#22930): In the case of formatting multibuffer selections, this buffer may
                // not have been sent to the language server. This seems like a fairly systemic
                // issue, though, the resolution probably is not specific to formatting.
                //
                // TODO: Instead of using current snapshot, should use the latest snapshot sent to
                // LSP.
                let snapshot = buffer_handle.read(cx).snapshot();
                for range in ranges {
                    lsp_ranges.push(range_to_lsp(range.to_point_utf16(&snapshot))?);
                }
                anyhow::Ok(())
            })??;

            let mut edits = None;
            for range in lsp_ranges {
                if let Some(mut edit) = language_server
                    .request::<lsp::request::RangeFormatting>(
                        lsp::DocumentRangeFormattingParams {
                            text_document: text_document.clone(),
                            range,
                            options: lsp_command::lsp_formatting_options(settings),
                            work_done_progress_params: Default::default(),
                        },
                        request_timeout,
                    )
                    .await
                    .into_response()?
                {
                    edits.get_or_insert_with(Vec::new).append(&mut edit);
                }
            }
            edits
        };

        if let Some(lsp_edits) = lsp_edits {
            this.update(cx, |this, cx| {
                this.as_local_mut().unwrap().edits_from_lsp(
                    buffer_handle,
                    lsp_edits,
                    language_server.server_id(),
                    None,
                    cx,
                )
            })?
            .await
        } else {
            Ok(Vec::with_capacity(0))
        }
    }

    pub(super) fn server_supports_formatting(server: &Arc<LanguageServer>) -> bool {
        let capabilities = server.capabilities();
        let formatting = capabilities.document_formatting_provider.as_ref();
        matches!(formatting, Some(p) if *p != OneOf::Left(false))
            || server_capabilities_support_range_formatting(&capabilities)
    }

    pub(super) async fn format_via_lsp(
        this: &WeakEntity<LspStore>,
        buffer: &Entity<Buffer>,
        abs_path: &Path,
        language_server: &Arc<LanguageServer>,
        settings: &LanguageSettings,
        cx: &mut AsyncApp,
    ) -> Result<Vec<(Range<Anchor>, Arc<str>)>> {
        let logger = zlog::scoped!("lsp_format");
        zlog::debug!(logger => "Formatting via LSP");

        let uri = file_path_to_lsp_url(abs_path)?;
        let text_document = lsp::TextDocumentIdentifier::new(uri);
        let capabilities = &language_server.capabilities();

        let formatting_provider = capabilities.document_formatting_provider.as_ref();
        let range_formatting_provider = capabilities.document_range_formatting_provider.as_ref();

        let request_timeout = cx.update(|app| {
            ProjectSettings::get_global(app)
                .global_lsp_settings
                .get_request_timeout()
        });

        let lsp_edits = if matches!(formatting_provider, Some(p) if *p != OneOf::Left(false)) {
            let _timer = zlog::time!(logger => "format-full");
            language_server
                .request::<lsp::request::Formatting>(
                    lsp::DocumentFormattingParams {
                        text_document,
                        options: lsp_command::lsp_formatting_options(settings),
                        work_done_progress_params: Default::default(),
                    },
                    request_timeout,
                )
                .await
                .into_response()?
        } else if matches!(range_formatting_provider, Some(p) if *p != OneOf::Left(false)) {
            let _timer = zlog::time!(logger => "format-range");
            let buffer_start = lsp::Position::new(0, 0);
            let buffer_end = buffer.read_with(cx, |b, _| point_to_lsp(b.max_point_utf16()));
            language_server
                .request::<lsp::request::RangeFormatting>(
                    lsp::DocumentRangeFormattingParams {
                        text_document: text_document.clone(),
                        range: lsp::Range::new(buffer_start, buffer_end),
                        options: lsp_command::lsp_formatting_options(settings),
                        work_done_progress_params: Default::default(),
                    },
                    request_timeout,
                )
                .await
                .into_response()?
        } else {
            None
        };

        if let Some(lsp_edits) = lsp_edits {
            this.update(cx, |this, cx| {
                this.as_local_mut().unwrap().edits_from_lsp(
                    buffer,
                    lsp_edits,
                    language_server.server_id(),
                    None,
                    cx,
                )
            })?
            .await
        } else {
            Ok(Vec::with_capacity(0))
        }
    }

    pub(super) async fn format_via_external_command(
        buffer: &FormattableBuffer,
        command: &str,
        arguments: Option<&[String]>,
        cx: &mut AsyncApp,
    ) -> Result<Option<Diff>> {
        let working_dir_path = buffer.handle.update(cx, |buffer, cx| {
            let file = File::from_dyn(buffer.file())?;
            let worktree = file.worktree.read(cx);
            let mut worktree_path = worktree.abs_path().to_path_buf();
            if worktree.root_entry()?.is_file() {
                worktree_path.pop();
            }
            Some(worktree_path)
        });

        use util::command::Stdio;
        let mut child = util::command::new_command(command);

        if let Some(buffer_env) = buffer.env.as_ref() {
            child.envs(buffer_env);
        }

        if let Some(working_dir_path) = working_dir_path {
            child.current_dir(working_dir_path);
        }

        if let Some(arguments) = arguments {
            child.args(arguments.iter().map(|arg| {
                if let Some(buffer_abs_path) = buffer.abs_path.as_ref() {
                    arg.replace("{buffer_path}", &buffer_abs_path.to_string_lossy())
                } else {
                    arg.replace("{buffer_path}", "Untitled")
                }
            }));
        }

        let mut child = child
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.as_mut().context("failed to acquire stdin")?;
        let text = buffer
            .handle
            .read_with(cx, |buffer, _| buffer.as_rope().clone());
        for chunk in text.chunks() {
            stdin.write_all(chunk.as_bytes()).await?;
        }
        stdin.flush().await?;

        let output = child.output().await?;
        anyhow::ensure!(
            output.status.success(),
            "command failed with exit code {:?}:\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        let stdout = String::from_utf8(output.stdout)?;
        Ok(Some(
            buffer
                .handle
                .update(cx, |buffer, cx| buffer.diff(stdout, cx))
                .await,
        ))
    }
}
