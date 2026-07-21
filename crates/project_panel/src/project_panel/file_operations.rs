use super::*;

impl ProjectPanel {
    pub fn undo(&mut self, _: &Undo, _window: &mut Window, _cx: &mut Context<Self>) {
        self.undo_manager.undo().log_err();
    }

    pub fn redo(&mut self, _: &Redo, _window: &mut Window, _cx: &mut Context<Self>) {
        self.undo_manager.redo().log_err();
    }

    pub(super) fn rename_impl(
        &mut self,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(SelectedEntry {
            worktree_id,
            entry_id,
        }) = self.selection
            && let Some(worktree) = self.project.read(cx).worktree_for_id(worktree_id, cx)
        {
            let sub_entry_id = self.unflatten_entry_id(entry_id);
            if let Some(entry) = worktree.read(cx).entry_for_id(sub_entry_id) {
                #[cfg(target_os = "windows")]
                if Some(entry) == worktree.read(cx).root_entry() {
                    return;
                }

                if Some(entry) == worktree.read(cx).root_entry() {
                    let settings = ProjectPanelSettings::get_global(cx);
                    let visible_worktrees_count =
                        self.project.read(cx).visible_worktrees(cx).count();
                    if settings.hide_root && visible_worktrees_count == 1 {
                        return;
                    }
                }

                self.state.edit_state = Some(EditState {
                    worktree_id,
                    entry_id: sub_entry_id,
                    leaf_entry_id: Some(entry_id),
                    is_dir: entry.is_dir(),
                    processing_filename: None,
                    previously_focused: None,
                    depth: 0,
                    validation_state: ValidationState::None,
                    temporarily_unfolded: None,
                });
                let file_name = entry.path.file_name().unwrap_or_default().to_string();
                let selection = selection.unwrap_or_else(|| {
                    // Folders have no extension, so select the whole name. Only
                    // files keep their extension unselected for quick renames.
                    let selection_end = if entry.is_dir() {
                        file_name.len()
                    } else {
                        let file_stem = entry.path.file_stem();
                        file_stem.map_or(file_name.len(), |file_stem| file_stem.len())
                    };
                    0..selection_end
                });
                self.filename_editor.update(cx, |editor, cx| {
                    editor.set_text(file_name, window, cx);
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.select_ranges([
                            MultiBufferOffset(selection.start)..MultiBufferOffset(selection.end)
                        ])
                    });
                });
                self.update_visible_entries(None, true, true, window, cx);
                cx.notify();
            }
        }
    }

    pub(super) fn rename(&mut self, _: &Rename, window: &mut Window, cx: &mut Context<Self>) {
        self.rename_impl(None, window, cx);
    }

    pub(super) fn trash(&mut self, action: &Trash, window: &mut Window, cx: &mut Context<Self>) {
        self.remove(true, action.skip_prompt, window, cx);
    }

    pub(super) fn delete(&mut self, action: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.remove(false, action.skip_prompt, window, cx);
    }

    pub(super) fn remove(
        &mut self,
        trash: bool,
        skip_prompt: bool,
        window: &mut Window,
        cx: &mut Context<ProjectPanel>,
    ) {
        maybe!({
            let items_to_delete = self.disjoint_effective_entries(cx);
            if items_to_delete.is_empty() {
                return None;
            }
            let project = self.project.read(cx);

            let mut dirty_buffers = 0;
            let file_paths = items_to_delete
                .iter()
                .filter_map(|selection| {
                    let project_path = project.path_for_entry(selection.entry_id, cx)?;
                    dirty_buffers +=
                        project.dirty_buffers(cx).any(|path| path == project_path) as usize;

                    Some((
                        selection.entry_id,
                        selection.worktree_id,
                        project_path.path.file_name()?.to_string(),
                    ))
                })
                .collect::<Vec<_>>();
            if file_paths.is_empty() {
                return None;
            }
            let answer = if !skip_prompt {
                let operation = if trash { "Trash" } else { "Delete" };
                let message_start = if trash {
                    "Do you want to trash"
                } else {
                    "Are you sure you want to permanently delete"
                };
                let prompt = match file_paths.first() {
                    Some((_, _, path)) if file_paths.len() == 1 => {
                        let unsaved_warning = if dirty_buffers > 0 {
                            "\n\nIt has unsaved changes, which will be lost."
                        } else {
                            ""
                        };

                        format!(
                            "{message_start} {}?{unsaved_warning}",
                            MarkdownInlineCode(path)
                        )
                    }
                    _ => {
                        const CUTOFF_POINT: usize = 10;
                        let names = if file_paths.len() > CUTOFF_POINT {
                            let truncated_path_counts = file_paths.len() - CUTOFF_POINT;
                            let mut paths = file_paths
                                .iter()
                                .map(|(_, _, path)| MarkdownInlineCode(path).to_string())
                                .take(CUTOFF_POINT)
                                .collect::<Vec<_>>();
                            paths.truncate(CUTOFF_POINT);
                            if truncated_path_counts == 1 {
                                paths.push(".. 1 file not shown".into());
                            } else {
                                paths.push(format!(".. {} files not shown", truncated_path_counts));
                            }
                            paths
                        } else {
                            file_paths
                                .iter()
                                .map(|(_, _, path)| MarkdownInlineCode(path).to_string())
                                .collect()
                        };
                        let unsaved_warning = if dirty_buffers == 0 {
                            String::new()
                        } else if dirty_buffers == 1 {
                            "\n\n1 of these has unsaved changes, which will be lost.".to_string()
                        } else {
                            format!(
                                "\n\n{dirty_buffers} of these have unsaved changes, which will be lost."
                            )
                        };

                        format!(
                            "{message_start} the following {} files?\n{}{unsaved_warning}",
                            file_paths.len(),
                            names.join("\n")
                        )
                    }
                };
                let detail = (!trash).then_some("This cannot be undone.");
                Some(window.prompt(
                    PromptLevel::Info,
                    &prompt,
                    detail,
                    &[operation, "Cancel"],
                    cx,
                ))
            } else {
                None
            };
            let next_selection = self.find_next_selection_after_deletion(items_to_delete, cx);
            cx.spawn_in(window, async move |panel, cx| {
                if let Some(answer) = answer
                    && answer.await != Ok(0)
                {
                    return anyhow::Ok(());
                }

                let mut changes = Vec::new();

                for (entry_id, worktree_id, _) in file_paths {
                    let trashed_entry = panel
                        .update(cx, |panel, cx| {
                            panel
                                .project
                                .update(cx, |project, cx| project.delete_entry(entry_id, trash, cx))
                                .context("no such entry")
                        })??
                        .await?;

                    // Keep track of trashed change so that we can then record
                    // all of the changes at once, such that undoing and redoing
                    // restores or trashes all files in batch.
                    if trash && let Some(trashed_entry) = trashed_entry {
                        changes.push(Change::Trashed(worktree_id, trashed_entry));
                    }
                }
                panel.update_in(cx, |panel, window, cx| {
                    if trash {
                        panel.undo_manager.record(changes).log_err();
                    }

                    if let Some(next_selection) = next_selection {
                        panel.update_visible_entries(
                            Some((next_selection.worktree_id, next_selection.entry_id)),
                            false,
                            true,
                            window,
                            cx,
                        );
                    } else {
                        panel.select_last(&SelectLast {}, window, cx);
                    }
                })?;
                Ok(())
            })
            .detach_and_log_err(cx);
            Some(())
        });
    }

    pub(super) fn find_next_selection_after_deletion(
        &self,
        sanitized_entries: BTreeSet<SelectedEntry>,
        cx: &mut Context<Self>,
    ) -> Option<SelectedEntry> {
        if sanitized_entries.is_empty() {
            return None;
        }
        let project = self.project.read(cx);
        let (worktree_id, worktree) = sanitized_entries
            .iter()
            .map(|entry| entry.worktree_id)
            .filter_map(|id| project.worktree_for_id(id, cx).map(|w| (id, w.read(cx))))
            .max_by(|(_, a), (_, b)| a.root_name().cmp(b.root_name()))?;
        let git_store = project.git_store().read(cx);

        let marked_entries_in_worktree = sanitized_entries
            .iter()
            .filter(|e| e.worktree_id == worktree_id)
            .collect::<HashSet<_>>();
        let latest_entry = marked_entries_in_worktree
            .iter()
            .max_by(|a, b| {
                match (
                    worktree.entry_for_id(a.entry_id),
                    worktree.entry_for_id(b.entry_id),
                ) {
                    (Some(a), Some(b)) => compare_paths(
                        (a.path.as_std_path(), a.is_file()),
                        (b.path.as_std_path(), b.is_file()),
                    ),
                    _ => cmp::Ordering::Equal,
                }
            })
            .and_then(|e| worktree.entry_for_id(e.entry_id))?;

        let parent_path = latest_entry.path.parent()?;
        let parent_entry = worktree.entry_for_path(parent_path)?;

        // Remove all siblings that are being deleted except the last marked entry
        let repo_snapshots = git_store.repo_snapshots(cx);
        let worktree_snapshot = worktree.snapshot();
        let hide_gitignore = ProjectPanelSettings::get_global(cx).hide_gitignore;
        let mut siblings: Vec<_> =
            ChildEntriesGitIter::new(&repo_snapshots, &worktree_snapshot, parent_path)
                .filter(|sibling| {
                    (sibling.id == latest_entry.id)
                        || (!marked_entries_in_worktree.contains(&&SelectedEntry {
                            worktree_id,
                            entry_id: sibling.id,
                        }) && (!hide_gitignore || !sibling.is_ignored))
                })
                .map(|entry| entry.to_owned())
                .collect();

        let sort_mode = ProjectPanelSettings::get_global(cx).sort_mode;
        let sort_order = ProjectPanelSettings::get_global(cx).sort_order;
        sort_worktree_entries(&mut siblings, sort_mode, sort_order);
        let sibling_entry_index = siblings
            .iter()
            .position(|sibling| sibling.id == latest_entry.id)?;

        if let Some(next_sibling) = sibling_entry_index
            .checked_add(1)
            .and_then(|i| siblings.get(i))
        {
            return Some(SelectedEntry {
                worktree_id,
                entry_id: next_sibling.id,
            });
        }
        if let Some(prev_sibling) = sibling_entry_index
            .checked_sub(1)
            .and_then(|i| siblings.get(i))
        {
            return Some(SelectedEntry {
                worktree_id,
                entry_id: prev_sibling.id,
            });
        }
        // No neighbour sibling found, fall back to parent
        Some(SelectedEntry {
            worktree_id,
            entry_id: parent_entry.id,
        })
    }
}
