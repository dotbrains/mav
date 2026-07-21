use super::*;

impl ProjectPanel {
    pub(super) fn restore_file(
        &mut self,
        action: &git::RestoreFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let selection = self.selection?;
            let project = self.project.read(cx);

            let (_worktree, entry) = self.selected_sub_entry(cx)?;
            if entry.is_dir() {
                return None;
            }

            let project_path = project.path_for_entry(selection.entry_id, cx)?;

            let git_store = project.git_store();
            let (repository, repo_path) = git_store
                .read(cx)
                .repository_and_path_for_project_path(&project_path, cx)?;

            let snapshot = repository.read(cx).snapshot();
            let status = snapshot.status_for_path(&repo_path)?;
            if !status.status.is_modified() && !status.status.is_deleted() {
                return None;
            }

            let file_name = entry.path.file_name()?.to_string();

            let answer = if !action.skip_prompt {
                let prompt = format!("Discard changes to {}?", file_name);
                Some(window.prompt(PromptLevel::Info, &prompt, None, &["Restore", "Cancel"], cx))
            } else {
                None
            };

            cx.spawn_in(window, async move |panel, cx| {
                if let Some(answer) = answer
                    && answer.await != Ok(0)
                {
                    return anyhow::Ok(());
                }

                let task = panel.update(cx, |_panel, cx| {
                    repository.update(cx, |repo, cx| {
                        repo.checkout_files("HEAD", vec![repo_path], cx)
                    })
                })?;

                if let Err(e) = task.await {
                    panel
                        .update(cx, |panel, cx| {
                            let message = format!("Failed to restore {}: {}", file_name, e);
                            let toast = StatusToast::new(message, cx, |this, _| {
                                this.icon(
                                    Icon::new(IconName::XCircle)
                                        .size(IconSize::Small)
                                        .color(Color::Error),
                                )
                                .dismiss_button(true)
                            });
                            panel
                                .workspace
                                .update(cx, |workspace, cx| {
                                    workspace.toggle_status_toast(toast, cx);
                                })
                                .ok();
                        })
                        .ok();
                }

                panel
                    .update(cx, |panel, cx| {
                        panel.project.update(cx, |project, cx| {
                            if let Some(buffer_id) = project
                                .buffer_store()
                                .read(cx)
                                .buffer_id_for_project_path(&project_path)
                            {
                                if let Some(buffer) = project.buffer_for_id(*buffer_id, cx) {
                                    buffer.update(cx, |buffer, cx| {
                                        let _ = buffer.reload(cx);
                                    });
                                }
                            }
                        })
                    })
                    .ok();

                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

            Some(())
        });
    }

    pub(super) fn add_to_gitignore(
        &mut self,
        _: &git::AddToGitignore,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let selection = self.selection?;
            let (_, entry) = self.selected_sub_entry(cx)?;
            let is_dir = entry.is_dir();
            let project = self.project.read(cx);

            let project_path = project.path_for_entry(selection.entry_id, cx)?;

            let git_store = project.git_store();
            let (repository, repo_path) = git_store
                .read(cx)
                .repository_and_path_for_project_path(&project_path, cx)?;

            let workspace = self.workspace.clone();
            let receiver =
                repository.update(cx, |repo, _| repo.add_path_to_gitignore(&repo_path, is_dir));

            cx.spawn(async move |_, cx| {
                if let Err(e) = receiver.await? {
                    if let Some(workspace) = workspace.upgrade() {
                        cx.update(|cx| {
                            let message = format!("Failed to add to .gitignore: {}", e);
                            let toast = StatusToast::new(message, cx, |this, _| {
                                this.icon(Icon::new(IconName::XCircle).color(Color::Error))
                                    .dismiss_button(true)
                            });
                            workspace.update(cx, |workspace, cx| {
                                workspace.toggle_status_toast(toast, cx);
                            });
                        });
                    }
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

            Some(())
        });
    }

    pub(super) fn add_to_git_info_exclude(
        &mut self,
        _: &git::AddToGitInfoExclude,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let selection = self.selection?;
            let (_, entry) = self.selected_sub_entry(cx)?;
            let is_dir = entry.is_dir();
            let project = self.project.read(cx);

            let project_path = project.path_for_entry(selection.entry_id, cx)?;

            let git_store = project.git_store();
            let (repository, repo_path) = git_store
                .read(cx)
                .repository_and_path_for_project_path(&project_path, cx)?;

            let workspace = self.workspace.clone();
            let receiver = repository.update(cx, |repo, _| {
                repo.add_path_to_git_info_exclude(&repo_path, is_dir)
            });

            cx.spawn(async move |_, cx| {
                if let Err(e) = receiver.await? {
                    if let Some(workspace) = workspace.upgrade() {
                        cx.update(|cx| {
                            let message = format!("Failed to add to .git/info/exclude: {}", e);
                            let toast = StatusToast::new(message, cx, |this, _| {
                                this.icon(Icon::new(IconName::XCircle).color(Color::Error))
                                    .dismiss_button(true)
                            });
                            workspace.update(cx, |workspace, cx| {
                                workspace.toggle_status_toast(toast, cx);
                            });
                        });
                    }
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

            Some(())
        });
    }
}
