use super::*;

impl OutlinePanel {
    pub(super) fn expand_selected_entry(
        &mut self,
        _: &ExpandSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };
        let Some(selected_entry) = self.selected_entry().cloned() else {
            return;
        };
        let mut buffers_to_unfold = HashSet::default();
        let entry_to_expand = match &selected_entry {
            PanelEntry::FoldedDirs(FoldedDirsEntry {
                entries: dir_entries,
                worktree_id,
                ..
            }) => dir_entries.last().map(|entry| {
                buffers_to_unfold.extend(self.buffers_inside_directory(*worktree_id, entry));
                CollapsedEntry::Dir(*worktree_id, entry.id)
            }),
            PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            })) => {
                buffers_to_unfold.extend(self.buffers_inside_directory(*worktree_id, entry));
                Some(CollapsedEntry::Dir(*worktree_id, entry.id))
            }
            PanelEntry::Fs(FsEntry::File(FsEntryFile {
                worktree_id,
                buffer_id,
                ..
            })) => {
                buffers_to_unfold.insert(*buffer_id);
                Some(CollapsedEntry::File(*worktree_id, *buffer_id))
            }
            PanelEntry::Fs(FsEntry::ExternalFile(external_file)) => {
                buffers_to_unfold.insert(external_file.buffer_id);
                Some(CollapsedEntry::ExternalFile(external_file.buffer_id))
            }
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                Some(CollapsedEntry::Excerpt(excerpt.clone()))
            }
            PanelEntry::Outline(OutlineEntry::Outline(outline)) => {
                Some(CollapsedEntry::Outline(outline.range.clone()))
            }
            PanelEntry::Search(_) => return,
        };
        let Some(collapsed_entry) = entry_to_expand else {
            return;
        };
        let expanded = self.collapsed_entries.remove(&collapsed_entry);
        if expanded {
            if let CollapsedEntry::Dir(worktree_id, dir_entry_id) = collapsed_entry {
                let task = self.project.update(cx, |project, cx| {
                    project.expand_entry(worktree_id, dir_entry_id, cx)
                });
                if let Some(task) = task {
                    task.detach_and_log_err(cx);
                }
            };

            active_editor.update(cx, |editor, cx| {
                buffers_to_unfold.retain(|buffer_id| editor.is_buffer_folded(*buffer_id, cx));
            });
            self.select_entry(selected_entry, true, window, cx);
            if buffers_to_unfold.is_empty() {
                self.update_cached_entries(None, window, cx);
            } else {
                self.toggle_buffers_fold(buffers_to_unfold, false, window, cx)
                    .detach();
            }
        } else {
            self.select_next(&SelectNext, window, cx)
        }
    }

    pub(super) fn collapse_selected_entry(
        &mut self,
        _: &CollapseSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };
        let Some(selected_entry) = self.selected_entry().cloned() else {
            return;
        };

        let mut buffers_to_fold = HashSet::default();
        let collapsed = match &selected_entry {
            PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            })) => {
                if self
                    .collapsed_entries
                    .insert(CollapsedEntry::Dir(*worktree_id, entry.id))
                {
                    buffers_to_fold.extend(self.buffers_inside_directory(*worktree_id, entry));
                    true
                } else {
                    false
                }
            }
            PanelEntry::Fs(FsEntry::File(FsEntryFile {
                worktree_id,
                buffer_id,
                ..
            })) => {
                if self
                    .collapsed_entries
                    .insert(CollapsedEntry::File(*worktree_id, *buffer_id))
                {
                    buffers_to_fold.insert(*buffer_id);
                    true
                } else {
                    false
                }
            }
            PanelEntry::Fs(FsEntry::ExternalFile(external_file)) => {
                if self
                    .collapsed_entries
                    .insert(CollapsedEntry::ExternalFile(external_file.buffer_id))
                {
                    buffers_to_fold.insert(external_file.buffer_id);
                    true
                } else {
                    false
                }
            }
            PanelEntry::FoldedDirs(folded_dirs) => {
                let mut folded = false;
                if let Some(dir_entry) = folded_dirs.entries.last()
                    && self
                        .collapsed_entries
                        .insert(CollapsedEntry::Dir(folded_dirs.worktree_id, dir_entry.id))
                {
                    folded = true;
                    buffers_to_fold
                        .extend(self.buffers_inside_directory(folded_dirs.worktree_id, dir_entry));
                }
                folded
            }
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => self
                .collapsed_entries
                .insert(CollapsedEntry::Excerpt(excerpt.clone())),
            PanelEntry::Outline(OutlineEntry::Outline(outline)) => self
                .collapsed_entries
                .insert(CollapsedEntry::Outline(outline.range.clone())),
            PanelEntry::Search(_) => false,
        };

        if collapsed {
            active_editor.update(cx, |editor, cx| {
                buffers_to_fold.retain(|buffer_id| !editor.is_buffer_folded(*buffer_id, cx));
            });
            self.select_entry(selected_entry, true, window, cx);
            if buffers_to_fold.is_empty() {
                self.update_cached_entries(None, window, cx);
            } else {
                self.toggle_buffers_fold(buffers_to_fold, true, window, cx)
                    .detach();
            }
        } else {
            self.select_parent(&SelectParent, window, cx);
        }
    }

    pub(super) fn expand_all_entries(
        &mut self,
        _: &ExpandAllEntries,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };

        let mut to_uncollapse: HashSet<CollapsedEntry> = HashSet::default();
        let mut buffers_to_unfold: HashSet<BufferId> = HashSet::default();

        for fs_entry in &self.fs_entries {
            match fs_entry {
                FsEntry::File(FsEntryFile {
                    worktree_id,
                    buffer_id,
                    ..
                }) => {
                    to_uncollapse.insert(CollapsedEntry::File(*worktree_id, *buffer_id));
                    buffers_to_unfold.insert(*buffer_id);
                }
                FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }) => {
                    to_uncollapse.insert(CollapsedEntry::ExternalFile(*buffer_id));
                    buffers_to_unfold.insert(*buffer_id);
                }
                FsEntry::Directory(FsEntryDirectory {
                    worktree_id, entry, ..
                }) => {
                    to_uncollapse.insert(CollapsedEntry::Dir(*worktree_id, entry.id));
                }
            }
        }

        for (_buffer_id, buffer) in &self.buffers {
            match &buffer.outlines {
                OutlineState::Outlines(outlines) => {
                    for outline in outlines {
                        to_uncollapse.insert(CollapsedEntry::Outline(outline.range.clone()));
                    }
                }
                OutlineState::Invalidated(outlines) => {
                    for outline in outlines {
                        to_uncollapse.insert(CollapsedEntry::Outline(outline.range.clone()));
                    }
                }
                OutlineState::NotFetched => {}
            }
            to_uncollapse.extend(
                buffer
                    .excerpts
                    .iter()
                    .map(|excerpt| CollapsedEntry::Excerpt(excerpt.clone())),
            );
        }

        for cached in &self.cached_entries {
            if let PanelEntry::FoldedDirs(FoldedDirsEntry {
                worktree_id,
                entries,
                ..
            }) = &cached.entry
            {
                if let Some(last) = entries.last() {
                    to_uncollapse.insert(CollapsedEntry::Dir(*worktree_id, last.id));
                }
            }
        }

        self.collapsed_entries
            .retain(|entry| !to_uncollapse.contains(entry));

        active_editor.update(cx, |editor, cx| {
            buffers_to_unfold.retain(|buffer_id| editor.is_buffer_folded(*buffer_id, cx));
        });

        if buffers_to_unfold.is_empty() {
            self.update_cached_entries(None, window, cx);
        } else {
            self.toggle_buffers_fold(buffers_to_unfold, false, window, cx)
                .detach();
        }
    }

    pub(super) fn collapse_all_entries(
        &mut self,
        _: &CollapseAllEntries,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };
        let mut buffers_to_fold = HashSet::default();
        self.collapsed_entries
            .extend(self.cached_entries.iter().filter_map(
                |cached_entry| match &cached_entry.entry {
                    PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                        worktree_id,
                        entry,
                        ..
                    })) => Some(CollapsedEntry::Dir(*worktree_id, entry.id)),
                    PanelEntry::Fs(FsEntry::File(FsEntryFile {
                        worktree_id,
                        buffer_id,
                        ..
                    })) => {
                        buffers_to_fold.insert(*buffer_id);
                        Some(CollapsedEntry::File(*worktree_id, *buffer_id))
                    }
                    PanelEntry::Fs(FsEntry::ExternalFile(external_file)) => {
                        buffers_to_fold.insert(external_file.buffer_id);
                        Some(CollapsedEntry::ExternalFile(external_file.buffer_id))
                    }
                    PanelEntry::FoldedDirs(FoldedDirsEntry {
                        worktree_id,
                        entries,
                        ..
                    }) => Some(CollapsedEntry::Dir(*worktree_id, entries.last()?.id)),
                    PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                        Some(CollapsedEntry::Excerpt(excerpt.clone()))
                    }
                    PanelEntry::Outline(OutlineEntry::Outline(outline)) => {
                        Some(CollapsedEntry::Outline(outline.range.clone()))
                    }
                    PanelEntry::Search(_) => None,
                },
            ));

        active_editor.update(cx, |editor, cx| {
            buffers_to_fold.retain(|buffer_id| !editor.is_buffer_folded(*buffer_id, cx));
        });
        if buffers_to_fold.is_empty() {
            self.update_cached_entries(None, window, cx);
        } else {
            self.toggle_buffers_fold(buffers_to_fold, true, window, cx)
                .detach();
        }
    }

    pub(super) fn toggle_expanded(
        &mut self,
        entry: &PanelEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };
        let mut fold = false;
        let mut buffers_to_toggle = HashSet::default();
        match entry {
            PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id,
                entry: dir_entry,
                ..
            })) => {
                let entry_id = dir_entry.id;
                let collapsed_entry = CollapsedEntry::Dir(*worktree_id, entry_id);
                buffers_to_toggle.extend(self.buffers_inside_directory(*worktree_id, dir_entry));
                if self.collapsed_entries.remove(&collapsed_entry) {
                    self.project
                        .update(cx, |project, cx| {
                            project.expand_entry(*worktree_id, entry_id, cx)
                        })
                        .unwrap_or_else(|| Task::ready(Ok(())))
                        .detach_and_log_err(cx);
                } else {
                    self.collapsed_entries.insert(collapsed_entry);
                    fold = true;
                }
            }
            PanelEntry::Fs(FsEntry::File(FsEntryFile {
                worktree_id,
                buffer_id,
                ..
            })) => {
                let collapsed_entry = CollapsedEntry::File(*worktree_id, *buffer_id);
                buffers_to_toggle.insert(*buffer_id);
                if !self.collapsed_entries.remove(&collapsed_entry) {
                    self.collapsed_entries.insert(collapsed_entry);
                    fold = true;
                }
            }
            PanelEntry::Fs(FsEntry::ExternalFile(external_file)) => {
                let collapsed_entry = CollapsedEntry::ExternalFile(external_file.buffer_id);
                buffers_to_toggle.insert(external_file.buffer_id);
                if !self.collapsed_entries.remove(&collapsed_entry) {
                    self.collapsed_entries.insert(collapsed_entry);
                    fold = true;
                }
            }
            PanelEntry::FoldedDirs(FoldedDirsEntry {
                worktree_id,
                entries: dir_entries,
                ..
            }) => {
                if let Some(dir_entry) = dir_entries.first() {
                    let entry_id = dir_entry.id;
                    let collapsed_entry = CollapsedEntry::Dir(*worktree_id, entry_id);
                    buffers_to_toggle
                        .extend(self.buffers_inside_directory(*worktree_id, dir_entry));
                    if self.collapsed_entries.remove(&collapsed_entry) {
                        self.project
                            .update(cx, |project, cx| {
                                project.expand_entry(*worktree_id, entry_id, cx)
                            })
                            .unwrap_or_else(|| Task::ready(Ok(())))
                            .detach_and_log_err(cx);
                    } else {
                        self.collapsed_entries.insert(collapsed_entry);
                        fold = true;
                    }
                }
            }
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                let collapsed_entry = CollapsedEntry::Excerpt(excerpt.clone());
                if !self.collapsed_entries.remove(&collapsed_entry) {
                    self.collapsed_entries.insert(collapsed_entry);
                }
            }
            PanelEntry::Outline(OutlineEntry::Outline(outline)) => {
                let collapsed_entry = CollapsedEntry::Outline(outline.range.clone());
                if !self.collapsed_entries.remove(&collapsed_entry) {
                    self.collapsed_entries.insert(collapsed_entry);
                }
            }
            _ => {}
        }

        active_editor.update(cx, |editor, cx| {
            buffers_to_toggle.retain(|buffer_id| {
                let folded = editor.is_buffer_folded(*buffer_id, cx);
                if fold { !folded } else { folded }
            });
        });

        self.select_entry(entry.clone(), true, window, cx);
        if buffers_to_toggle.is_empty() {
            self.update_cached_entries(None, window, cx);
        } else {
            self.toggle_buffers_fold(buffers_to_toggle, fold, window, cx)
                .detach();
        }
    }

    pub(super) fn toggle_buffers_fold(
        &self,
        buffers: HashSet<BufferId>,
        fold: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let Some(active_editor) = self.active_editor() else {
            return Task::ready(());
        };
        cx.spawn_in(window, async move |outline_panel, cx| {
            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    active_editor.update(cx, |editor, cx| {
                        for buffer_id in buffers {
                            outline_panel
                                .preserve_selection_on_buffer_fold_toggles
                                .insert(buffer_id);
                            if fold {
                                editor.fold_buffer(buffer_id, cx);
                            } else {
                                editor.unfold_buffer(buffer_id, cx);
                            }
                        }
                    });
                    if let Some(selection) = outline_panel.selected_entry().cloned() {
                        outline_panel.scroll_editor_to_entry(&selection, false, false, window, cx);
                    }
                })
                .ok();
        })
    }
}
