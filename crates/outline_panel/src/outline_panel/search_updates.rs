use super::*;

impl OutlinePanel {
    pub(super) fn update_non_fs_items(
        &mut self,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> bool {
        if !self.active {
            return false;
        }

        let mut update_cached_items = false;
        update_cached_items |= self.update_search_matches(window, cx);
        self.fetch_outdated_outlines(window, cx);
        if update_cached_items {
            self.selected_entry.invalidate();
        }
        update_cached_items
    }

    pub(super) fn update_search_matches(
        &mut self,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> bool {
        if !self.active {
            return false;
        }

        let project_search = self
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>());
        let project_search_matches = project_search
            .as_ref()
            .map(|project_search| project_search.read(cx).get_matches(cx))
            .unwrap_or_default();

        let buffer_search = self
            .active_item()
            .as_deref()
            .and_then(|active_item| {
                self.workspace
                    .upgrade()
                    .and_then(|workspace| workspace.read(cx).pane_for(active_item))
            })
            .and_then(|pane| {
                pane.read(cx)
                    .toolbar()
                    .read(cx)
                    .item_of_type::<BufferSearchBar>()
            });
        let buffer_search_matches = self
            .active_editor()
            .map(|active_editor| {
                active_editor.update(cx, |editor, cx| editor.get_matches(window, cx).0)
            })
            .unwrap_or_default();

        let mut update_cached_entries = false;
        if buffer_search_matches.is_empty() && project_search_matches.is_empty() {
            if matches!(self.mode, ItemsDisplayMode::Search(_)) {
                self.mode = ItemsDisplayMode::Outline;
                update_cached_entries = true;
            }
        } else {
            let (kind, new_search_matches, new_search_query) = if buffer_search_matches.is_empty() {
                (
                    SearchKind::Project,
                    project_search_matches,
                    project_search
                        .map(|project_search| project_search.read(cx).search_query_text(cx))
                        .unwrap_or_default(),
                )
            } else {
                (
                    SearchKind::Buffer,
                    buffer_search_matches,
                    buffer_search
                        .map(|buffer_search| buffer_search.read(cx).query(cx))
                        .unwrap_or_default(),
                )
            };

            let changed = match &self.mode {
                ItemsDisplayMode::Search(current) => {
                    current.query != new_search_query
                        || current.kind != kind
                        || current.matches.len() != new_search_matches.len()
                        || current
                            .matches
                            .iter()
                            .zip(&new_search_matches)
                            .any(|((existing, _), incoming)| existing != incoming)
                }
                ItemsDisplayMode::Outline => true,
            };
            if changed {
                let previous_matches = match &mut self.mode {
                    ItemsDisplayMode::Search(current) if current.kind == kind => {
                        current.matches.drain(..).collect()
                    }
                    _ => HashMap::default(),
                };
                self.mode = ItemsDisplayMode::Search(SearchState::new(
                    kind,
                    new_search_query,
                    previous_matches,
                    new_search_matches,
                    cx.theme().syntax().clone(),
                    window,
                    cx,
                ));
                update_cached_entries = true;
            }
        }
        update_cached_entries
    }

    pub(super) fn add_buffer_entries(
        &mut self,
        state: &mut GenerationState,
        buffer_id: BufferId,
        parent_depth: usize,
        track_matches: bool,
        is_singleton: bool,
        query: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return;
        };

        let buffer_snapshot = self.buffer_snapshot_for_id(buffer_id, cx);

        for excerpt in &buffer.excerpts {
            let excerpt_depth = parent_depth + 1;
            self.push_entry(
                state,
                track_matches,
                PanelEntry::Outline(OutlineEntry::Excerpt(excerpt.clone())),
                excerpt_depth,
                cx,
            );

            let mut outline_base_depth = excerpt_depth + 1;
            if is_singleton {
                outline_base_depth = 0;
                state.clear();
            } else if query.is_none()
                && self
                    .collapsed_entries
                    .contains(&CollapsedEntry::Excerpt(excerpt.clone()))
            {
                continue;
            }

            let mut last_depth_at_level: Vec<Option<Range<Anchor>>> = vec![None; 10];

            let all_outlines: Vec<_> = buffer.iter_outlines().collect();

            let mut outline_has_children = HashMap::default();
            let mut visible_outlines = Vec::new();
            let mut collapsed_state: Option<(usize, Range<Anchor>)> = None;

            for (i, &outline) in all_outlines.iter().enumerate() {
                let has_children = all_outlines
                    .get(i + 1)
                    .map(|next| next.depth > outline.depth)
                    .unwrap_or(false);

                outline_has_children.insert((outline.range.clone(), outline.depth), has_children);

                let mut should_include = true;

                if let Some((collapsed_depth, collapsed_range)) = &collapsed_state {
                    if outline.depth <= *collapsed_depth {
                        collapsed_state = None;
                    } else if let Some(buffer_snapshot) = buffer_snapshot.as_ref() {
                        let outline_start = outline.range.start;
                        if outline_start
                            .cmp(&collapsed_range.start, buffer_snapshot)
                            .is_ge()
                            && outline_start
                                .cmp(&collapsed_range.end, buffer_snapshot)
                                .is_lt()
                        {
                            should_include = false; // Skip - inside collapsed range
                        } else {
                            collapsed_state = None;
                        }
                    }
                }

                // Check if this outline itself is collapsed
                if should_include
                    && self
                        .collapsed_entries
                        .contains(&CollapsedEntry::Outline(outline.range.clone()))
                {
                    collapsed_state = Some((outline.depth, outline.range.clone()));
                }

                if should_include {
                    visible_outlines.push(outline);
                }
            }

            self.outline_children_cache
                .entry(buffer_id)
                .or_default()
                .extend(outline_has_children);

            for outline in visible_outlines {
                let outline_entry = outline.clone();

                if outline.depth < last_depth_at_level.len() {
                    last_depth_at_level[outline.depth] = Some(outline.range.clone());
                    // Clear deeper levels when we go back to a shallower depth
                    for d in (outline.depth + 1)..last_depth_at_level.len() {
                        last_depth_at_level[d] = None;
                    }
                }

                self.push_entry(
                    state,
                    track_matches,
                    PanelEntry::Outline(OutlineEntry::Outline(outline_entry)),
                    outline_base_depth + outline.depth,
                    cx,
                );
            }
        }
    }

    pub(super) fn add_search_entries(
        &mut self,
        state: &mut GenerationState,
        search: &SearchPrecomputed,
        parent_entry: &FsEntry,
        parent_depth: usize,
        track_matches: bool,
        is_singleton: bool,
        cx: &mut Context<Self>,
    ) {
        let ItemsDisplayMode::Search(search_state) = &self.mode else {
            return;
        };
        let kind = search_state.kind;

        let (buffer_id, excerpts) = match parent_entry {
            FsEntry::Directory(_) => return,
            FsEntry::ExternalFile(external) => (external.buffer_id, &external.excerpts),
            FsEntry::File(file) => (file.buffer_id, &file.excerpts),
        };

        if search.folded_buffers.contains(&buffer_id) {
            return;
        }
        let Some(buffer_matches) = search.matches_by_buffer.get(&buffer_id) else {
            return;
        };

        let excerpt_ranges = excerpts
            .iter()
            .filter_map(|excerpt| {
                let start = search
                    .multi_buffer_snapshot
                    .anchor_in_buffer(excerpt.context.start)?;
                let end = search
                    .multi_buffer_snapshot
                    .anchor_in_buffer(excerpt.context.end)?;
                Some(start..end)
            })
            .collect::<Vec<_>>();

        let depth = if is_singleton { 0 } else { parent_depth + 1 };
        for (match_range, search_data) in buffer_matches.iter().filter(|(match_range, _)| {
            excerpt_ranges.iter().any(|excerpt_range| {
                excerpt_range.overlaps(match_range, &search.multi_buffer_snapshot)
            })
        }) {
            self.push_entry(
                state,
                track_matches,
                PanelEntry::Search(SearchEntry {
                    match_range: match_range.clone(),
                    kind,
                    render_data: Arc::clone(search_data),
                }),
                depth,
                cx,
            );
        }
    }
}
