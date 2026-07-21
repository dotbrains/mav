use super::*;

impl LocalLspStore {
    pub(super) async fn apply_code_action_formatter(
        code_action_name: &str,
        lsp_store: &WeakEntity<LspStore>,
        buffer: &FormattableBuffer,
        formatting_transaction_id: clock::Lamport,
        adapters_and_servers: &[(Arc<CachedLspAdapter>, Arc<LanguageServer>)],
        request_timeout: Duration,
        logger: zlog::Logger,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        let logger = zlog::scoped!(logger => "code-actions");
        zlog::trace!(logger => "formatting");
        let _timer = zlog::time!(logger => "Formatting buffer using code actions");

        let Some(buffer_path_abs) = buffer.abs_path.as_ref() else {
            zlog::warn!(logger => "Cannot format buffer that is not backed by a file on disk using code actions. Skipping");
            return Ok(());
        };

        let code_action_kind: CodeActionKind = code_action_name.to_string().into();
        zlog::trace!(logger => "Attempting to resolve code actions {:?}", &code_action_kind);

        let mut actions_and_servers = Vec::new();

        for (index, (_, language_server)) in adapters_and_servers.iter().enumerate() {
            let actions_result = Self::get_server_code_actions_from_action_kinds(
                &lsp_store,
                language_server.server_id(),
                vec![code_action_kind.clone()],
                &buffer.handle,
                cx,
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to resolve code action {:?} with language server {}",
                    code_action_kind,
                    language_server.name()
                )
            });
            let Ok(actions) = actions_result else {
                // note: it may be better to set result to the error and break formatters here
                // but for now we try to execute the actions that we can resolve and skip the rest
                zlog::error!(
                    logger =>
                    "Failed to resolve code action {:?} with language server {}",
                    code_action_kind,
                    language_server.name()
                );
                continue;
            };
            for action in actions {
                actions_and_servers.push((action, index));
            }
        }

        if actions_and_servers.is_empty() {
            zlog::warn!(logger => "No code actions were resolved, continuing");
            return Ok(());
        }

        'actions: for (mut action, server_index) in actions_and_servers {
            let server = &adapters_and_servers[server_index].1;

            let describe_code_action = |action: &CodeAction| {
                format!(
                    "code action '{}' with title \"{}\" on server {}",
                    action
                        .lsp_action
                        .action_kind()
                        .unwrap_or("unknown".into())
                        .as_str(),
                    action.lsp_action.title(),
                    server.name(),
                )
            };

            zlog::trace!(logger => "Executing {}", describe_code_action(&action));

            if let Err(err) =
                Self::try_resolve_code_action(server, &mut action, request_timeout).await
            {
                zlog::error!(
                    logger =>
                    "Failed to resolve {}. Error: {}",
                    describe_code_action(&action),
                    err
                );
                continue;
            }

            if let Some(edit) = action.lsp_action.edit().cloned() {
                // NOTE: code below duplicated from `Self::deserialize_workspace_edit`
                // but filters out and logs warnings for code actions that require unreasonably
                // difficult handling on our part, such as:
                // - applying edits that call commands
                //   which can result in arbitrary workspace edits being sent from the server that
                //   have no way of being tied back to the command that initiated them (i.e. we
                //   can't know which edits are part of the format request, or if the server is done sending
                //   actions in response to the command)
                // - actions that create/delete/modify/rename files other than the one we are formatting
                //   as we then would need to handle such changes correctly in the local history as well
                //   as the remote history through the ProjectTransaction
                // - actions with snippet edits, as these simply don't make sense in the context of a format request
                // Supporting these actions is not impossible, but not supported as of yet.
                if edit.changes.is_none() && edit.document_changes.is_none() {
                    zlog::trace!(
                        logger =>
                        "No changes for code action. Skipping {}",
                        describe_code_action(&action),
                    );
                    continue;
                }

                let mut operations = Vec::new();
                if let Some(document_changes) = edit.document_changes {
                    match document_changes {
                        lsp::DocumentChanges::Edits(edits) => operations
                            .extend(edits.into_iter().map(lsp::DocumentChangeOperation::Edit)),
                        lsp::DocumentChanges::Operations(ops) => operations = ops,
                    }
                } else if let Some(changes) = edit.changes {
                    operations.extend(changes.into_iter().map(|(uri, edits)| {
                        lsp::DocumentChangeOperation::Edit(lsp::TextDocumentEdit {
                            text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                                uri,
                                version: None,
                            },
                            edits: edits.into_iter().map(Edit::Plain).collect(),
                        })
                    }));
                }

                let mut edits = Vec::with_capacity(operations.len());

                if operations.is_empty() {
                    zlog::trace!(
                        logger =>
                        "No changes for code action. Skipping {}",
                        describe_code_action(&action),
                    );
                    continue;
                }
                for operation in operations {
                    let op = match operation {
                        lsp::DocumentChangeOperation::Edit(op) => op,
                        lsp::DocumentChangeOperation::Op(_) => {
                            zlog::warn!(
                                logger =>
                                "Code actions which create, delete, or rename files are not supported on format. Skipping {}",
                                describe_code_action(&action),
                            );
                            continue 'actions;
                        }
                    };
                    let Ok(file_path) = op.text_document.uri.to_file_path() else {
                        zlog::warn!(
                            logger =>
                            "Failed to convert URI '{:?}' to file path. Skipping {}",
                            &op.text_document.uri,
                            describe_code_action(&action),
                        );
                        continue 'actions;
                    };
                    if &file_path != buffer_path_abs {
                        zlog::warn!(
                            logger =>
                            "File path '{:?}' does not match buffer path '{:?}'. Skipping {}",
                            file_path,
                            buffer_path_abs,
                            describe_code_action(&action),
                        );
                        continue 'actions;
                    }

                    let mut lsp_edits = Vec::new();
                    for edit in op.edits {
                        match edit {
                            Edit::Plain(edit) => {
                                if !lsp_edits.contains(&edit) {
                                    lsp_edits.push(edit);
                                }
                            }
                            Edit::Annotated(edit) => {
                                if !lsp_edits.contains(&edit.text_edit) {
                                    lsp_edits.push(edit.text_edit);
                                }
                            }
                            Edit::Snippet(_) => {
                                zlog::warn!(
                                    logger =>
                                    "Code actions which produce snippet edits are not supported during formatting. Skipping {}",
                                    describe_code_action(&action),
                                );
                                continue 'actions;
                            }
                        }
                    }
                    let edits_result = lsp_store
                        .update(cx, |lsp_store, cx| {
                            lsp_store.as_local_mut().unwrap().edits_from_lsp(
                                &buffer.handle,
                                lsp_edits,
                                server.server_id(),
                                op.text_document.version,
                                cx,
                            )
                        })?
                        .await;
                    let Ok(resolved_edits) = edits_result else {
                        zlog::warn!(
                            logger =>
                            "Failed to resolve edits from LSP for buffer {:?} while handling {}",
                            buffer_path_abs.as_path(),
                            describe_code_action(&action),
                        );
                        continue 'actions;
                    };
                    edits.extend(resolved_edits);
                }

                if edits.is_empty() {
                    zlog::warn!(logger => "No edits resolved from LSP");
                    continue;
                }

                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        zlog::info!("Applying edits {edits:?}. Content: {:?}", buffer.text());
                        buffer.edit(edits, None, cx);
                        zlog::info!("Applied edits. New Content: {:?}", buffer.text());
                    },
                )?;
            }

            let Some(command) = action.lsp_action.command() else {
                continue;
            };

            zlog::warn!(
                logger =>
                "Executing code action command '{}'. This may cause formatting to abort unnecessarily as well as splitting formatting into two entries in the undo history",
                &command.command,
            );

            let server_capabilities = server.capabilities();
            let available_commands = server_capabilities
                .execute_command_provider
                .as_ref()
                .map(|options| options.commands.as_slice())
                .unwrap_or_default();
            if !available_commands.contains(&command.command) {
                zlog::warn!(
                    logger =>
                    "Cannot execute a command {} not listed in the language server capabilities of server {}",
                    command.command,
                    server.name(),
                );
                continue;
            }

            extend_formatting_transaction(buffer, formatting_transaction_id, cx, |_, _| {})?;
            zlog::info!(logger => "Executing command {}", &command.command);

            lsp_store.update(cx, |this, _| {
                this.as_local_mut()
                    .unwrap()
                    .last_workspace_edits_by_language_server
                    .remove(&server.server_id());
            })?;

            let execute_command_result = server
                .request::<lsp::request::ExecuteCommand>(
                    lsp::ExecuteCommandParams {
                        command: command.command.clone(),
                        arguments: command.arguments.clone().unwrap_or_default(),
                        ..Default::default()
                    },
                    request_timeout,
                )
                .await
                .into_response();

            if execute_command_result.is_err() {
                zlog::error!(
                    logger =>
                    "Failed to execute command '{}' as part of {}",
                    &command.command,
                    describe_code_action(&action),
                );
                continue 'actions;
            }

            let mut project_transaction_command = lsp_store.update(cx, |this, _| {
                this.as_local_mut()
                    .unwrap()
                    .last_workspace_edits_by_language_server
                    .remove(&server.server_id())
                    .unwrap_or_default()
            })?;

            if let Some(transaction) = project_transaction_command.0.remove(&buffer.handle) {
                zlog::trace!(
                    logger =>
                    "Successfully captured {} edits that resulted from command {}",
                    transaction.edit_ids.len(),
                    &command.command,
                );
                let transaction_id_project_transaction = transaction.id;
                buffer.handle.update(cx, |buffer, _| {
                    // it may have been removed from history if push_to_history was
                    // false in deserialize_workspace_edit. If so push it so we
                    // can merge it with the format transaction
                    // and pop the combined transaction off the history stack
                    // later if push_to_history is false
                    if buffer.get_transaction(transaction.id).is_none() {
                        buffer.push_transaction(transaction, Instant::now());
                    }
                    buffer.merge_transactions(
                        transaction_id_project_transaction,
                        formatting_transaction_id,
                    );
                });
            }

            if project_transaction_command.0.is_empty() {
                continue;
            }

            let mut extra_buffers = String::new();
            for buffer in project_transaction_command.0.keys() {
                buffer.read_with(cx, |b, cx| {
                    let Some(path) = b.project_path(cx) else {
                        return;
                    };

                    if !extra_buffers.is_empty() {
                        extra_buffers.push_str(", ");
                    }
                    extra_buffers.push_str(path.path.as_unix_str());
                });
            }
            zlog::warn!(
                logger =>
                "Unexpected edits to buffers other than the buffer actively being formatted due to command {}. Impacted buffers: [{}].",
                &command.command,
                extra_buffers,
            );
            // NOTE: if this case is hit, the proper thing to do is to for each buffer, merge the extra transaction
            // into the existing transaction in project_transaction if there is one, and if there isn't one in project_transaction,
            // add it so it's included, and merge it into the format transaction when its created later
        }

        Ok(())
    }
}
