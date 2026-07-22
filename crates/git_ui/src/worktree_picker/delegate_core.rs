use super::*;

impl WorktreePickerDelegate {
    fn build_fixed_entries(&self) -> Vec<WorktreeEntry> {
        worktree_create_targets(
            self.has_multiple_repositories,
            self.default_branch.clone(),
            self.current_branch_name.as_deref(),
        )
        .into_iter()
        .map(|target| match target {
            WorktreeCreateTarget::CurrentBranch => WorktreeEntry::CreateFromCurrentBranch,
            WorktreeCreateTarget::DefaultBranch(default_branch) => {
                WorktreeEntry::CreateFromDefaultBranch { default_branch }
            }
        })
        .collect()
    }

    fn all_repo_worktrees(&self) -> &[GitWorktree] {
        &self.all_worktrees
    }

    fn creation_blocked_reason(&self, cx: &App) -> Option<SharedString> {
        let project = self.project.read(cx);
        if project.is_via_collab() {
            Some("Worktree creation is not supported in collaborative projects".into())
        } else if project.repositories(cx).is_empty() {
            Some("Requires a Git repository in the project".into())
        } else {
            None
        }
    }

    fn can_delete_worktree(&self, worktree: &GitWorktree) -> bool {
        !worktree.is_main && !self.project_worktree_paths.contains(&worktree.path)
    }

    fn refresh_project_worktree_paths(&mut self, window: &mut Window, cx: &mut App) {
        let mut paths = self.active_worktree_paths.clone();

        if let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten()
            && let Some(workspace) = self.workspace.upgrade()
        {
            let group_key = workspace.read(cx).project_group_key(cx);
            if let Some(group_workspaces) = multi_workspace
                .read(cx)
                .workspaces_for_project_group(&group_key, cx)
            {
                for group_workspace in group_workspaces {
                    for worktree in group_workspace
                        .read(cx)
                        .project()
                        .read(cx)
                        .visible_worktrees(cx)
                    {
                        paths.insert(worktree.read(cx).abs_path().to_path_buf());
                    }
                }
            }
        }

        self.project_worktree_paths = paths;
    }

    fn is_force_delete_hovering_index(&self, index: usize) -> bool {
        self.modifiers.alt && self.hovered_delete_index == Some(index)
    }

    fn delete_worktree(
        &mut self,
        ix: usize,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(entry) = self.matches.get(ix) else {
            return;
        };
        let WorktreeEntry::Worktree { worktree, .. } = entry else {
            return;
        };
        if !self.can_delete_worktree(worktree)
            || self.deleting_worktree_paths.contains(&worktree.path)
        {
            return;
        }

        let repo = self.project.read(cx).active_repository(cx);
        let Some(repo) = repo else {
            return;
        };
        let path = worktree.path.clone();
        let display_name = worktree.directory_name(
            self.all_worktrees
                .iter()
                .find(|worktree| worktree.is_main)
                .map(|worktree| worktree.path.as_path()),
        );
        let workspace = self.workspace.clone();

        self.deleting_worktree_paths.insert(path.clone());
        if self.hovered_delete_index == Some(ix) {
            self.hovered_delete_index = None;
        }
        cx.notify();

        cx.spawn_in(window, async move |picker, cx| {
            let initial_result = match repo
                .update(cx, |repo, _| repo.remove_worktree(path.clone(), force))
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    picker.update_in(cx, |picker, _window, cx| {
                        if picker.delegate.deleting_worktree_paths.remove(&path) {
                            cx.notify();
                        }
                    })?;
                    return Err(error.into());
                }
            };

            let (result, attempted_force) = match initial_result {
                Ok(()) => (Ok(()), force),
                Err(error) => {
                    log::error!("Failed to remove worktree: {}", error);

                    let force_delete_prompt = (!force)
                        .then(|| {
                            force_delete_prompt_for_worktree_remove_error(&error, &display_name)
                        })
                        .flatten();

                    if let Some(prompt_message) = force_delete_prompt {
                        picker.update_in(cx, |picker, _window, cx| {
                            if picker.delegate.deleting_worktree_paths.remove(&path) {
                                cx.notify();
                            }
                        })?;

                        let answer = cx.update(|window, cx| {
                            window.prompt(
                                PromptLevel::Warning,
                                &prompt_message,
                                None,
                                &["Force Delete", "Cancel"],
                                cx,
                            )
                        })?;

                        if answer.await != Ok(0) {
                            return Ok(());
                        }

                        let should_retry = picker.update_in(cx, |picker, _window, cx| {
                            let worktree_still_exists = picker
                                .delegate
                                .all_worktrees
                                .iter()
                                .any(|worktree| worktree.path == path);
                            if !worktree_still_exists
                                || !picker.delegate.deleting_worktree_paths.insert(path.clone())
                            {
                                return false;
                            }
                            cx.notify();
                            true
                        })?;

                        if !should_retry {
                            return Ok(());
                        }

                        let retry = match repo
                            .update(cx, |repo, _| repo.remove_worktree(path.clone(), true))
                            .await
                        {
                            Ok(result) => result,
                            Err(error) => {
                                picker.update_in(cx, |picker, _window, cx| {
                                    if picker.delegate.deleting_worktree_paths.remove(&path) {
                                        cx.notify();
                                    }
                                })?;
                                return Err(error.into());
                            }
                        };

                        if let Err(error) = &retry {
                            log::error!("Failed to force remove worktree: {error}");
                        }

                        (retry, true)
                    } else {
                        (Err(error), force)
                    }
                }
            };

            if let Err(error) = result {
                picker.update_in(cx, |picker, _window, cx| {
                    if picker.delegate.deleting_worktree_paths.remove(&path) {
                        cx.notify();
                    }
                })?;

                if let Some(workspace) = workspace.upgrade() {
                    cx.update(|_window, cx| {
                        show_error_toast(
                            workspace,
                            remove_worktree_command(&path, attempted_force),
                            error,
                            cx,
                        )
                    })?;
                }

                return Ok(());
            }

            picker.update_in(cx, |picker, _window, cx| {
                picker.delegate.deleting_worktree_paths.remove(&path);
                picker.delegate.matches.retain(|e| {
                    !matches!(e, WorktreeEntry::Worktree { worktree, .. } if worktree.path == path)
                });
                picker.delegate.all_worktrees.retain(|w| w.path != path);
                if picker.delegate.matches.is_empty() {
                    picker.delegate.selected_index = 0;
                } else if picker.delegate.selected_index >= picker.delegate.matches.len() {
                    picker.delegate.selected_index = picker.delegate.matches.len() - 1;
                }
                picker.delegate.hovered_delete_index = None;
                cx.notify();
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Finds the workspace in this window (other than the picker's own
    /// workspace) that has `worktree_path` open as a visible worktree.
    fn workspace_for_open_worktree(
        &self,
        worktree_path: &Path,
        window: &Window,
        cx: &App,
    ) -> Option<Entity<Workspace>> {
        if self.active_worktree_paths.contains(worktree_path) {
            return None;
        }
        let multi_workspace = window.root::<MultiWorkspace>().flatten()?;
        let workspace = self.workspace.upgrade()?;
        let group_key = workspace.read(cx).project_group_key(cx);
        multi_workspace
            .read(cx)
            .workspaces_for_project_group(&group_key, cx)?
            .into_iter()
            .find(|group_workspace| {
                *group_workspace != workspace
                    && group_workspace
                        .read(cx)
                        .project()
                        .read(cx)
                        .visible_worktrees(cx)
                        .any(|worktree| worktree.read(cx).abs_path().as_ref() == worktree_path)
            })
    }

    fn remove_worktree_from_window(
        &mut self,
        worktree_path: &Path,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        if self.deleting_worktree_paths.contains(worktree_path) {
            return;
        }
        let Some(workspace_to_remove) = self.workspace_for_open_worktree(worktree_path, window, cx)
        else {
            return;
        };
        let Some(window_handle) = window.window_handle().downcast::<MultiWorkspace>() else {
            return;
        };

        cx.spawn_in(window, async move |picker, cx| {
            let removed = window_handle
                .update(cx, |multi_workspace, window, cx| {
                    multi_workspace.close_workspace(&workspace_to_remove, window, cx)
                })?
                .await?;

            if removed {
                picker.update_in(cx, |picker, window, cx| {
                    picker.delegate.refresh_project_worktree_paths(window, cx);
                    picker.refresh(window, cx);
                })?;
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn sync_selected_index(&mut self, has_query: bool) {
        if !has_query {
            return;
        }

        if let Some(index) = self
            .matches
            .iter()
            .position(|entry| matches!(entry, WorktreeEntry::Worktree { .. }))
        {
            self.selected_index = index;
        } else if let Some(index) = self
            .matches
            .iter()
            .position(|entry| matches!(entry, WorktreeEntry::CreateNamed { .. }))
        {
            self.selected_index = index;
        } else {
            self.selected_index = 0;
        }
    }
}
