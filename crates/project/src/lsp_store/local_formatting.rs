use super::*;

impl LocalLspStore {
    pub(super) async fn format_locally(
        lsp_store: WeakEntity<LspStore>,
        mut buffers: Vec<FormattableBuffer>,
        push_to_history: bool,
        trigger: FormatTrigger,
        logger: zlog::Logger,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<ProjectTransaction> {
        // Do not allow multiple concurrent formatting requests for the
        // same buffer.
        lsp_store.update(cx, |this, cx| {
            let this = this.as_local_mut().unwrap();
            buffers.retain(|buffer| {
                this.buffers_being_formatted
                    .insert(buffer.handle.read(cx).remote_id())
            });
        })?;

        let _cleanup = defer({
            let this = lsp_store.clone();
            let mut cx = cx.clone();
            let buffers = &buffers;
            move || {
                this.update(&mut cx, |this, cx| {
                    let this = this.as_local_mut().unwrap();
                    for buffer in buffers {
                        this.buffers_being_formatted
                            .remove(&buffer.handle.read(cx).remote_id());
                    }
                })
                .ok();
            }
        });

        let mut project_transaction = ProjectTransaction::default();

        for buffer in &buffers {
            zlog::debug!(
                logger =>
                "formatting buffer '{:?}'",
                buffer.abs_path.as_ref().unwrap_or(&PathBuf::from("unknown")).display()
            );
            // Create an empty transaction to hold all of the formatting edits.
            let formatting_transaction_id = buffer.handle.update(cx, |buffer, cx| {
                // ensure no transactions created while formatting are
                // grouped with the previous transaction in the history
                // based on the transaction group interval
                buffer.finalize_last_transaction();
                buffer
                    .start_transaction()
                    .context("transaction already open")?;
                buffer.end_transaction(cx);
                let transaction_id = buffer.push_empty_transaction(cx.background_executor().now());
                buffer.finalize_last_transaction();
                anyhow::Ok(transaction_id)
            })?;

            let result = Self::format_buffer_locally(
                lsp_store.clone(),
                buffer,
                formatting_transaction_id,
                trigger,
                logger,
                cx,
            )
            .await;

            buffer.handle.update(cx, |buffer, cx| {
                let Some(formatting_transaction) =
                    buffer.get_transaction(formatting_transaction_id).cloned()
                else {
                    zlog::warn!(logger => "no formatting transaction");
                    return;
                };
                if formatting_transaction.edit_ids.is_empty() {
                    zlog::debug!(logger => "no changes made while formatting");
                    buffer.forget_transaction(formatting_transaction_id);
                    return;
                }
                if !push_to_history {
                    zlog::trace!(logger => "forgetting format transaction");
                    buffer.forget_transaction(formatting_transaction.id);
                }
                project_transaction
                    .0
                    .insert(cx.entity(), formatting_transaction);
            });

            result?;
        }

        Ok(project_transaction)
    }

    pub(super) async fn format_buffer_locally(
        lsp_store: WeakEntity<LspStore>,
        buffer: &FormattableBuffer,
        formatting_transaction_id: clock::Lamport,
        trigger: FormatTrigger,
        logger: zlog::Logger,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let (adapters_and_servers, settings, request_timeout) =
            lsp_store.update(cx, |lsp_store, cx| {
                buffer.handle.update(cx, |buffer, cx| {
                    let adapters_and_servers = lsp_store
                        .as_local()
                        .unwrap()
                        .language_servers_for_buffer(buffer, cx)
                        .map(|(adapter, lsp)| (adapter.clone(), lsp.clone()))
                        .collect::<Vec<_>>();
                    let settings = LanguageSettings::for_buffer(buffer, cx).into_owned();
                    let request_timeout = ProjectSettings::get_global(cx)
                        .global_lsp_settings
                        .get_request_timeout();
                    (adapters_and_servers, settings, request_timeout)
                })
            })?;
        let had_existing_line_endings = buffer
            .handle
            .read_with(cx, |buffer, _| buffer.max_point().row > 0);

        // handle whitespace formatting
        if settings.remove_trailing_whitespace_on_save {
            zlog::trace!(logger => "removing trailing whitespace");
            let diff = buffer
                .handle
                .read_with(cx, |buffer, cx| buffer.remove_trailing_whitespace(cx))
                .await;
            extend_formatting_transaction(buffer, formatting_transaction_id, cx, |buffer, cx| {
                buffer.apply_diff(diff, cx);
            })?;
        }

        if settings.ensure_final_newline_on_save {
            zlog::trace!(logger => "ensuring final newline");
            extend_formatting_transaction(buffer, formatting_transaction_id, cx, |buffer, cx| {
                buffer.ensure_final_newline(cx);
            })?;
        }

        let line_ending_policy = match settings.line_ending {
            LineEndingSetting::Detect => None,
            LineEndingSetting::PreferLf => Some((LineEnding::Unix, true)),
            LineEndingSetting::PreferCrlf => Some((LineEnding::Windows, true)),
            LineEndingSetting::EnforceLf => Some((LineEnding::Unix, false)),
            LineEndingSetting::EnforceCrlf => Some((LineEnding::Windows, false)),
        };
        if let Some((desired_line_ending, preserve_existing)) = line_ending_policy {
            buffer.handle.update(cx, |buffer, cx| {
            if buffer.line_ending() == desired_line_ending {
                return;
            }
            if preserve_existing && had_existing_line_endings {
                zlog::trace!(
                    logger => "preserving existing line endings ({}) on save",
                    buffer.line_ending().label()
                );
                return;
            }
            zlog::trace!(logger => "normalizing line endings to {}", desired_line_ending.label());
            buffer.set_line_ending(desired_line_ending, cx);
        });
        }

        // Formatter for `code_actions_on_format` that runs before
        // the rest of the formatters
        let mut code_actions_on_format_formatters = None;
        let should_run_code_actions_on_format = !matches!(
            (trigger, &settings.format_on_save),
            (FormatTrigger::Save, &FormatOnSave::Off)
        );
        if should_run_code_actions_on_format {
            let have_code_actions_to_run_on_format = settings
                .code_actions_on_format
                .values()
                .any(|enabled| *enabled);
            if have_code_actions_to_run_on_format {
                zlog::trace!(logger => "going to run code actions on format");
                code_actions_on_format_formatters = Some(
                    settings
                        .code_actions_on_format
                        .iter()
                        .filter_map(|(action, enabled)| enabled.then_some(action))
                        .cloned()
                        .map(Formatter::CodeAction)
                        .collect::<Vec<_>>(),
                );
            }
        }

        let formatters = match (trigger, &settings.format_on_save) {
            (FormatTrigger::Save, FormatOnSave::Off) => &[],
            (FormatTrigger::Manual, _) | (FormatTrigger::Save, FormatOnSave::On) => {
                settings.formatter.as_ref()
            }
        };

        let formatters = code_actions_on_format_formatters
            .iter()
            .flatten()
            .chain(formatters);

        for formatter in formatters {
            let formatter = if formatter == &Formatter::Auto {
                if settings.prettier.allowed {
                    zlog::trace!(logger => "Formatter set to auto: defaulting to prettier");
                    &Formatter::Prettier
                } else {
                    zlog::trace!(logger => "Formatter set to auto: defaulting to primary language server");
                    &Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current)
                }
            } else {
                formatter
            };
            if let Err(err) = Self::apply_formatter(
                formatter,
                &lsp_store,
                buffer,
                formatting_transaction_id,
                &adapters_and_servers,
                &settings,
                request_timeout,
                logger,
                cx,
            )
            .await
            {
                zlog::error!(logger => "Formatter failed, skipping: {err:#}");
            }
        }

        Ok(())
    }
}
