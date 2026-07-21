use super::*;

impl LocalLspStore {
    pub(super) async fn apply_formatter(
        formatter: &Formatter,
        lsp_store: &WeakEntity<LspStore>,
        buffer: &FormattableBuffer,
        formatting_transaction_id: clock::Lamport,
        adapters_and_servers: &[(Arc<CachedLspAdapter>, Arc<LanguageServer>)],
        settings: &LanguageSettings,
        request_timeout: Duration,
        logger: zlog::Logger,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        match formatter {
            Formatter::None => {
                zlog::trace!(logger => "skipping formatter 'none'");
                return Ok(());
            }
            Formatter::Auto => {
                debug_panic!("Auto resolved above");
                return Ok(());
            }
            Formatter::Prettier => {
                let logger = zlog::scoped!(logger => "prettier");
                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer via prettier");

                // When selection ranges are provided (via FormatSelections), we pass the
                // encompassing UTF-16 range to Prettier so it can scope its formatting.
                // After diffing, we filter the resulting edits to only keep those that
                // overlap with the original byte-level selection ranges.
                let (range_utf16, byte_ranges) = match buffer.ranges.as_ref() {
                    Some(ranges) if !ranges.is_empty() => {
                        let (utf16_range, byte_ranges) =
                            buffer.handle.read_with(cx, |buffer, _cx| {
                                let snapshot = buffer.snapshot();
                                let mut min_start_utf16 = OffsetUtf16(usize::MAX);
                                let mut max_end_utf16 = OffsetUtf16(0);
                                let mut byte_ranges = Vec::with_capacity(ranges.len());
                                for range in ranges {
                                    let start_utf16 = range.start.to_offset_utf16(&snapshot);
                                    let end_utf16 = range.end.to_offset_utf16(&snapshot);
                                    min_start_utf16.0 = min_start_utf16.0.min(start_utf16.0);
                                    max_end_utf16.0 = max_end_utf16.0.max(end_utf16.0);

                                    let start_byte = range.start.to_offset(&snapshot);
                                    let end_byte = range.end.to_offset(&snapshot);
                                    byte_ranges.push(start_byte..end_byte);
                                }
                                (min_start_utf16..max_end_utf16, byte_ranges)
                            });
                        (Some(utf16_range), Some(byte_ranges))
                    }
                    _ => (None, None),
                };

                let prettier = lsp_store.read_with(cx, |lsp_store, _cx| {
                    lsp_store.prettier_store().unwrap().downgrade()
                })?;
                let diff = prettier_store::format_with_prettier(
                    &prettier,
                    &buffer.handle,
                    range_utf16,
                    cx,
                )
                .await
                .transpose()?;
                let Some(mut diff) = diff else {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                };

                if let Some(byte_ranges) = byte_ranges {
                    diff.edits.retain(|(edit_range, _)| {
                        byte_ranges.iter().any(|selection_range| {
                            edit_range.start < selection_range.end
                                && edit_range.end > selection_range.start
                        })
                    });
                    if diff.edits.is_empty() {
                        zlog::trace!(logger => "No changes within selection");
                        return Ok(());
                    }
                }

                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.apply_diff(diff, cx);
                    },
                )?;
            }
            Formatter::External { command, arguments } => {
                let logger = zlog::scoped!(logger => "command");

                if buffer.ranges.is_some() {
                    zlog::debug!(logger => "External formatter does not support range formatting; skipping");
                    return Ok(());
                }

                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer via external command");

                let diff =
                    Self::format_via_external_command(buffer, &command, arguments.as_deref(), cx)
                        .await
                        .with_context(|| {
                            format!("Failed to format buffer via external command: {}", command)
                        })?;
                let Some(diff) = diff else {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                };

                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.apply_diff(diff, cx);
                    },
                )?;
            }
            Formatter::LanguageServer(specifier) => {
                let logger = zlog::scoped!(logger => "language-server");
                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer using language server");

                let Some(buffer_path_abs) = buffer.abs_path.as_ref() else {
                    zlog::warn!(logger => "Cannot format buffer that is not backed by a file on disk using language servers. Skipping");
                    return Ok(());
                };

                let language_server = match specifier {
                    settings::LanguageServerFormatterSpecifier::Specific { name } => {
                        adapters_and_servers.iter().find_map(|(adapter, server)| {
                            if adapter.name.0.as_ref() == name {
                                Some(server.clone())
                            } else {
                                None
                            }
                        })
                    }
                    settings::LanguageServerFormatterSpecifier::Current => adapters_and_servers
                        .iter()
                        .find(|(_, server)| Self::server_supports_formatting(server))
                        .map(|(_, server)| server.clone()),
                };

                let Some(language_server) = language_server else {
                    log::debug!(
                        "No language server found to format buffer '{:?}'. Skipping",
                        buffer_path_abs.as_path().to_string_lossy()
                    );
                    return Ok(());
                };

                zlog::trace!(
                    logger =>
                    "Formatting buffer '{:?}' using language server '{:?}'",
                    buffer_path_abs.as_path().to_string_lossy(),
                    language_server.name()
                );

                let edits = if let Some(ranges) = buffer.ranges.as_ref() {
                    zlog::trace!(logger => "formatting ranges");
                    Self::format_ranges_via_lsp(
                        &lsp_store,
                        &buffer.handle,
                        ranges,
                        buffer_path_abs,
                        &language_server,
                        &settings,
                        cx,
                    )
                    .await
                    .context("Failed to format ranges via language server")?
                } else {
                    zlog::trace!(logger => "formatting full");
                    Self::format_via_lsp(
                        &lsp_store,
                        &buffer.handle,
                        buffer_path_abs,
                        &language_server,
                        &settings,
                        cx,
                    )
                    .await
                    .context("failed to format via language server")?
                };

                if edits.is_empty() {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                }
                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.edit(edits, None, cx);
                    },
                )?;
            }
            Formatter::CodeAction(code_action_name) => {
                Self::apply_code_action_formatter(
                    code_action_name.as_ref(),
                    lsp_store,
                    buffer,
                    formatting_transaction_id,
                    adapters_and_servers,
                    request_timeout,
                    logger,
                    cx,
                )
                .await?;
            }
        }

        Ok(())
    }
}
