use super::*;

impl ProjectPanel {
    pub(super) fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let entries = self.disjoint_effective_entries(cx);
        if !entries.is_empty() {
            self.write_entries_to_system_clipboard(&entries, cx);
            self.clipboard = Some(ClipboardEntry::Cut(entries));
            cx.notify();
        }
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let entries = self.disjoint_effective_entries(cx);
        if !entries.is_empty() {
            self.write_entries_to_system_clipboard(&entries, cx);
            self.clipboard = Some(ClipboardEntry::Copied(entries));
            cx.notify();
        }
    }

    pub(super) fn create_paste_path(
        &self,
        source: &SelectedEntry,
        (worktree, target_entry): (Entity<Worktree>, &Entry),
        cx: &App,
    ) -> Option<(Arc<RelPath>, Option<Range<usize>>)> {
        let mut new_path = target_entry.path.to_rel_path_buf();
        // If we're pasting into a file, or a directory into itself, go up one level.
        if target_entry.is_file() || (target_entry.is_dir() && target_entry.id == source.entry_id) {
            new_path.pop();
        }

        let source_worktree = self
            .project
            .read(cx)
            .worktree_for_entry(source.entry_id, cx)?;
        let source_entry = source_worktree.read(cx).entry_for_id(source.entry_id)?;

        let clipboard_entry_file_name = source_entry.path.file_name()?.to_string();
        new_path.push(RelPath::unix(&clipboard_entry_file_name).unwrap());

        let (extension, file_name_without_extension) = if source_entry.is_file() {
            (
                new_path.extension().map(|s| s.to_string()),
                new_path.file_stem()?.to_string(),
            )
        } else {
            (None, clipboard_entry_file_name.clone())
        };

        let file_name_len = file_name_without_extension.len();
        let mut disambiguation_range = None;
        let mut ix = 0;
        {
            let worktree = worktree.read(cx);
            while worktree.entry_for_path(&new_path).is_some() {
                new_path.pop();

                let mut new_file_name = file_name_without_extension.to_string();

                let disambiguation = " copy";
                let mut disambiguation_len = disambiguation.len();

                new_file_name.push_str(disambiguation);

                if ix > 0 {
                    let extra_disambiguation = format!(" {}", ix);
                    disambiguation_len += extra_disambiguation.len();
                    new_file_name.push_str(&extra_disambiguation);
                }
                if let Some(extension) = extension.as_ref() {
                    new_file_name.push_str(".");
                    new_file_name.push_str(extension);
                }

                new_path.push(RelPath::unix(&new_file_name).unwrap());

                disambiguation_range = Some(0..(file_name_len + disambiguation_len));
                ix += 1;
            }
        }
        Some((new_path.as_rel_path().into(), disambiguation_range))
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(external_paths) = self.external_paths_from_system_clipboard(cx) {
            let target_entry_id = self
                .selection
                .map(|s| s.entry_id)
                .or(self.state.last_worktree_root_id);
            if let Some(entry_id) = target_entry_id {
                self.drop_external_files(external_paths.paths(), entry_id, window, cx);
            }
            return;
        }

        maybe!({
            let (worktree, entry) = self.selected_entry_handle(cx)?;
            let entry = entry.clone();
            let worktree_id = worktree.read(cx).id();
            let clipboard_entries = self
                .clipboard
                .as_ref()
                .filter(|clipboard| !clipboard.items().is_empty())?;

            enum PasteTask {
                Rename {
                    task: Task<Result<CreatedEntry>>,
                    from: ProjectPath,
                    to: ProjectPath,
                },
                Copy {
                    task: Task<Result<Option<Entry>>>,
                    destination: ProjectPath,
                },
            }

            let mut paste_tasks = Vec::new();
            let mut disambiguation_range = None;
            let clip_is_cut = clipboard_entries.is_cut();
            for clipboard_entry in clipboard_entries.items() {
                let (new_path, new_disambiguation_range) =
                    self.create_paste_path(clipboard_entry, self.selected_sub_entry(cx)?, cx)?;
                let clip_entry_id = clipboard_entry.entry_id;
                let destination: ProjectPath = (worktree_id, new_path).into();
                let task = if clipboard_entries.is_cut() {
                    let original_path = self.project.read(cx).path_for_entry(clip_entry_id, cx)?;
                    let task = self.project.update(cx, |project, cx| {
                        project.rename_entry(clip_entry_id, destination.clone(), cx)
                    });
                    PasteTask::Rename {
                        task,
                        from: original_path,
                        to: destination,
                    }
                } else {
                    let task = self.project.update(cx, |project, cx| {
                        project.copy_entry(clip_entry_id, destination.clone(), cx)
                    });
                    PasteTask::Copy { task, destination }
                };
                paste_tasks.push(task);
                disambiguation_range = new_disambiguation_range.or(disambiguation_range);
            }

            let item_count = paste_tasks.len();
            let workspace = self.workspace.clone();

            cx.spawn_in(window, async move |project_panel, mut cx| {
                let mut last_succeed = None;
                let mut changes = Vec::new();

                for task in paste_tasks {
                    match task {
                        PasteTask::Rename { task, from, to } => {
                            if let Some(CreatedEntry::Included(entry)) = task
                                .await
                                .notify_workspace_async_err(workspace.clone(), &mut cx)
                            {
                                changes.push(Change::Renamed(from, to));
                                last_succeed = Some(entry);
                            }
                        }
                        PasteTask::Copy { task, destination } => {
                            if let Some(Some(entry)) = task
                                .await
                                .notify_workspace_async_err(workspace.clone(), &mut cx)
                            {
                                changes.push(Change::Created(destination));
                                last_succeed = Some(entry);
                            }
                        }
                    }
                }

                project_panel
                    .update(cx, |this, _| {
                        this.undo_manager.record(changes).log_err();
                    })
                    .ok();

                // update selection
                if let Some(entry) = last_succeed {
                    project_panel
                        .update_in(cx, |project_panel, window, cx| {
                            project_panel.selection = Some(SelectedEntry {
                                worktree_id,
                                entry_id: entry.id,
                            });

                            if item_count == 1 {
                                // open entry if not dir, setting is enabled, and only focus if rename is not pending
                                if !entry.is_dir() {
                                    let settings = ProjectPanelSettings::get_global(cx);
                                    if settings.auto_open.should_open_on_paste() {
                                        project_panel.open_entry(
                                            entry.id,
                                            disambiguation_range.is_none(),
                                            false,
                                            cx,
                                        );
                                    }
                                }

                                // if only one entry was pasted and it was disambiguated, open the rename editor
                                if disambiguation_range.is_some() {
                                    cx.defer_in(window, |this, window, cx| {
                                        this.rename_impl(disambiguation_range, window, cx);
                                    });
                                }
                            }
                        })
                        .ok();
                }

                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

            if clip_is_cut {
                // Convert the clipboard cut entry to a copy entry after the first paste.
                self.clipboard = self.clipboard.take().map(ClipboardEntry::into_copy_entry);
            }

            self.expand_entry(worktree_id, entry.id, cx);
            Some(())
        });
    }

    pub(super) fn write_entries_to_system_clipboard(
        &self,
        entries: &BTreeSet<SelectedEntry>,
        cx: &mut App,
    ) {
        let project = self.project.read(cx);
        let paths: Vec<String> = entries
            .iter()
            .filter_map(|entry| {
                let worktree = project.worktree_for_id(entry.worktree_id, cx)?;
                let worktree = worktree.read(cx);
                let worktree_entry = worktree.entry_for_id(entry.entry_id)?;
                Some(
                    worktree
                        .abs_path()
                        .join(worktree_entry.path.as_std_path())
                        .to_string_lossy()
                        .to_string(),
                )
            })
            .collect();
        if !paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(paths.join("\n")));
        }
    }

    pub(super) fn external_paths_from_system_clipboard(&self, cx: &App) -> Option<ExternalPaths> {
        let clipboard_item = cx.read_from_clipboard()?;
        for entry in clipboard_item.entries() {
            if let GpuiClipboardEntry::ExternalPaths(paths) = entry {
                if !paths.paths().is_empty() {
                    return Some(paths.clone());
                }
            }
        }
        None
    }

    pub(super) fn has_pasteable_content(&self, cx: &App) -> bool {
        if self
            .clipboard
            .as_ref()
            .is_some_and(|c| !c.items().is_empty())
        {
            return true;
        }
        self.external_paths_from_system_clipboard(cx).is_some()
    }
}
