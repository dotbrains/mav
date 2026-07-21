use super::*;

impl OutlinePanel {
    pub(super) fn update_cached_entries(
        &mut self,
        debounce: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) {
        if !self.active {
            return;
        }

        // A pending debounced update will read the latest state when it fires,
        // so we don't need to reschedule. Constantly rescheduling under a steady stream
        // of events (e.g. project search streaming results) would starve the task forever.
        if debounce.is_some() && self.cached_entries_update_pending {
            return;
        }
        self.cached_entries_update_pending = true;

        self.cached_entries_update_task = cx.spawn_in(window, async move |outline_panel, cx| {
            if let Some(debounce) = debounce {
                cx.background_executor().timer(debounce).await;
            }
            let Some(new_cached_entries) = outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.cached_entries_update_pending = false;
                    let is_singleton = outline_panel.is_singleton_active(cx);
                    let query = outline_panel.query(cx);
                    outline_panel.generate_cached_entries(is_singleton, query, window, cx)
                })
                .ok()
            else {
                return;
            };
            let (new_cached_entries, max_width_item_index) = new_cached_entries.await;
            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.cached_entries = new_cached_entries;
                    outline_panel.max_width_item_index = max_width_item_index;
                    if (outline_panel.selected_entry.is_invalidated()
                        || matches!(outline_panel.selected_entry, SelectedEntry::None))
                        && let Some(new_selected_entry) =
                            outline_panel.active_editor().and_then(|active_editor| {
                                outline_panel.location_for_editor_selection(
                                    &active_editor,
                                    window,
                                    cx,
                                )
                            })
                    {
                        outline_panel.select_entry(new_selected_entry, false, window, cx);
                    }

                    cx.notify();
                })
                .ok();
        });
    }

    pub(super) fn push_entry(
        &self,
        state: &mut GenerationState,
        track_matches: bool,
        entry: PanelEntry,
        depth: usize,
        cx: &mut App,
    ) {
        let entry = if let PanelEntry::FoldedDirs(folded_dirs_entry) = &entry {
            match folded_dirs_entry.entries.len() {
                0 => {
                    debug_panic!("Empty folded dirs receiver");
                    return;
                }
                1 => PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                    worktree_id: folded_dirs_entry.worktree_id,
                    entry: folded_dirs_entry.entries[0].clone(),
                })),
                _ => entry,
            }
        } else {
            entry
        };

        if track_matches {
            let id = state.entries.len();
            match &entry {
                PanelEntry::Fs(fs_entry) => {
                    if let Some(file_name) = self
                        .relative_path(fs_entry, cx)
                        .and_then(|path| Some(path.file_name()?.to_string()))
                    {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &file_name));
                    }
                }
                PanelEntry::FoldedDirs(folded_dir_entry) => {
                    let dir_names = self.dir_names_string(
                        &folded_dir_entry.entries,
                        folded_dir_entry.worktree_id,
                        cx,
                    );
                    {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &dir_names));
                    }
                }
                PanelEntry::Outline(OutlineEntry::Outline(outline_entry)) => state
                    .match_candidates
                    .push(StringMatchCandidate::new(id, &outline_entry.text)),
                PanelEntry::Outline(OutlineEntry::Excerpt(_)) => {}
                PanelEntry::Search(new_search_entry) => {
                    if let Some(search_data) = new_search_entry.render_data.get() {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &search_data.context_text));
                    }
                }
            }
        }

        let width_estimate = self.width_estimate(depth, &entry, cx);
        if Some(width_estimate)
            > state
                .max_width_estimate_and_index
                .map(|(estimate, _)| estimate)
        {
            state.max_width_estimate_and_index = Some((width_estimate, state.entries.len()));
        }
        state.entries.push(CachedEntry {
            depth,
            entry,
            string_match: None,
        });
    }

    pub(super) fn dir_names_string(
        &self,
        entries: &[GitEntry],
        worktree_id: WorktreeId,
        cx: &App,
    ) -> String {
        let dir_names_segment = entries
            .iter()
            .map(|entry| self.entry_name(&worktree_id, entry, cx))
            .collect::<PathBuf>();
        dir_names_segment.to_string_lossy().into_owned()
    }

    pub(super) fn query(&self, cx: &App) -> Option<String> {
        let query = self.filter_editor.read(cx).text(cx);
        if query.trim().is_empty() {
            None
        } else {
            Some(query)
        }
    }

    pub(super) fn is_expanded(&self, entry: &FsEntry) -> bool {
        let entry_to_check = match entry {
            FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }) => {
                CollapsedEntry::ExternalFile(*buffer_id)
            }
            FsEntry::File(FsEntryFile {
                worktree_id,
                buffer_id,
                ..
            }) => CollapsedEntry::File(*worktree_id, *buffer_id),
            FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            }) => CollapsedEntry::Dir(*worktree_id, entry.id),
        };
        !self.collapsed_entries.contains(&entry_to_check)
    }
}

#[derive(Debug, Default)]
pub(super) struct GenerationState {
    pub(super) entries: Vec<CachedEntry>,
    pub(super) match_candidates: Vec<StringMatchCandidate>,
    pub(super) max_width_estimate_and_index: Option<(u64, usize)>,
}

impl GenerationState {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.match_candidates.clear();
        self.max_width_estimate_and_index = None;
    }
}
