use super::*;

impl AgentDiff {
    fn register_editor(
        &mut self,
        workspace: WeakEntity<Workspace>,
        buffer: WeakEntity<Buffer>,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_thread) = self.workspace_threads.get_mut(&workspace) else {
            return;
        };

        let weak_editor = editor.downgrade();

        workspace_thread
            .singleton_editors
            .entry(buffer.clone())
            .or_default()
            .entry(weak_editor.clone())
            .or_insert_with(|| {
                let workspace = workspace.clone();
                cx.observe_release(&editor, move |this, _, _cx| {
                    let Some(active_thread) = this.workspace_threads.get_mut(&workspace) else {
                        return;
                    };

                    if let Entry::Occupied(mut entry) =
                        active_thread.singleton_editors.entry(buffer)
                    {
                        let set = entry.get_mut();
                        set.remove(&weak_editor);

                        if set.is_empty() {
                            entry.remove();
                        }
                    }
                })
            });

        self.update_reviewing_editors(&workspace, window, cx);
    }

    fn update_reviewing_editors(
        &mut self,
        workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !AgentSettings::get_global(cx).single_file_review {
            for (editor, _) in self.reviewing_editors.drain() {
                editor
                    .update(cx, |editor, cx| {
                        editor.end_temporary_diff_override(cx);
                        editor.unregister_addon::<EditorAgentDiffAddon>();
                    })
                    .ok();
            }
            return;
        }

        let Some(workspace_thread) = self.workspace_threads.get_mut(workspace) else {
            return;
        };

        let Some(thread) = workspace_thread.thread.upgrade() else {
            return;
        };

        let action_log = thread.read(cx).action_log();
        let changed_buffers = action_log.read(cx).changed_buffers(cx).collect::<Vec<_>>();

        let mut unaffected = self.reviewing_editors.clone();

        for (buffer, diff_handle) in changed_buffers {
            if buffer.read(cx).file().is_none() {
                continue;
            }

            let Some(buffer_editors) = workspace_thread.singleton_editors.get(&buffer.downgrade())
            else {
                continue;
            };

            for weak_editor in buffer_editors.keys() {
                let Some(editor) = weak_editor.upgrade() else {
                    continue;
                };

                let multibuffer = editor.read(cx).buffer().clone();
                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.add_diff(diff_handle.clone(), cx);
                });

                let reviewing_state = EditorState::Reviewing;

                let previous_state = self
                    .reviewing_editors
                    .insert(weak_editor.clone(), reviewing_state.clone());

                if previous_state.is_none() {
                    editor.update(cx, |editor, cx| {
                        editor.start_temporary_diff_override();
                        editor.set_render_diff_hunk_controls(
                            diff_hunk_controls(&thread, workspace.clone()),
                            cx,
                        );
                        editor.set_render_diff_hunks_as_unstaged(true, cx);
                        editor.set_expand_all_diff_hunks(cx);
                        editor.register_addon(EditorAgentDiffAddon);
                    });
                } else {
                    unaffected.remove(weak_editor);
                }

                if reviewing_state == EditorState::Reviewing
                    && previous_state != Some(reviewing_state)
                {
                    // Jump to first hunk when we enter review mode
                    editor.update(cx, |editor, cx| {
                        let snapshot = multibuffer.read(cx).snapshot(cx);
                        if let Some(first_hunk) = snapshot.diff_hunks().next() {
                            let first_hunk_start = first_hunk.multi_buffer_range.start;

                            editor.change_selections(
                                SelectionEffects::scroll(Autoscroll::center()),
                                window,
                                cx,
                                |selections| {
                                    selections.select_ranges([first_hunk_start..first_hunk_start])
                                },
                            );
                        }
                    });
                }
            }
        }

        // Remove editors from this workspace that are no longer under review
        for (editor, _) in unaffected {
            // Note: We could avoid this check by storing `reviewing_editors` by Workspace,
            // but that would add another lookup in `AgentDiff::editor_state`
            // which gets called much more frequently.
            let in_workspace = editor
                .read_with(cx, |editor, _cx| editor.workspace())
                .ok()
                .flatten()
                .is_some_and(|editor_workspace| {
                    editor_workspace.entity_id() == workspace.entity_id()
                });

            if in_workspace {
                editor
                    .update(cx, |editor, cx| {
                        editor.end_temporary_diff_override(cx);
                        editor.unregister_addon::<EditorAgentDiffAddon>();
                    })
                    .ok();
                self.reviewing_editors.remove(&editor);
            }
        }

        cx.notify();
    }

    fn editor_state(&self, editor: &WeakEntity<Editor>) -> EditorState {
        self.reviewing_editors
            .get(editor)
            .cloned()
            .unwrap_or(EditorState::Idle)
    }

    fn deploy_pane_from_editor(&self, editor: &Entity<Editor>, window: &mut Window, cx: &mut App) {
        let Some(workspace) = editor.read(cx).workspace() else {
            return;
        };

        let Some(WorkspaceThread { thread, .. }) =
            self.workspace_threads.get(&workspace.downgrade())
        else {
            return;
        };

        let Some(thread) = thread.upgrade() else {
            return;
        };

        AgentDiffPane::deploy(thread, workspace.downgrade(), window, cx).log_err();
    }

    fn keep_all(
        editor: &Entity<Editor>,
        thread: &Entity<AcpThread>,
        _workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> PostReviewState {
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            keep_edits_in_ranges(
                editor,
                &snapshot,
                thread,
                vec![editor::Anchor::Min..editor::Anchor::Max],
                window,
                cx,
            );
        });
        PostReviewState::AllReviewed
    }

    fn reject_all(
        editor: &Entity<Editor>,
        thread: &Entity<AcpThread>,
        workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> PostReviewState {
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            reject_edits_in_ranges(
                editor,
                &snapshot,
                thread,
                vec![editor::Anchor::Min..editor::Anchor::Max],
                workspace.clone(),
                window,
                cx,
            );
        });
        PostReviewState::AllReviewed
    }

    fn keep(
        editor: &Entity<Editor>,
        thread: &Entity<AcpThread>,
        _workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> PostReviewState {
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            keep_edits_in_selection(editor, &snapshot, thread, window, cx);
            Self::post_review_state(&snapshot)
        })
    }

    fn reject(
        editor: &Entity<Editor>,
        thread: &Entity<AcpThread>,
        workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> PostReviewState {
        editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            reject_edits_in_selection(editor, &snapshot, thread, workspace.clone(), window, cx);
            Self::post_review_state(&snapshot)
        })
    }

    fn post_review_state(snapshot: &MultiBufferSnapshot) -> PostReviewState {
        for (i, _) in snapshot.diff_hunks().enumerate() {
            if i > 0 {
                return PostReviewState::Pending;
            }
        }
        PostReviewState::AllReviewed
    }

    fn review_in_active_editor(
        &mut self,
        workspace: &mut Workspace,
        review: impl Fn(
            &Entity<Editor>,
            &Entity<AcpThread>,
            &WeakEntity<Workspace>,
            &mut Window,
            &mut App,
        ) -> PostReviewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let active_item = workspace.active_item(cx)?;
        let editor = active_item.act_as::<Editor>(cx)?;

        if !matches!(
            self.editor_state(&editor.downgrade()),
            EditorState::Reviewing
        ) {
            return None;
        }

        let WorkspaceThread { thread, .. } =
            self.workspace_threads.get(&workspace.weak_handle())?;

        let thread = thread.upgrade()?;

        let review_result = review(&editor, &thread, &workspace.weak_handle(), window, cx);

        if matches!(review_result, PostReviewState::AllReviewed)
            && let Some(curr_buffer) = editor.read(cx).buffer().read(cx).as_singleton()
        {
            let changed_buffers = thread.read(cx).action_log().read(cx).changed_buffers(cx);

            let mut keys = changed_buffers.map(|(buffer, _)| buffer);
            keys.find(|k| *k == curr_buffer);
            let next_project_path = keys
                .next()
                .filter(|k| *k != curr_buffer)
                .and_then(|after| after.read(cx).project_path(cx));
            drop(keys);

            if let Some(path) = next_project_path {
                let task = workspace.open_path(path, None, true, window, cx);
                let task = cx.spawn(async move |_, _cx| task.await.map(|_| ()));
                return Some(task);
            }
        }

        Some(Task::ready(Ok(())))
    }
}

enum PostReviewState {
    AllReviewed,
    Pending,
}
