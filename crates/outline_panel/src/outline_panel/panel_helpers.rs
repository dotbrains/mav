use super::*;

impl OutlinePanel {
    pub(super) fn selected_entry(&self) -> Option<&PanelEntry> {
        match &self.selected_entry {
            SelectedEntry::Invalidated(entry) => entry.as_ref(),
            SelectedEntry::Valid(entry, _) => Some(entry),
            SelectedEntry::None => None,
        }
    }

    pub(super) fn select_entry(
        &mut self,
        entry: PanelEntry,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if focus {
            self.focus_handle.focus(window, cx);
        }
        let ix = self
            .cached_entries
            .iter()
            .enumerate()
            .find(|(_, cached_entry)| &cached_entry.entry == &entry)
            .map(|(i, _)| i)
            .unwrap_or_default();

        self.selected_entry = SelectedEntry::Valid(entry, ix);

        self.autoscroll(cx);
        cx.notify();
    }

    pub(super) fn width_estimate(&self, depth: usize, entry: &PanelEntry, cx: &App) -> u64 {
        let item_text_chars = match entry {
            PanelEntry::Fs(FsEntry::ExternalFile(external)) => self
                .buffer_snapshot_for_id(external.buffer_id, cx)
                .and_then(|snapshot| Some(snapshot.file()?.path().file_name()?.len()))
                .unwrap_or_default(),
            PanelEntry::Fs(FsEntry::Directory(directory)) => directory
                .entry
                .path
                .file_name()
                .map(|name| name.len())
                .unwrap_or_default(),
            PanelEntry::Fs(FsEntry::File(file)) => file
                .entry
                .path
                .file_name()
                .map(|name| name.len())
                .unwrap_or_default(),
            PanelEntry::FoldedDirs(folded_dirs) => {
                folded_dirs
                    .entries
                    .iter()
                    .map(|dir| {
                        dir.path
                            .file_name()
                            .map(|name| name.len())
                            .unwrap_or_default()
                    })
                    .sum::<usize>()
                    + folded_dirs.entries.len().saturating_sub(1) * "/".len()
            }
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => self
                .excerpt_label(&excerpt, cx)
                .map(|label| label.len())
                .unwrap_or_default(),
            PanelEntry::Outline(OutlineEntry::Outline(entry)) => entry.text.len(),
            PanelEntry::Search(search) => search
                .render_data
                .get()
                .map(|data| data.context_text.len())
                .unwrap_or_default(),
        };

        (item_text_chars + depth) as u64
    }

    pub(super) fn buffers_inside_directory(
        &self,
        dir_worktree: WorktreeId,
        dir_entry: &GitEntry,
    ) -> HashSet<BufferId> {
        if !dir_entry.is_dir() {
            debug_panic!("buffers_inside_directory called on a non-directory entry {dir_entry:?}");
            return HashSet::default();
        }

        self.fs_entries
            .iter()
            .skip_while(|fs_entry| match fs_entry {
                FsEntry::Directory(directory) => {
                    directory.worktree_id != dir_worktree || &directory.entry != dir_entry
                }
                _ => true,
            })
            .skip(1)
            .take_while(|fs_entry| match fs_entry {
                FsEntry::ExternalFile(..) => false,
                FsEntry::Directory(directory) => {
                    directory.worktree_id == dir_worktree
                        && directory.entry.path.starts_with(&dir_entry.path)
                }
                FsEntry::File(file) => {
                    file.worktree_id == dir_worktree && file.entry.path.starts_with(&dir_entry.path)
                }
            })
            .filter_map(|fs_entry| match fs_entry {
                FsEntry::File(file) => Some(file.buffer_id),
                _ => None,
            })
            .collect()
    }
}

pub(super) fn back_to_common_visited_parent(
    visited_dirs: &mut Vec<(ProjectEntryId, Arc<RelPath>)>,
    worktree_id: &WorktreeId,
    new_entry: &Entry,
) -> Option<(WorktreeId, ProjectEntryId)> {
    while let Some((visited_dir_id, visited_path)) = visited_dirs.last() {
        match new_entry.path.parent() {
            Some(parent_path) => {
                if parent_path == visited_path.as_ref() {
                    return Some((*worktree_id, *visited_dir_id));
                }
            }
            None => {
                break;
            }
        }
        visited_dirs.pop();
    }
    None
}

pub(super) fn file_name(path: &Path) -> String {
    let mut current_path = path;
    loop {
        if let Some(file_name) = current_path.file_name() {
            return file_name.to_string_lossy().into_owned();
        }
        match current_path.parent() {
            Some(parent) => current_path = parent,
            None => return path.to_string_lossy().into_owned(),
        }
    }
}

pub(super) fn find_active_indent_guide_ix(
    outline_panel: &OutlinePanel,
    candidates: &[IndentGuideLayout],
) -> Option<usize> {
    let SelectedEntry::Valid(_, target_ix) = &outline_panel.selected_entry else {
        return None;
    };
    let target_depth = outline_panel
        .cached_entries
        .get(*target_ix)
        .map(|cached_entry| cached_entry.depth)?;

    let (target_ix, target_depth) = if let Some(target_depth) = outline_panel
        .cached_entries
        .get(target_ix + 1)
        .filter(|cached_entry| cached_entry.depth > target_depth)
        .map(|entry| entry.depth)
    {
        (target_ix + 1, target_depth.saturating_sub(1))
    } else {
        (*target_ix, target_depth.saturating_sub(1))
    };

    candidates
        .iter()
        .enumerate()
        .find(|(_, guide)| {
            guide.offset.y <= target_ix
                && target_ix < guide.offset.y + guide.length
                && guide.offset.x == target_depth
        })
        .map(|(ix, _)| ix)
}

pub(super) fn subscribe_for_editor_events(
    editor: &Entity<Editor>,
    window: &mut Window,
    cx: &mut Context<OutlinePanel>,
) -> Subscription {
    let debounce = Some(UPDATE_DEBOUNCE);
    cx.subscribe_in(
        editor,
        window,
        move |outline_panel, editor, e: &EditorEvent, window, cx| {
            if !outline_panel.active {
                return;
            }
            match e {
                EditorEvent::SelectionsChanged { local: true } => {
                    outline_panel.reveal_entry_for_selection(editor.clone(), window, cx);
                    cx.notify();
                }
                EditorEvent::BuffersRemoved { removed_buffer_ids } => {
                    outline_panel
                        .buffers
                        .retain(|buffer_id, _| !removed_buffer_ids.contains(buffer_id));
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::BufferRangesUpdated { buffer, .. } => {
                    outline_panel
                        .new_entries_for_fs_update
                        .insert(buffer.read(cx).remote_id());
                    outline_panel.invalidate_outlines(&[buffer.read(cx).remote_id()]);
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::BuffersEdited { buffer_ids } => {
                    outline_panel.invalidate_outlines(buffer_ids);
                    let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                    if update_cached_items {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::BufferFoldToggled { ids, .. } => {
                    outline_panel.invalidate_outlines(ids);
                    let mut latest_unfolded_buffer_id = None;
                    let mut latest_folded_buffer_id = None;
                    let mut ignore_selections_change = false;
                    outline_panel.new_entries_for_fs_update.extend(
                        ids.iter()
                            .filter(|id| {
                                if outline_panel.buffers.contains_key(&id) {
                                    ignore_selections_change |= outline_panel
                                        .preserve_selection_on_buffer_fold_toggles
                                        .remove(&id);
                                    if editor.read(cx).is_buffer_folded(**id, cx) {
                                        latest_folded_buffer_id = Some(**id);
                                        false
                                    } else {
                                        latest_unfolded_buffer_id = Some(**id);
                                        true
                                    }
                                } else {
                                    false
                                }
                            })
                            .copied(),
                    );
                    if !ignore_selections_change
                        && let Some(entry_to_select) = latest_unfolded_buffer_id
                            .or(latest_folded_buffer_id)
                            .and_then(|toggled_buffer_id| {
                                outline_panel.fs_entries.iter().find_map(
                                    |fs_entry| match fs_entry {
                                        FsEntry::ExternalFile(external) => {
                                            if external.buffer_id == toggled_buffer_id {
                                                Some(fs_entry.clone())
                                            } else {
                                                None
                                            }
                                        }
                                        FsEntry::File(FsEntryFile { buffer_id, .. }) => {
                                            if *buffer_id == toggled_buffer_id {
                                                Some(fs_entry.clone())
                                            } else {
                                                None
                                            }
                                        }
                                        FsEntry::Directory(..) => None,
                                    },
                                )
                            })
                            .map(PanelEntry::Fs)
                    {
                        outline_panel.select_entry(entry_to_select, true, window, cx);
                    }

                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::Reparsed(buffer_id) => {
                    if let Some(buffer) = outline_panel.buffers.get_mut(buffer_id) {
                        buffer.invalidate_outlines();
                    }
                    let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                    if update_cached_items {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::OutlineSymbolsChanged => {
                    for buffer in outline_panel.buffers.values_mut() {
                        buffer.invalidate_outlines();
                    }
                    if matches!(
                        outline_panel.selected_entry(),
                        Some(PanelEntry::Outline(..)),
                    ) {
                        outline_panel.selected_entry.invalidate();
                    }
                    if outline_panel.update_non_fs_items(window, cx) {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::TitleChanged => {
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                _ => {}
            }
        },
    )
}

pub(super) fn empty_icon() -> AnyElement {
    h_flex()
        .size(IconSize::default().rems())
        .invisible()
        .flex_none()
        .into_any_element()
}
