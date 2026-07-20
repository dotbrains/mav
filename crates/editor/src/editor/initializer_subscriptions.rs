use super::*;

impl Editor {
    pub(super) fn project_subscriptions(
        full_mode: bool,
        project: Option<&Entity<Project>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let mut project_subscriptions = Vec::new();
        if !full_mode {
            return project_subscriptions;
        }
        let Some(project) = project else {
            return project_subscriptions;
        };

        project_subscriptions.push(cx.subscribe_in(
            project,
            window,
            |editor, _, event, window, cx| match event {
                project::Event::RefreshCodeLens => {
                    editor.refresh_code_lenses(None, window, cx);
                }
                project::Event::RefreshInlayHints {
                    server_id,
                    request_id,
                } => {
                    editor.refresh_inlay_hints(
                        InlayHintRefreshReason::RefreshRequested {
                            server_id: *server_id,
                            request_id: *request_id,
                        },
                        cx,
                    );
                }
                project::Event::RefreshSemanticTokens {
                    server_id,
                    request_id,
                } => {
                    editor.refresh_semantic_tokens(
                        None,
                        Some(RefreshForServer {
                            server_id: *server_id,
                            request_id: *request_id,
                        }),
                        cx,
                    );
                }
                project::Event::LanguageServerRemoved(_) => {
                    editor.registered_buffers.clear();
                    editor.register_visible_buffers(cx);
                    editor.invalidate_semantic_tokens(None);
                    editor.refresh_runnables(None, window, cx);
                    editor.update_lsp_data(None, window, cx);
                    editor.refresh_inlay_hints(InlayHintRefreshReason::ServerRemoved, cx);
                }
                project::Event::SnippetEdit(id, snippet_edits) => {
                    // todo(lw): Non singletons
                    if let Some(buffer) = editor.buffer.read(cx).as_singleton() {
                        let snapshot = buffer.read(cx).snapshot();
                        let focus_handle = editor.focus_handle(cx);
                        if snapshot.remote_id() == *id && focus_handle.is_focused(window) {
                            for (range, snippet) in snippet_edits {
                                let buffer_range =
                                    language::range_from_lsp(*range).to_offset(&snapshot);
                                editor
                                    .insert_snippet(
                                        &[MultiBufferOffset(buffer_range.start)
                                            ..MultiBufferOffset(buffer_range.end)],
                                        snippet.clone(),
                                        window,
                                        cx,
                                    )
                                    .ok();
                            }
                        }
                    }
                }
                project::Event::LanguageServerBufferRegistered { buffer_id, .. } => {
                    let buffer_id = *buffer_id;
                    if editor.buffer().read(cx).buffer(buffer_id).is_some() {
                        editor.register_buffer(buffer_id, cx);
                        editor.refresh_runnables(Some(buffer_id), window, cx);
                        editor.update_lsp_data(Some(buffer_id), window, cx);
                        editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
                        refresh_linked_ranges(editor, window, cx);
                        editor.refresh_code_actions_for_selection(window, cx);
                        editor.refresh_document_highlights(cx);
                    }
                }

                project::Event::EntryRenamed(transaction, project_path, abs_path) => {
                    let Some(workspace) = editor.workspace() else {
                        return;
                    };
                    let Some(active_editor) = workspace.read(cx).active_item_as::<Self>(cx) else {
                        return;
                    };

                    if active_editor.entity_id() == cx.entity_id() {
                        let entity_id = cx.entity_id();
                        workspace.update(cx, |this, cx| {
                            this.panes_mut()
                                .iter_mut()
                                .filter(|pane| pane.entity_id() != entity_id)
                                .for_each(|p| {
                                    p.update(cx, |pane, _| {
                                        pane.nav_history_mut().rename_item(
                                            entity_id,
                                            project_path.clone(),
                                            abs_path.clone().into(),
                                        );
                                    })
                                });
                        });

                        Self::open_transaction_for_hidden_buffers(
                            workspace,
                            transaction.clone(),
                            "Rename".to_string(),
                            window,
                            cx,
                        );
                    }
                }

                project::Event::WorkspaceEditApplied(transaction) => {
                    let Some(workspace) = editor.workspace() else {
                        return;
                    };
                    let Some(active_editor) = workspace.read(cx).active_item_as::<Self>(cx) else {
                        return;
                    };

                    if active_editor.entity_id() == cx.entity_id() {
                        Self::open_transaction_for_hidden_buffers(
                            workspace,
                            transaction.clone(),
                            "LSP Edit".to_string(),
                            window,
                            cx,
                        );
                    }
                }

                _ => {}
            },
        ));
        if let Some(task_inventory) = project
            .read(cx)
            .task_store()
            .read(cx)
            .task_inventory()
            .cloned()
        {
            project_subscriptions.push(cx.observe_in(
                &task_inventory,
                window,
                |editor, _, window, cx| {
                    editor.refresh_runnables(None, window, cx);
                },
            ));
        };

        project_subscriptions.push(cx.subscribe_in(
            &project.read(cx).breakpoint_store(),
            window,
            |editor, _, event, window, cx| match event {
                BreakpointStoreEvent::ClearDebugLines => {
                    editor.clear_row_highlights::<ActiveDebugLine>();
                    editor.refresh_inline_values(cx);
                }
                BreakpointStoreEvent::SetDebugLine => {
                    if editor.go_to_active_debug_line(window, cx) {
                        cx.stop_propagation();
                    }

                    editor.refresh_inline_values(cx);
                }
                _ => {}
            },
        ));
        let git_store = project.read(cx).git_store().clone();
        let project = project.clone();
        project_subscriptions.push(cx.subscribe(&git_store, move |this, _, event, cx| {
            if let GitStoreEvent::RepositoryAdded = event {
                this.load_diff_task = Some(
                    update_uncommitted_diff_for_buffer(
                        cx.entity(),
                        &project,
                        this.buffer.read(cx).all_buffers(),
                        this.buffer.clone(),
                        cx,
                    )
                    .shared(),
                );
            }
        }));

        project_subscriptions
    }
}
