use super::*;

impl OutlinePanel {
    pub(super) fn reveal_entry_for_selection(
        &mut self,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active
            || !OutlinePanelSettings::get_global(cx).auto_reveal_entries
            || self.focus_handle.contains_focused(window, cx)
        {
            return;
        }
        let project = self.project.clone();
        self.reveal_selection_task = cx.spawn_in(window, async move |outline_panel, cx| {
            cx.background_executor().timer(UPDATE_DEBOUNCE).await;
            let multibuffer_snapshot =
                editor.read_with(cx, |editor, cx| editor.buffer().read(cx).snapshot(cx));
            let entry_with_selection =
                outline_panel.update_in(cx, |outline_panel, window, cx| {
                    outline_panel.location_for_editor_selection(&editor, window, cx)
                })?;
            let Some(entry_with_selection) = entry_with_selection else {
                outline_panel.update(cx, |outline_panel, cx| {
                    outline_panel.selected_entry = SelectedEntry::None;
                    cx.notify();
                })?;
                return Ok(());
            };
            let related_buffer_entry = match &entry_with_selection {
                PanelEntry::Fs(FsEntry::File(FsEntryFile {
                    worktree_id,
                    buffer_id,
                    ..
                })) => project.update(cx, |project, cx| {
                    let entry_id = project
                        .buffer_for_id(*buffer_id, cx)
                        .and_then(|buffer| buffer.read(cx).entry_id(cx));
                    project
                        .worktree_for_id(*worktree_id, cx)
                        .zip(entry_id)
                        .and_then(|(worktree, entry_id)| {
                            let entry = worktree.read(cx).entry_for_id(entry_id)?.clone();
                            Some((worktree, entry))
                        })
                }),
                PanelEntry::Outline(outline_entry) => {
                    let buffer_id = outline_entry.buffer_id();
                    let outline_range = outline_entry.range();
                    outline_panel.update(cx, |outline_panel, cx| {
                        outline_panel
                            .collapsed_entries
                            .remove(&CollapsedEntry::ExternalFile(buffer_id));
                        if let Some(buffer_snapshot) =
                            outline_panel.buffer_snapshot_for_id(buffer_id, cx)
                        {
                            outline_panel.collapsed_entries.retain(|entry| match entry {
                                CollapsedEntry::Excerpt(excerpt_range) => {
                                    let intersects = excerpt_range.context.start.buffer_id
                                        == buffer_id
                                        && (excerpt_range
                                            .contains(&outline_range.start, &buffer_snapshot)
                                            || excerpt_range
                                                .contains(&outline_range.end, &buffer_snapshot));
                                    !intersects
                                }
                                _ => true,
                            });
                        }
                        let project = outline_panel.project.read(cx);
                        let entry_id = project
                            .buffer_for_id(buffer_id, cx)
                            .and_then(|buffer| buffer.read(cx).entry_id(cx));

                        entry_id.and_then(|entry_id| {
                            project
                                .worktree_for_entry(entry_id, cx)
                                .and_then(|worktree| {
                                    let worktree_id = worktree.read(cx).id();
                                    outline_panel
                                        .collapsed_entries
                                        .remove(&CollapsedEntry::File(worktree_id, buffer_id));
                                    let entry = worktree.read(cx).entry_for_id(entry_id)?.clone();
                                    Some((worktree, entry))
                                })
                        })
                    })?
                }
                PanelEntry::Fs(FsEntry::ExternalFile(..)) => None,
                PanelEntry::Search(SearchEntry { match_range, .. }) => multibuffer_snapshot
                    .anchor_to_buffer_anchor(match_range.start)
                    .map(|(anchor, _)| anchor.buffer_id)
                    .map(|buffer_id| {
                        outline_panel.update(cx, |outline_panel, cx| {
                            outline_panel
                                .collapsed_entries
                                .remove(&CollapsedEntry::ExternalFile(buffer_id));
                            let project = project.read(cx);
                            let entry_id = project
                                .buffer_for_id(buffer_id, cx)
                                .and_then(|buffer| buffer.read(cx).entry_id(cx));

                            entry_id.and_then(|entry_id| {
                                project
                                    .worktree_for_entry(entry_id, cx)
                                    .and_then(|worktree| {
                                        let worktree_id = worktree.read(cx).id();
                                        outline_panel
                                            .collapsed_entries
                                            .remove(&CollapsedEntry::File(worktree_id, buffer_id));
                                        let entry =
                                            worktree.read(cx).entry_for_id(entry_id)?.clone();
                                        Some((worktree, entry))
                                    })
                            })
                        })
                    })
                    .transpose()?
                    .flatten(),
                _ => return anyhow::Ok(()),
            };
            if let Some((worktree, buffer_entry)) = related_buffer_entry {
                outline_panel.update(cx, |outline_panel, cx| {
                    let worktree_id = worktree.read(cx).id();
                    let mut dirs_to_expand = Vec::new();
                    {
                        let mut traversal = worktree.read(cx).traverse_from_path(
                            true,
                            true,
                            true,
                            buffer_entry.path.as_ref(),
                        );
                        let mut current_entry = buffer_entry;
                        loop {
                            if current_entry.is_dir()
                                && outline_panel
                                    .collapsed_entries
                                    .remove(&CollapsedEntry::Dir(worktree_id, current_entry.id))
                            {
                                dirs_to_expand.push(current_entry.id);
                            }

                            if traversal.back_to_parent()
                                && let Some(parent_entry) = traversal.entry()
                            {
                                current_entry = parent_entry.clone();
                                continue;
                            }
                            break;
                        }
                    }
                    for dir_to_expand in dirs_to_expand {
                        project
                            .update(cx, |project, cx| {
                                project.expand_entry(worktree_id, dir_to_expand, cx)
                            })
                            .unwrap_or_else(|| Task::ready(Ok(())))
                            .detach_and_log_err(cx)
                    }
                })?
            }

            outline_panel.update_in(cx, |outline_panel, window, cx| {
                outline_panel.select_entry(entry_with_selection, false, window, cx);
                outline_panel.update_cached_entries(None, window, cx);
            })?;

            anyhow::Ok(())
        });
    }

    pub(super) fn location_for_editor_selection(
        &self,
        editor: &Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PanelEntry> {
        let editor_snapshot = editor.update(cx, |editor, cx| editor.snapshot(window, cx));
        let multi_buffer = editor.read(cx).buffer();
        let multi_buffer_snapshot = multi_buffer.read(cx).snapshot(cx);
        let anchor = editor.update(cx, |editor, _| editor.selections.newest_anchor().head());
        let selection_display_point = anchor.to_display_point(&editor_snapshot);
        let (anchor, _) = multi_buffer_snapshot.anchor_to_buffer_anchor(anchor)?;

        if editor.read(cx).is_buffer_folded(anchor.buffer_id, cx) {
            return self
                .fs_entries
                .iter()
                .find(|fs_entry| match fs_entry {
                    FsEntry::Directory(..) => false,
                    FsEntry::File(FsEntryFile {
                        buffer_id: other_buffer_id,
                        ..
                    })
                    | FsEntry::ExternalFile(FsEntryExternalFile {
                        buffer_id: other_buffer_id,
                        ..
                    }) => anchor.buffer_id == *other_buffer_id,
                })
                .cloned()
                .map(PanelEntry::Fs);
        }

        match &self.mode {
            ItemsDisplayMode::Search(search_state) => search_state
                .matches
                .iter()
                .rev()
                .min_by_key(|&(match_range, _)| {
                    let match_display_range =
                        match_range.clone().to_display_points(&editor_snapshot);
                    let start_distance = if selection_display_point < match_display_range.start {
                        match_display_range.start - selection_display_point
                    } else {
                        selection_display_point - match_display_range.start
                    };
                    let end_distance = if selection_display_point < match_display_range.end {
                        match_display_range.end - selection_display_point
                    } else {
                        selection_display_point - match_display_range.end
                    };
                    start_distance + end_distance
                })
                .and_then(|(closest_range, _)| {
                    self.cached_entries.iter().find_map(|cached_entry| {
                        if let PanelEntry::Search(SearchEntry { match_range, .. }) =
                            &cached_entry.entry
                        {
                            if match_range == closest_range {
                                Some(cached_entry.entry.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                }),
            ItemsDisplayMode::Outline => self.outline_location(
                anchor,
                multi_buffer_snapshot,
                editor_snapshot,
                selection_display_point,
                cx,
            ),
        }
    }

    pub(super) fn outline_location(
        &self,
        selection_anchor: Anchor,
        multi_buffer_snapshot: editor::MultiBufferSnapshot,
        editor_snapshot: editor::EditorSnapshot,
        selection_display_point: DisplayPoint,
        cx: &App,
    ) -> Option<PanelEntry> {
        let excerpt_outlines = self
            .buffers
            .get(&selection_anchor.buffer_id)
            .into_iter()
            .flat_map(|buffer| buffer.iter_outlines())
            .flat_map(|outline| {
                let range = multi_buffer_snapshot
                    .buffer_anchor_range_to_anchor_range(outline.range.clone())?;
                Some((
                    range.start.to_display_point(&editor_snapshot)
                        ..range.end.to_display_point(&editor_snapshot),
                    outline,
                ))
            })
            .collect::<Vec<_>>();

        let mut matching_outline_indices = Vec::new();
        let mut children = HashMap::default();
        let mut parents_stack = Vec::<(&Range<DisplayPoint>, &&Outline, usize)>::new();

        for (i, (outline_range, outline)) in excerpt_outlines.iter().enumerate() {
            if outline_range
                .to_inclusive()
                .contains(&selection_display_point)
            {
                matching_outline_indices.push(i);
            } else if (outline_range.start.row()..outline_range.end.row())
                .to_inclusive()
                .contains(&selection_display_point.row())
            {
                matching_outline_indices.push(i);
            }

            while let Some((parent_range, parent_outline, _)) = parents_stack.last() {
                if parent_outline.depth >= outline.depth
                    || !parent_range.contains(&outline_range.start)
                {
                    parents_stack.pop();
                } else {
                    break;
                }
            }
            if let Some((_, _, parent_index)) = parents_stack.last_mut() {
                children
                    .entry(*parent_index)
                    .or_insert_with(Vec::new)
                    .push(i);
            }
            parents_stack.push((outline_range, outline, i));
        }

        let outline_item = matching_outline_indices
            .into_iter()
            .flat_map(|i| Some((i, excerpt_outlines.get(i)?)))
            .filter(|(i, _)| {
                children
                    .get(i)
                    .map(|children| {
                        children.iter().all(|child_index| {
                            excerpt_outlines
                                .get(*child_index)
                                .map(|(child_range, _)| child_range.start > selection_display_point)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(true)
            })
            .min_by_key(|(_, (outline_range, outline))| {
                let distance_from_start = if outline_range.start > selection_display_point {
                    outline_range.start - selection_display_point
                } else {
                    selection_display_point - outline_range.start
                };
                let distance_from_end = if outline_range.end > selection_display_point {
                    outline_range.end - selection_display_point
                } else {
                    selection_display_point - outline_range.end
                };

                // An outline item's range can extend to the same row the next
                // item starts on, so when the cursor is at the start of that
                // row, prefer the item that starts there over any item whose
                // range merely overlaps that row.
                let cursor_not_at_outline_start = outline_range.start != selection_display_point;
                (
                    cursor_not_at_outline_start,
                    cmp::Reverse(outline.depth),
                    distance_from_start,
                    distance_from_end,
                )
            })
            .map(|(_, (_, outline))| *outline)
            .cloned();

        let closest_container = match outline_item {
            Some(outline) => PanelEntry::Outline(OutlineEntry::Outline(outline)),
            None => {
                self.cached_entries.iter().rev().find_map(|cached_entry| {
                    match &cached_entry.entry {
                        PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                            if excerpt.context.start.buffer_id == selection_anchor.buffer_id
                                && let Some(buffer_snapshot) =
                                    self.buffer_snapshot_for_id(excerpt.context.start.buffer_id, cx)
                                && excerpt.contains(&selection_anchor, &buffer_snapshot)
                            {
                                Some(cached_entry.entry.clone())
                            } else {
                                None
                            }
                        }
                        PanelEntry::Fs(
                            FsEntry::ExternalFile(FsEntryExternalFile {
                                buffer_id: file_buffer_id,
                                excerpts: file_excerpts,
                                ..
                            })
                            | FsEntry::File(FsEntryFile {
                                buffer_id: file_buffer_id,
                                excerpts: file_excerpts,
                                ..
                            }),
                        ) => {
                            if *file_buffer_id == selection_anchor.buffer_id
                                && let Some(buffer_snapshot) =
                                    self.buffer_snapshot_for_id(*file_buffer_id, cx)
                                && file_excerpts.iter().any(|excerpt| {
                                    excerpt.contains(&selection_anchor, &buffer_snapshot)
                                })
                            {
                                Some(cached_entry.entry.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                })?
            }
        };
        Some(closest_container)
    }
}
