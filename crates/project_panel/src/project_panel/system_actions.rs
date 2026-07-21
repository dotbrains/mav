use super::*;

impl ProjectPanel {
    pub(super) fn download_from_remote(
        &mut self,
        _: &DownloadFromRemote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.effective_entries();
        if entries.is_empty() {
            return;
        }

        let project = self.project.read(cx);

        // Collect file entries with their worktree_id, path, and relative path for destination
        // For directories, we collect all files under them recursively
        let mut files_to_download: Vec<(WorktreeId, Arc<RelPath>, PathBuf)> = Vec::new();

        for selected in entries.iter() {
            let Some(worktree) = project.worktree_for_id(selected.worktree_id, cx) else {
                continue;
            };
            let worktree = worktree.read(cx);
            let Some(entry) = worktree.entry_for_id(selected.entry_id) else {
                continue;
            };

            if entry.is_file() {
                // Single file: use just the filename
                let filename = entry
                    .path
                    .file_name()
                    .map(str::to_string)
                    .unwrap_or_default();
                files_to_download.push((
                    selected.worktree_id,
                    entry.path.clone(),
                    PathBuf::from(filename),
                ));
            } else if entry.is_dir() {
                // Directory: collect all files recursively, preserving relative paths
                let dir_name = entry
                    .path
                    .file_name()
                    .map(str::to_string)
                    .unwrap_or_default();
                let base_path = entry.path.clone();

                // Use traverse_from_path to iterate all entries under this directory
                let mut traversal = worktree.traverse_from_path(true, true, true, &entry.path);
                while let Some(child_entry) = traversal.entry() {
                    // Stop when we're no longer under the directory
                    if !child_entry.path.starts_with(&base_path) {
                        break;
                    }

                    if child_entry.is_file() {
                        // Calculate relative path from the directory root
                        let relative_path = child_entry
                            .path
                            .strip_prefix(&base_path)
                            .map(|p| PathBuf::from(dir_name.clone()).join(p.as_unix_str()))
                            .unwrap_or_else(|_| {
                                PathBuf::from(
                                    child_entry
                                        .path
                                        .file_name()
                                        .map(str::to_string)
                                        .unwrap_or_default(),
                                )
                            });
                        files_to_download.push((
                            selected.worktree_id,
                            child_entry.path.clone(),
                            relative_path,
                        ));
                    }
                    traversal.advance();
                }
            }
        }

        if files_to_download.is_empty() {
            return;
        }

        let total_files = files_to_download.len();
        let workspace = self.workspace.clone();

        let destination_dir = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Download".into()),
        });

        let fs = self.fs.clone();
        let notification_id =
            workspace::notifications::NotificationId::Named("download-progress".into());
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = destination_dir.await {
                if let Some(dest_dir) = paths.pop() {
                    // Show initial toast
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.show_toast(
                                workspace::Toast::new(
                                    notification_id.clone(),
                                    format!("Downloading 0/{} files...", total_files),
                                ),
                                cx,
                            );
                        })
                        .ok();

                    for (index, (worktree_id, entry_path, relative_path)) in
                        files_to_download.into_iter().enumerate()
                    {
                        // Update progress toast
                        workspace
                            .update(cx, |workspace, cx| {
                                workspace.show_toast(
                                    workspace::Toast::new(
                                        notification_id.clone(),
                                        format!(
                                            "Downloading {}/{} files...",
                                            index + 1,
                                            total_files
                                        ),
                                    ),
                                    cx,
                                );
                            })
                            .ok();

                        let destination_path = dest_dir.join(&relative_path);

                        // Create parent directories if needed
                        if let Some(parent) = destination_path.parent() {
                            if !parent.exists() {
                                fs.create_dir(parent).await.log_err();
                            }
                        }

                        let download_task = this.update(cx, |this, cx| {
                            let project = this.project.clone();
                            project.update(cx, |project, cx| {
                                project.download_file(worktree_id, entry_path, destination_path, cx)
                            })
                        });
                        if let Ok(task) = download_task {
                            task.await.log_err();
                        }
                    }

                    // Show completion toast
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.show_toast(
                                workspace::Toast::new(
                                    notification_id.clone(),
                                    format!("Downloaded {} files", total_files),
                                ),
                                cx,
                            );
                        })
                        .ok();
                }
            }
        })
        .detach();
    }

    pub(super) fn duplicate(&mut self, _: &Duplicate, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy {}, window, cx);
        self.paste(&Paste {}, window, cx);
    }

    pub(super) fn copy_path(
        &mut self,
        _: &mav_actions::workspace::CopyPath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let abs_file_paths = {
            let project = self.project.read(cx);
            self.effective_entries()
                .into_iter()
                .filter_map(|entry| {
                    let entry_path = project.path_for_entry(entry.entry_id, cx)?.path;
                    Some(
                        project
                            .worktree_for_id(entry.worktree_id, cx)?
                            .read(cx)
                            .absolutize(&entry_path)
                            .to_string_lossy()
                            .to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        if !abs_file_paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(abs_file_paths.join("\n")));
        }
    }

    pub(super) fn copy_relative_path(
        &mut self,
        _: &mav_actions::workspace::CopyRelativePath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_style = self.project.read(cx).path_style(cx);
        let file_paths = {
            let project = self.project.read(cx);
            self.effective_entries()
                .into_iter()
                .filter_map(|entry| {
                    Some(
                        project
                            .path_for_entry(entry.entry_id, cx)?
                            .path
                            .display(path_style)
                            .into_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };
        if !file_paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(file_paths.join("\n")));
        }
    }

    pub(super) fn reveal_in_finder(
        &mut self,
        _: &RevealInFileManager,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.reveal_in_file_manager_path(cx) {
            self.project
                .update(cx, |project, cx| project.reveal_path(&path, cx));
        }
    }

    pub(super) fn remove_from_project(
        &mut self,
        _: &RemoveFromProject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for entry in self.effective_entries().iter() {
            let worktree_id = entry.worktree_id;
            self.project
                .update(cx, |project, cx| project.remove_worktree(worktree_id, cx));
        }
    }

    pub(super) fn file_abs_paths_to_diff(&self, cx: &Context<Self>) -> Option<(PathBuf, PathBuf)> {
        let mut selections_abs_path = self
            .marked_entries
            .iter()
            .filter_map(|entry| {
                let project = self.project.read(cx);
                let worktree = project.worktree_for_id(entry.worktree_id, cx)?;
                let entry = worktree.read(cx).entry_for_id(entry.entry_id)?;
                if !entry.is_file() {
                    return None;
                }
                Some(worktree.read(cx).absolutize(&entry.path))
            })
            .rev();

        let last_path = selections_abs_path.next()?;
        let previous_to_last = selections_abs_path.next()?;
        Some((previous_to_last, last_path))
    }

    pub(super) fn compare_marked_files(
        &mut self,
        _: &CompareMarkedFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_files = self.file_abs_paths_to_diff(cx);
        if let Some((file_path1, file_path2)) = selected_files {
            self.workspace
                .update(cx, |workspace, cx| {
                    FileDiffView::open(file_path1, file_path2, workspace.weak_handle(), window, cx)
                        .detach_and_log_err(cx);
                })
                .ok();
        }
    }

    pub(super) fn open_system(
        &mut self,
        _: &OpenWithSystem,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            let abs_path = worktree.absolutize(&entry.path);
            cx.open_with_system(&abs_path);
        }
    }

    pub(super) fn open_in_terminal(
        &mut self,
        _: &OpenInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_sub_entry(cx) {
            let abs_path = match &entry.canonical_path {
                Some(canonical_path) => canonical_path.to_path_buf(),
                None => worktree.read(cx).absolutize(&entry.path),
            };

            let working_directory = if entry.is_dir() {
                Some(abs_path)
            } else {
                abs_path.parent().map(|path| path.to_path_buf())
            };
            if let Some(working_directory) = working_directory {
                window.dispatch_action(
                    workspace::OpenTerminal {
                        working_directory,
                        local: false,
                    }
                    .boxed_clone(),
                    cx,
                )
            }
        }
    }

    pub fn new_search_in_directory(
        &mut self,
        _: &NewSearchInDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_sub_entry(cx) {
            let dir_path = if entry.is_dir() {
                entry.path.clone()
            } else {
                // entry is a file, use its parent directory
                match entry.path.parent() {
                    Some(parent) => Arc::from(parent),
                    None => {
                        // File at root, open search with empty filter
                        self.workspace
                            .update(cx, |workspace, cx| {
                                search::ProjectSearchView::new_search_in_directory(
                                    workspace,
                                    RelPath::empty(),
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                        return;
                    }
                }
            };

            let include_root = self.project.read(cx).visible_worktrees(cx).count() > 1;
            let dir_path = if include_root {
                worktree.read(cx).root_name().join(&dir_path)
            } else {
                dir_path
            };

            self.workspace
                .update(cx, |workspace, cx| {
                    search::ProjectSearchView::new_search_in_directory(
                        workspace, &dir_path, window, cx,
                    );
                })
                .ok();
        }
    }
}
