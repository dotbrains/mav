use super::*;

impl GitPanel {
    pub(super) fn revert_selected(
        &mut self,
        action: &git::RestoreFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_style = self.project.read(cx).path_style(cx);
        maybe!({
            let list_entry = self.entries.get(self.selected_entry?)?.clone();
            let entry = list_entry.status_entry()?.to_owned();
            let skip_prompt = action.skip_prompt || entry.status.is_created();

            let prompt = if skip_prompt {
                Task::ready(Ok(0))
            } else {
                let prompt = window.prompt(
                    PromptLevel::Warning,
                    &format!(
                        "Are you sure you want to discard changes to {}?",
                        MarkdownInlineCode(
                            entry
                                .repo_path
                                .file_name()
                                .unwrap_or(entry.repo_path.display(path_style).as_ref())
                        ),
                    ),
                    None,
                    &["Discard Changes", "Cancel"],
                    cx,
                );
                cx.background_spawn(prompt)
            };

            let this = cx.weak_entity();
            window
                .spawn(cx, async move |cx| {
                    if prompt.await? != 0 {
                        return anyhow::Ok(());
                    }

                    this.update_in(cx, |this, window, cx| {
                        this.revert_entry(&entry, window, cx);
                    })?;

                    Ok(())
                })
                .detach();
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
            let list_entry = self.entries.get(self.selected_entry?)?.clone();
            let entry = list_entry.status_entry()?.to_owned();

            if !entry.status.is_created() {
                return Some(());
            }

            let active_repository = self.active_repository.clone()?;
            let workspace = self.workspace.clone();
            let repo_path = entry.repo_path;

            let receiver = active_repository
                .update(cx, |repo, _| repo.add_path_to_gitignore(&repo_path, false));

            cx.spawn(async move |_, cx| {
                if let Err(e) = receiver.await? {
                    if let Some(workspace) = workspace.upgrade() {
                        cx.update(|cx| {
                            show_error_toast(workspace, "add to .gitignore", e, cx);
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
            let list_entry = self.entries.get(self.selected_entry?)?.clone();
            let entry = list_entry.status_entry()?.to_owned();

            if !entry.status.is_created() {
                return Some(());
            }

            let active_repository = self.active_repository.clone()?;
            let workspace = self.workspace.clone();
            let repo_path = entry.repo_path;

            let receiver = active_repository.update(cx, |repo, _| {
                repo.add_path_to_git_info_exclude(&repo_path, false)
            });

            cx.spawn(async move |_, cx| {
                if let Err(e) = receiver.await? {
                    if let Some(workspace) = workspace.upgrade() {
                        cx.update(|cx| {
                            show_error_toast(workspace, "add to .git/info/exclude", e, cx);
                        });
                    }
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

            Some(())
        });
    }

    fn revert_entry(
        &mut self,
        entry: &GitStatusEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let active_repo = self.active_repository.clone()?;
            let path = active_repo
                .read(cx)
                .repo_path_to_project_path(&entry.repo_path, cx)?;
            let workspace = self.workspace.clone();

            if entry.status.staging().has_staged() {
                self.change_file_stage(false, vec![entry.clone()], cx);
            }
            let filename = path.path.file_name()?.to_string();

            if !entry.status.is_created() {
                self.perform_checkout(vec![entry.clone()], window, cx);
            } else {
                let prompt = prompt(&format!("Trash {}?", filename), None, window, cx);
                cx.spawn_in(window, async move |_, cx| {
                    match prompt.await? {
                        TrashCancel::Trash => {}
                        TrashCancel::Cancel => return Ok(()),
                    }
                    let task = workspace.update(cx, |workspace, cx| {
                        workspace
                            .project()
                            .update(cx, |project, cx| project.delete_file(path, true, cx))
                    })?;
                    if let Some(task) = task {
                        task.await?;
                    }
                    Ok(())
                })
                .detach_and_prompt_err(
                    "Failed to trash file",
                    window,
                    cx,
                    |e, _, _| Some(format!("{e}")),
                );
            }
            Some(())
        });
    }

    fn perform_checkout(
        &mut self,
        entries: Vec<GitStatusEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        let Some(active_repository) = self.active_repository.clone() else {
            return;
        };

        let task = cx.spawn_in(window, async move |this, cx| {
            let tasks: Vec<_> = workspace.update(cx, |workspace, cx| {
                workspace.project().update(cx, |project, cx| {
                    entries
                        .iter()
                        .filter_map(|entry| {
                            let path = active_repository
                                .read(cx)
                                .repo_path_to_project_path(&entry.repo_path, cx)?;
                            Some(project.open_buffer(path, cx))
                        })
                        .collect()
                })
            })?;

            let buffers = futures::future::join_all(tasks).await;

            this.update_in(cx, |this, window, cx| {
                let task = active_repository.update(cx, |repo, cx| {
                    repo.checkout_files(
                        "HEAD",
                        entries
                            .into_iter()
                            .map(|entries| entries.repo_path)
                            .collect(),
                        cx,
                    )
                });
                this.update_visible_entries(window, cx);
                cx.notify();
                task
            })?
            .await?;

            let tasks: Vec<_> = cx.update(|_, cx| {
                buffers
                    .iter()
                    .filter_map(|buffer| {
                        buffer.as_ref().ok()?.update(cx, |buffer, cx| {
                            buffer.is_dirty().then(|| buffer.reload(cx))
                        })
                    })
                    .collect()
            })?;

            futures::future::join_all(tasks).await;

            Ok(())
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;

            this.update_in(cx, |this, window, cx| {
                if let Err(err) = result {
                    this.update_visible_entries(window, cx);
                    this.show_error_toast("checkout", err, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn restore_tracked_files(
        &mut self,
        _: &RestoreTrackedFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self
            .entries
            .iter()
            .filter_map(|entry| entry.status_entry().cloned())
            .filter(|status_entry| !status_entry.status.is_created())
            .collect::<Vec<_>>();

        match entries.len() {
            0 => return,
            1 => return self.revert_entry(&entries[0], window, cx),
            _ => {}
        }
        let mut details = entries
            .iter()
            .filter_map(|entry| entry.repo_path.as_ref().file_name())
            .map(|filename| filename.to_string())
            .take(5)
            .join("\n");
        if entries.len() > 5 {
            details.push_str(&format!("\nand {} more…", entries.len() - 5))
        }

        #[derive(strum::EnumIter, strum::VariantNames)]
        #[strum(serialize_all = "title_case")]
        enum RestoreCancel {
            RestoreTrackedFiles,
            Cancel,
        }
        let prompt = prompt(
            "Discard changes to these files?",
            Some(&details),
            window,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(RestoreCancel::RestoreTrackedFiles) = prompt.await {
                this.update_in(cx, |this, window, cx| {
                    this.perform_checkout(entries, window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    pub(super) fn clean_all(
        &mut self,
        _: &TrashUntrackedFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        let Some(active_repo) = self.active_repository.clone() else {
            return;
        };
        let to_delete = self
            .entries
            .iter()
            .filter_map(|entry| entry.status_entry())
            .filter(|status_entry| status_entry.status.is_created())
            .cloned()
            .collect::<Vec<_>>();

        match to_delete.len() {
            0 => return,
            1 => return self.revert_entry(&to_delete[0], window, cx),
            _ => {}
        };

        let mut details = to_delete
            .iter()
            .map(|entry| {
                entry
                    .repo_path
                    .as_ref()
                    .file_name()
                    .map(|f| f.to_string())
                    .unwrap_or_default()
            })
            .take(5)
            .join("\n");

        if to_delete.len() > 5 {
            details.push_str(&format!("\nand {} more…", to_delete.len() - 5))
        }

        let prompt = prompt("Trash these files?", Some(&details), window, cx);
        cx.spawn_in(window, async move |this, cx| {
            match prompt.await? {
                TrashCancel::Trash => {}
                TrashCancel::Cancel => return Ok(()),
            }
            let tasks = workspace.update(cx, |workspace, cx| {
                to_delete
                    .iter()
                    .filter_map(|entry| {
                        workspace.project().update(cx, |project, cx| {
                            let project_path = active_repo
                                .read(cx)
                                .repo_path_to_project_path(&entry.repo_path, cx)?;
                            project.delete_file(project_path, true, cx)
                        })
                    })
                    .collect::<Vec<_>>()
            })?;
            let to_unstage = to_delete
                .into_iter()
                .filter(|entry| !entry.status.staging().is_fully_unstaged())
                .collect();
            this.update(cx, |this, cx| this.change_file_stage(false, to_unstage, cx))?;
            for task in tasks {
                task.await?;
            }
            Ok(())
        })
        .detach_and_prompt_err("Failed to trash files", window, cx, |e, _, _| {
            Some(format!("{e}"))
        });
    }
}
