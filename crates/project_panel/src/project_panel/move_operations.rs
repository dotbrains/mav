use super::*;

impl ProjectPanel {
    pub(super) fn move_entry(
        &mut self,
        entry_to_move: ProjectEntryId,
        destination: ProjectEntryId,
        destination_is_file: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<CreatedEntry>>> {
        if self
            .project
            .read(cx)
            .entry_is_worktree_root(entry_to_move, cx)
        {
            self.move_worktree_root(entry_to_move, destination, cx);
            None
        } else {
            self.move_worktree_entry(entry_to_move, destination, destination_is_file, cx)
        }
    }

    pub(super) fn move_worktree_root(
        &mut self,
        entry_to_move: ProjectEntryId,
        destination: ProjectEntryId,
        cx: &mut Context<Self>,
    ) {
        self.project.update(cx, |project, cx| {
            let Some(worktree_to_move) = project.worktree_for_entry(entry_to_move, cx) else {
                return;
            };
            let Some(destination_worktree) = project.worktree_for_entry(destination, cx) else {
                return;
            };

            let worktree_id = worktree_to_move.read(cx).id();
            let destination_id = destination_worktree.read(cx).id();

            project
                .move_worktree(worktree_id, destination_id, cx)
                .log_err();
        });
    }

    pub(super) fn move_worktree_entry(
        &mut self,
        entry_to_move: ProjectEntryId,
        destination_entry: ProjectEntryId,
        destination_is_file: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<CreatedEntry>>> {
        if entry_to_move == destination_entry {
            return None;
        }

        let (destination_worktree, rename_task) = self.project.update(cx, |project, cx| {
            let Some(source_path) = project.path_for_entry(entry_to_move, cx) else {
                return (None, None);
            };
            let Some(destination_path) = project.path_for_entry(destination_entry, cx) else {
                return (None, None);
            };
            let destination_worktree_id = destination_path.worktree_id;

            let destination_dir = if destination_is_file {
                destination_path.path.parent().unwrap_or(RelPath::empty())
            } else {
                destination_path.path.as_ref()
            };

            let Some(source_name) = source_path.path.file_name() else {
                return (None, None);
            };
            let Ok(source_name) = RelPath::unix(source_name) else {
                return (None, None);
            };

            let mut new_path = destination_dir.to_rel_path_buf();
            new_path.push(source_name);
            let rename_task = (new_path.as_rel_path() != source_path.path.as_ref()).then(|| {
                project.rename_entry(
                    entry_to_move,
                    (destination_worktree_id, new_path).into(),
                    cx,
                )
            });

            (
                project.worktree_id_for_entry(destination_entry, cx),
                rename_task,
            )
        });

        if let Some(destination_worktree) = destination_worktree {
            self.expand_entry(destination_worktree, destination_entry, cx);
        }
        rename_task
    }

    pub(super) fn drop_external_files(
        &mut self,
        paths: &[PathBuf],
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut paths: Vec<Arc<Path>> = paths.iter().map(|path| Arc::from(path.clone())).collect();

        let open_file_after_drop = paths.len() == 1 && paths[0].is_file();

        let Some((target_directory, worktree, fs)) = maybe!({
            let project = self.project.read(cx);
            let fs = project.fs().clone();
            let worktree = project.worktree_for_entry(entry_id, cx)?;
            let entry = worktree.read(cx).entry_for_id(entry_id)?;
            let path = entry.path.clone();
            let target_directory = if entry.is_dir() {
                path
            } else {
                path.parent()?.into()
            };
            Some((target_directory, worktree, fs))
        }) else {
            return;
        };

        let mut paths_to_replace = Vec::new();
        for path in &paths {
            if let Some(name) = path.file_name()
                && let Some(name) = name.to_str()
            {
                let target_path = target_directory.join(RelPath::unix(name).unwrap());
                if worktree.read(cx).entry_for_path(&target_path).is_some() {
                    paths_to_replace.push((name.to_string(), path.clone()));
                }
            }
        }

        cx.spawn_in(window, async move |this, cx| {
            async move {
                for (filename, original_path) in &paths_to_replace {
                    let prompt_message = format!(
                        concat!(
                            "A file or folder with name {} ",
                            "already exists in the destination folder. ",
                            "Do you want to replace it?"
                        ),
                        filename
                    );
                    let answer = cx
                        .update(|window, cx| {
                            window.prompt(
                                PromptLevel::Info,
                                &prompt_message,
                                None,
                                &["Replace", "Cancel"],
                                cx,
                            )
                        })?
                        .await?;

                    if answer == 1
                        && let Some(item_idx) = paths.iter().position(|p| p == original_path)
                    {
                        paths.remove(item_idx);
                    }
                }

                if paths.is_empty() {
                    return Ok(());
                }

                let (worktree_id, task) = worktree.update(cx, |worktree, cx| {
                    (
                        worktree.id(),
                        worktree.copy_external_entries(target_directory, paths, fs, cx),
                    )
                });

                let opened_entries: Vec<_> = task
                    .await
                    .with_context(|| "failed to copy external paths")?;
                this.update_in(cx, |this, window, cx| {
                    let mut did_open = false;
                    if open_file_after_drop && !opened_entries.is_empty() {
                        let settings = ProjectPanelSettings::get_global(cx);
                        if settings.auto_open.should_open_on_drop() {
                            this.open_entry(opened_entries[0], true, false, cx);
                            did_open = true;
                        }
                    }

                    if !did_open {
                        let new_selection = opened_entries
                            .last()
                            .map(|&entry_id| (worktree_id, entry_id));
                        for &entry_id in &opened_entries {
                            this.expand_entry(worktree_id, entry_id, cx);
                        }
                        this.marked_entries.clear();
                        this.update_visible_entries(new_selection, false, false, window, cx);
                    }

                    let changes: Vec<Change> = opened_entries
                        .iter()
                        .filter_map(|entry_id| {
                            worktree.read(cx).entry_for_id(*entry_id).map(|entry| {
                                Change::Created(ProjectPath {
                                    worktree_id,
                                    path: entry.path.clone(),
                                })
                            })
                        })
                        .collect();

                    this.undo_manager.record(changes).log_err();
                })
            }
            .log_err()
            .await
        })
        .detach();
    }
}
