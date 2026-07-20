use super::*;

impl LocalLspStore {
    pub(super) async fn get_server_code_actions_from_action_kinds(
        lsp_store: &WeakEntity<LspStore>,
        language_server_id: LanguageServerId,
        code_action_kinds: Vec<lsp::CodeActionKind>,
        buffer: &Entity<Buffer>,
        cx: &mut AsyncApp,
    ) -> Result<Vec<CodeAction>> {
        let actions = lsp_store
            .update(cx, move |this, cx| {
                let request = GetCodeActions {
                    range: text::Anchor::min_max_range_for_buffer(buffer.read(cx).remote_id()),
                    kinds: Some(code_action_kinds),
                };
                let server = LanguageServerToQuery::Other(language_server_id);
                this.request_lsp(buffer.clone(), server, request, cx)
            })?
            .await?;
        Ok(actions)
    }

    pub async fn execute_code_actions_on_server(
        lsp_store: &WeakEntity<LspStore>,
        language_server: &Arc<LanguageServer>,
        actions: Vec<CodeAction>,
        push_to_history: bool,
        project_transaction: &mut ProjectTransaction,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        let request_timeout = cx.update(|app| {
            ProjectSettings::get_global(app)
                .global_lsp_settings
                .get_request_timeout()
        });

        for mut action in actions {
            Self::try_resolve_code_action(language_server, &mut action, request_timeout)
                .await
                .context("resolving a formatting code action")?;

            if let Some(edit) = action.lsp_action.edit() {
                if edit.changes.is_none() && edit.document_changes.is_none() {
                    continue;
                }

                let new = Self::deserialize_workspace_edit(
                    lsp_store.upgrade().context("project dropped")?,
                    edit.clone(),
                    push_to_history,
                    language_server.clone(),
                    cx,
                )
                .await?;
                project_transaction.0.extend(new.0);
            }

            let Some(command) = action.lsp_action.command() else {
                continue;
            };

            let server_capabilities = language_server.capabilities();
            let available_commands = server_capabilities
                .execute_command_provider
                .as_ref()
                .map(|options| options.commands.as_slice())
                .unwrap_or_default();
            if !available_commands.contains(&command.command) {
                log::warn!(
                    "Cannot execute a command {} not listed in the language server capabilities",
                    command.command
                );
                continue;
            }

            lsp_store.update(cx, |lsp_store, _| {
                if let LspStoreMode::Local(mode) = &mut lsp_store.mode {
                    mode.last_workspace_edits_by_language_server
                        .remove(&language_server.server_id());
                }
            })?;

            language_server
                .request::<lsp::request::ExecuteCommand>(
                    lsp::ExecuteCommandParams {
                        command: command.command.clone(),
                        arguments: command.arguments.clone().unwrap_or_default(),
                        ..Default::default()
                    },
                    request_timeout,
                )
                .await
                .into_response()
                .context("execute command")?;

            lsp_store.update(cx, |this, _| {
                if let LspStoreMode::Local(mode) = &mut this.mode {
                    project_transaction.0.extend(
                        mode.last_workspace_edits_by_language_server
                            .remove(&language_server.server_id())
                            .unwrap_or_default()
                            .0,
                    )
                }
            })?;
        }
        Ok(())
    }

    pub async fn deserialize_text_edits(
        this: Entity<LspStore>,
        buffer_to_edit: Entity<Buffer>,
        edits: Vec<lsp::TextEdit>,
        push_to_history: bool,
        _: Arc<CachedLspAdapter>,
        language_server: Arc<LanguageServer>,
        cx: &mut AsyncApp,
    ) -> Result<Option<Transaction>> {
        let edits = this
            .update(cx, |this, cx| {
                this.as_local_mut().unwrap().edits_from_lsp(
                    &buffer_to_edit,
                    edits,
                    language_server.server_id(),
                    None,
                    cx,
                )
            })
            .await?;

        let transaction = buffer_to_edit.update(cx, |buffer, cx| {
            buffer.finalize_last_transaction();
            buffer.start_transaction();
            for (range, text) in edits {
                buffer.edit([(range, text)], None, cx);
            }

            if buffer.end_transaction(cx).is_some() {
                let transaction = buffer.finalize_last_transaction().unwrap().clone();
                if !push_to_history {
                    buffer.forget_transaction(transaction.id);
                }
                Some(transaction)
            } else {
                None
            }
        });

        Ok(transaction)
    }

    #[allow(clippy::type_complexity)]
    pub fn edits_from_lsp(
        &mut self,
        buffer: &Entity<Buffer>,
        lsp_edits: impl 'static + Send + IntoIterator<Item = lsp::TextEdit>,
        server_id: LanguageServerId,
        version: Option<i32>,
        cx: &mut Context<LspStore>,
    ) -> Task<Result<Vec<(Range<Anchor>, Arc<str>)>>> {
        let snapshot = self.buffer_snapshot_for_lsp_version(buffer, server_id, version, cx);
        cx.background_spawn(async move {
            let snapshot = snapshot?;
            let mut lsp_edits = lsp_edits
                .into_iter()
                .map(|edit| (range_from_lsp(edit.range), edit.new_text))
                .collect::<Vec<_>>();

            lsp_edits.sort_unstable_by_key(|(range, _)| (range.start, range.end));

            let mut lsp_edits = lsp_edits.into_iter().peekable();
            let mut edits = Vec::new();
            while let Some((range, mut new_text)) = lsp_edits.next() {
                // Clip invalid ranges provided by the language server.
                let mut range = snapshot.clip_point_utf16(range.start, Bias::Left)
                    ..snapshot.clip_point_utf16(range.end, Bias::Left);

                // Combine any LSP edits that are adjacent.
                //
                // Also, combine LSP edits that are separated from each other by only
                // a newline. This is important because for some code actions,
                // Rust-analyzer rewrites the entire buffer via a series of edits that
                // are separated by unchanged newline characters.
                //
                // In order for the diffing logic below to work properly, any edits that
                // cancel each other out must be combined into one.
                while let Some((next_range, next_text)) = lsp_edits.peek() {
                    if next_range.start.0 > range.end {
                        if next_range.start.0.row > range.end.row + 1
                            || next_range.start.0.column > 0
                            || snapshot.clip_point_utf16(
                                Unclipped(PointUtf16::new(range.end.row, u32::MAX)),
                                Bias::Left,
                            ) > range.end
                        {
                            break;
                        }
                        new_text.push('\n');
                    }
                    range.end = snapshot.clip_point_utf16(next_range.end, Bias::Left);
                    new_text.push_str(next_text);
                    lsp_edits.next();
                }

                // For multiline edits, perform a diff of the old and new text so that
                // we can identify the changes more precisely, preserving the locations
                // of any anchors positioned in the unchanged regions.
                if range.end.row > range.start.row {
                    let offset = range.start.to_offset(&snapshot);
                    let old_text = snapshot.text_for_range(range).collect::<String>();
                    let range_edits = language::text_diff(old_text.as_str(), &new_text);
                    edits.extend(range_edits.into_iter().map(|(range, replacement)| {
                        (
                            snapshot.anchor_after(offset + range.start)
                                ..snapshot.anchor_before(offset + range.end),
                            replacement,
                        )
                    }));
                } else if range.end == range.start {
                    let anchor = snapshot.anchor_after(range.start);
                    edits.push((anchor..anchor, new_text.into()));
                } else {
                    let edit_start = snapshot.anchor_after(range.start);
                    let edit_end = snapshot.anchor_before(range.end);
                    edits.push((edit_start..edit_end, new_text.into()));
                }
            }

            Ok(edits)
        })
    }

    pub(crate) async fn deserialize_workspace_edit(
        this: Entity<LspStore>,
        edit: lsp::WorkspaceEdit,
        push_to_history: bool,
        language_server: Arc<LanguageServer>,
        cx: &mut AsyncApp,
    ) -> Result<ProjectTransaction> {
        let fs = this.read_with(cx, |this, _| this.as_local().unwrap().fs.clone());

        let mut operations = Vec::new();
        if let Some(document_changes) = edit.document_changes {
            match document_changes {
                lsp::DocumentChanges::Edits(edits) => {
                    operations.extend(edits.into_iter().map(lsp::DocumentChangeOperation::Edit))
                }
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

        let mut project_transaction = ProjectTransaction::default();
        for operation in operations {
            match operation {
                lsp::DocumentChangeOperation::Op(lsp::ResourceOp::Create(op)) => {
                    let abs_path = op
                        .uri
                        .to_file_path()
                        .map_err(|()| anyhow!("can't convert URI to path"))?;

                    if let Some(parent_path) = abs_path.parent() {
                        fs.create_dir(parent_path).await?;
                    }
                    if abs_path.ends_with("/") {
                        fs.create_dir(&abs_path).await?;
                    } else {
                        fs.create_file(
                            &abs_path,
                            op.options
                                .map(|options| fs::CreateOptions {
                                    overwrite: options.overwrite.unwrap_or(false),
                                    ignore_if_exists: options.ignore_if_exists.unwrap_or(false),
                                })
                                .unwrap_or_default(),
                        )
                        .await?;
                    }
                }

                lsp::DocumentChangeOperation::Op(lsp::ResourceOp::Rename(op)) => {
                    let source_abs_path = op
                        .old_uri
                        .to_file_path()
                        .map_err(|()| anyhow!("can't convert URI to path"))?;
                    let target_abs_path = op
                        .new_uri
                        .to_file_path()
                        .map_err(|()| anyhow!("can't convert URI to path"))?;

                    let options = fs::RenameOptions {
                        overwrite: op
                            .options
                            .as_ref()
                            .and_then(|options| options.overwrite)
                            .unwrap_or(false),
                        ignore_if_exists: op
                            .options
                            .as_ref()
                            .and_then(|options| options.ignore_if_exists)
                            .unwrap_or(false),
                        create_parents: true,
                    };

                    fs.rename(&source_abs_path, &target_abs_path, options)
                        .await?;
                }

                lsp::DocumentChangeOperation::Op(lsp::ResourceOp::Delete(op)) => {
                    let abs_path = op
                        .uri
                        .to_file_path()
                        .map_err(|()| anyhow!("can't convert URI to path"))?;
                    let options = op
                        .options
                        .map(|options| fs::RemoveOptions {
                            recursive: options.recursive.unwrap_or(false),
                            ignore_if_not_exists: options.ignore_if_not_exists.unwrap_or(false),
                        })
                        .unwrap_or_default();
                    if abs_path.ends_with("/") {
                        fs.remove_dir(&abs_path, options).await?;
                    } else {
                        fs.remove_file(&abs_path, options).await?;
                    }
                }

                lsp::DocumentChangeOperation::Edit(op) => {
                    let buffer_to_edit = this
                        .update(cx, |this, cx| {
                            this.open_local_buffer_via_lsp(
                                op.text_document.uri.clone(),
                                language_server.server_id(),
                                cx,
                            )
                        })
                        .await?;

                    let edits = this
                        .update(cx, |this, cx| {
                            let path = buffer_to_edit.read(cx).project_path(cx);
                            let active_entry = this.active_entry;
                            let is_active_entry = path.is_some_and(|project_path| {
                                this.worktree_store
                                    .read(cx)
                                    .entry_for_path(&project_path, cx)
                                    .is_some_and(|entry| Some(entry.id) == active_entry)
                            });
                            let local = this.as_local_mut().unwrap();

                            let (mut edits, mut snippet_edits) = (vec![], vec![]);
                            for edit in op.edits {
                                match edit {
                                    Edit::Plain(edit) => {
                                        if !edits.contains(&edit) {
                                            edits.push(edit)
                                        }
                                    }
                                    Edit::Annotated(edit) => {
                                        if !edits.contains(&edit.text_edit) {
                                            edits.push(edit.text_edit)
                                        }
                                    }
                                    Edit::Snippet(edit) => {
                                        let Ok(snippet) = Snippet::parse(&edit.snippet.value)
                                        else {
                                            continue;
                                        };

                                        if is_active_entry {
                                            snippet_edits.push((edit.range, snippet));
                                        } else {
                                            // Since this buffer is not focused, apply a normal edit.
                                            let new_edit = TextEdit {
                                                range: edit.range,
                                                new_text: snippet.text,
                                            };
                                            if !edits.contains(&new_edit) {
                                                edits.push(new_edit);
                                            }
                                        }
                                    }
                                }
                            }
                            if !snippet_edits.is_empty() {
                                let buffer_id = buffer_to_edit.read(cx).remote_id();
                                let version = if let Some(buffer_version) = op.text_document.version
                                {
                                    local
                                        .buffer_snapshot_for_lsp_version(
                                            &buffer_to_edit,
                                            language_server.server_id(),
                                            Some(buffer_version),
                                            cx,
                                        )
                                        .ok()
                                        .map(|snapshot| snapshot.version)
                                } else {
                                    Some(buffer_to_edit.read(cx).saved_version().clone())
                                };

                                let most_recent_edit =
                                    version.and_then(|version| version.most_recent());
                                // Check if the edit that triggered that edit has been made by this participant.

                                if let Some(most_recent_edit) = most_recent_edit {
                                    cx.emit(LspStoreEvent::SnippetEdit {
                                        buffer_id,
                                        edits: snippet_edits,
                                        most_recent_edit,
                                    });
                                }
                            }

                            local.edits_from_lsp(
                                &buffer_to_edit,
                                edits,
                                language_server.server_id(),
                                op.text_document.version,
                                cx,
                            )
                        })
                        .await?;

                    let transaction = buffer_to_edit.update(cx, |buffer, cx| {
                        buffer.finalize_last_transaction();
                        buffer.start_transaction();
                        for (range, text) in edits {
                            buffer.edit([(range, text)], None, cx);
                        }

                        buffer.end_transaction(cx).and_then(|transaction_id| {
                            if push_to_history {
                                buffer.finalize_last_transaction();
                                buffer.get_transaction(transaction_id).cloned()
                            } else {
                                buffer.forget_transaction(transaction_id)
                            }
                        })
                    });
                    if let Some(transaction) = transaction {
                        project_transaction.0.insert(buffer_to_edit, transaction);
                    }
                }
            }
        }

        Ok(project_transaction)
    }

    pub(super) async fn on_lsp_workspace_edit(
        this: WeakEntity<LspStore>,
        params: lsp::ApplyWorkspaceEditParams,
        server_id: LanguageServerId,
        cx: &mut AsyncApp,
    ) -> Result<lsp::ApplyWorkspaceEditResponse> {
        let this = this.upgrade().context("project project closed")?;
        let language_server = this
            .read_with(cx, |this, _| this.language_server_for_id(server_id))
            .context("language server not found")?;
        let transaction = Self::deserialize_workspace_edit(
            this.clone(),
            params.edit,
            true,
            language_server.clone(),
            cx,
        )
        .await
        .log_err();
        this.update(cx, |this, cx| {
            if let Some(transaction) = transaction {
                cx.emit(LspStoreEvent::WorkspaceEditApplied(transaction.clone()));

                this.as_local_mut()
                    .unwrap()
                    .last_workspace_edits_by_language_server
                    .insert(server_id, transaction);
            }
        });
        Ok(lsp::ApplyWorkspaceEditResponse {
            applied: true,
            failed_change: None,
            failure_reason: None,
        })
    }
}
