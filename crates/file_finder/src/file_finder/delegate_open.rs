use super::*;

impl FileFinderDelegate {
    pub(super) fn calculate_selected_index(&self, cx: &mut Context<Picker<Self>>) -> usize {
        if FileFinderSettings::get_global(cx).skip_focus_for_active_in_search
            && let Some(Match::History { path, .. }) = self.matches.get(0)
            && Some(path) == self.currently_opened_path.as_ref()
        {
            let elements_after_first = self.matches.len() - 1;
            if elements_after_first > 0 {
                return 1;
            }
        }

        0
    }

    pub(super) fn key_context(&self, _window: &Window, _cx: &App) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("FileFinder");
        key_context
    }

    /// Shared file-opening logic for both `confirm` and `confirm_without_dismiss`.
    ///
    /// When `dismiss_after_open` is true this behaves like a normal confirm: the file is focused
    /// and the finder is dismissed.  When false the finder stays open so the user can continue
    /// opening more files.
    pub(super) fn open_selected_file(
        &mut self,
        secondary: bool,
        dismiss_after_open: bool,
        window: &mut Window,
        cx: &mut Context<Picker<FileFinderDelegate>>,
    ) {
        let Some(m) = self.matches.get(self.selected_index()).cloned() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        // Channel matches always dismiss the finder.
        if let Match::Channel { channel_id, .. } = &m {
            let channel_id = channel_id.0;
            let finder = self.file_finder.clone();
            window.dispatch_action(OpenChannelNotesById { channel_id }.boxed_clone(), cx);
            finder.update(cx, |_, cx| cx.emit(DismissEvent)).log_err();
            return;
        }

        // Focus the new item only when dismissing — this avoids stealing focus from the modal.
        // Always activate (make the tab current) so every opened file is visually reflected.
        let focus_item = dismiss_after_open;

        let open_task = workspace.update(cx, |workspace, cx| {
            let split_or_open = |workspace: &mut Workspace,
                                 project_path,
                                 window: &mut Window,
                                 cx: &mut Context<Workspace>| {
                let allow_preview =
                    PreviewTabsSettings::get_global(cx).enable_preview_from_file_finder;
                if secondary {
                    workspace.split_path_preview(project_path, allow_preview, None, window, cx)
                } else {
                    workspace.open_path_preview_in_tabbed_pane(
                        project_path,
                        None,
                        focus_item,
                        allow_preview,
                        true,
                        window,
                        cx,
                    )
                }
            };

            match &m {
                Match::CreateNew(project_path) => {
                    if secondary {
                        workspace.split_path_preview(project_path.clone(), false, None, window, cx)
                    } else {
                        workspace.open_path_preview_in_tabbed_pane(
                            project_path.clone(),
                            None,
                            focus_item,
                            false,
                            true,
                            window,
                            cx,
                        )
                    }
                }
                Match::History { path, .. } => {
                    let worktree_id = path.project.worktree_id;
                    if workspace
                        .project()
                        .read(cx)
                        .worktree_for_id(worktree_id, cx)
                        .is_some()
                    {
                        split_or_open(
                            workspace,
                            ProjectPath {
                                worktree_id,
                                path: Arc::clone(&path.project.path),
                            },
                            window,
                            cx,
                        )
                    } else if secondary {
                        workspace.split_abs_path(path.absolute.clone(), false, window, cx)
                    } else {
                        workspace.open_abs_path(
                            path.absolute.clone(),
                            OpenOptions {
                                visible: Some(OpenVisible::None),
                                ..Default::default()
                            },
                            window,
                            cx,
                        )
                    }
                }
                Match::Search(path_match) => {
                    let project_path =
                        project_path_for_search_match(workspace.project(), &path_match.0, cx);
                    split_or_open(workspace, project_path, window, cx)
                }
                Match::Channel { .. } => unreachable!("handled above"),
            }
        });

        let selection_query = self.latest_search_query.clone();
        let finder = self.file_finder.clone();
        let workspace = self.workspace.clone();

        cx.spawn_in(window, async move |_, mut cx| {
            let item = open_task
                .await
                .notify_workspace_async_err(workspace, &mut cx)?;
            if let Some(active_editor) = item.downcast::<Editor>() {
                active_editor
                    .downgrade()
                    .update_in(cx, |editor, window, cx| {
                        let Some(buffer) = editor.buffer().read(cx).as_singleton() else {
                            return;
                        };
                        let buffer_snapshot = buffer.read(cx).snapshot();
                        let Some(selection_query) = selection_query.as_ref() else {
                            return;
                        };
                        let Some(selection_range) =
                            selection_query.selection_range(&buffer_snapshot)
                        else {
                            return;
                        };
                        editor.go_to_singleton_buffer_range(selection_range, window, cx);
                    })
                    .log_err();
            }
            if dismiss_after_open {
                finder.update(cx, |_, cx| cx.emit(DismissEvent)).ok()?;
            }
            Some(())
        })
        .detach();
    }

    pub(super) fn confirm_without_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Picker<FileFinderDelegate>>,
    ) {
        self.open_selected_file(false, false, window, cx);
    }
}
