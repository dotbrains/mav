use super::*;

impl Editor {
    pub fn go_to_reference_before_or_after_position(
        &mut self,
        direction: Direction,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let selection = self.selections.newest_anchor();
        let head = selection.head();

        let multi_buffer = self.buffer.read(cx);

        let (buffer, text_head) = multi_buffer.text_anchor_for_position(head, cx)?;
        let workspace = self.workspace()?;
        let project = workspace.read(cx).project().clone();
        let references =
            project.update(cx, |project, cx| project.references(&buffer, text_head, cx));
        Some(cx.spawn_in(window, async move |editor, cx| -> Result<()> {
            let Some(locations) = references.await? else {
                return Ok(());
            };

            if locations.is_empty() {
                // totally normal - the cursor may be on something which is not
                // a symbol (e.g. a keyword)
                log::info!("no references found under cursor");
                return Ok(());
            }

            let multi_buffer = editor.read_with(cx, |editor, _| editor.buffer().clone())?;

            let (locations, current_location_index) =
                multi_buffer.update(cx, |multi_buffer, cx| {
                    let multi_buffer_snapshot = multi_buffer.snapshot(cx);
                    let mut locations = locations
                        .into_iter()
                        .filter_map(|loc| {
                            let start = multi_buffer_snapshot.anchor_in_excerpt(loc.range.start)?;
                            let end = multi_buffer_snapshot.anchor_in_excerpt(loc.range.end)?;
                            Some(start..end)
                        })
                        .collect::<Vec<_>>();
                    // There is an O(n) implementation, but given this list will be
                    // small (usually <100 items), the extra O(log(n)) factor isn't
                    // worth the (surprisingly large amount of) extra complexity.
                    locations
                        .sort_unstable_by(|l, r| l.start.cmp(&r.start, &multi_buffer_snapshot));

                    let head_offset = head.to_offset(&multi_buffer_snapshot);

                    let current_location_index = locations.iter().position(|loc| {
                        loc.start.to_offset(&multi_buffer_snapshot) <= head_offset
                            && loc.end.to_offset(&multi_buffer_snapshot) >= head_offset
                    });

                    (locations, current_location_index)
                });

            let Some(current_location_index) = current_location_index else {
                // This indicates something has gone wrong, because we already
                // handle the "no references" case above
                log::error!(
                    "failed to find current reference under cursor. Total references: {}",
                    locations.len()
                );
                return Ok(());
            };

            let destination_location_index = match direction {
                Direction::Next => (current_location_index + count) % locations.len(),
                Direction::Prev => {
                    (current_location_index + locations.len() - count % locations.len())
                        % locations.len()
                }
            };

            // TODO(cameron): is this needed?
            // the thinking is to avoid "jumping to the current location" (avoid
            // polluting "jumplist" in vim terms)
            if current_location_index == destination_location_index {
                return Ok(());
            }

            let Range { start, end } = locations[destination_location_index];

            editor.update_in(cx, |editor, window, cx| {
                let effects = SelectionEffects::scroll(Autoscroll::for_go_to_definition(
                    editor.cursor_top_offset(cx),
                    cx,
                ));

                editor.unfold_ranges(&[start..end], false, false, cx);
                editor.change_selections(effects, window, cx, |s| {
                    s.select_ranges([start..start]);
                });
            })?;

            Ok(())
        }))
    }

    pub fn find_all_references(
        &mut self,
        action: &FindAllReferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<Navigated>>> {
        let always_open_multibuffer = action.always_open_multibuffer;
        let selection = self.selections.newest_anchor();
        let multi_buffer = self.buffer.read(cx);
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);
        let selection_offset = selection.map(|anchor| anchor.to_offset(&multi_buffer_snapshot));
        let selection_point = selection.map(|anchor| anchor.to_point(&multi_buffer_snapshot));
        let head = selection_offset.head();

        let head_anchor = multi_buffer_snapshot.anchor_at(
            head,
            if head < selection_offset.tail() {
                Bias::Right
            } else {
                Bias::Left
            },
        );

        match self
            .find_all_references_task_sources
            .binary_search_by(|anchor| anchor.cmp(&head_anchor, &multi_buffer_snapshot))
        {
            Ok(_) => {
                log::info!(
                    "Ignoring repeated FindAllReferences invocation with the position of already running task"
                );
                return None;
            }
            Err(i) => {
                self.find_all_references_task_sources.insert(i, head_anchor);
            }
        }

        let (buffer, head) = multi_buffer.text_anchor_for_position(head, cx)?;
        let workspace = self.workspace()?;
        let project = workspace.read(cx).project().clone();
        let references = project.update(cx, |project, cx| project.references(&buffer, head, cx));
        Some(cx.spawn_in(window, async move |editor, cx| {
            let _cleanup = cx.on_drop(&editor, move |editor, _| {
                if let Ok(i) = editor
                    .find_all_references_task_sources
                    .binary_search_by(|anchor| anchor.cmp(&head_anchor, &multi_buffer_snapshot))
                {
                    editor.find_all_references_task_sources.remove(i);
                }
            });

            let Some(locations) = references.await? else {
                return anyhow::Ok(Navigated::No);
            };
            let mut locations = cx.update(|_, cx| {
                locations
                    .into_iter()
                    .map(|location| {
                        let buffer = location.buffer.read(cx);
                        (location.buffer, location.range.to_point(buffer))
                    })
                    // if special-casing the single-match case, remove ranges
                    // that intersect current selection
                    .filter(|(location_buffer, location)| {
                        if always_open_multibuffer || &buffer != location_buffer {
                            return true;
                        }

                        !location.contains_inclusive(&selection_point.range())
                    })
                    .into_group_map()
            })?;
            if locations.is_empty() {
                return anyhow::Ok(Navigated::No);
            }
            for ranges in locations.values_mut() {
                ranges.sort_unstable_by_key(|range| (range.start, Reverse(range.end)));
                ranges.dedup();
            }
            let mut num_locations = 0;
            for ranges in locations.values_mut() {
                ranges.sort_unstable_by_key(|range| (range.start, Reverse(range.end)));
                ranges.dedup();
                num_locations += ranges.len();
            }

            if num_locations == 1 && !always_open_multibuffer {
                let Some((target_buffer, target_ranges)) = locations.into_iter().next() else {
                    return anyhow::Ok(Navigated::No);
                };
                let Some(target_range) = target_ranges.first().cloned() else {
                    return anyhow::Ok(Navigated::No);
                };

                return editor.update_in(cx, |editor, window, cx| {
                    let range = target_range.to_point(target_buffer.read(cx));
                    let range = editor.range_for_match(&range);
                    let range = range.start..range.start;

                    if Some(&target_buffer) == editor.buffer.read(cx).as_singleton().as_ref() {
                        editor.go_to_singleton_buffer_range(range, window, cx);
                    } else {
                        let pane = workspace.read(cx).active_pane().clone();
                        window.defer(cx, move |window, cx| {
                            let target_editor: Entity<Self> =
                                workspace.update(cx, |workspace, cx| {
                                    let pane = workspace.active_pane().clone();

                                    let preview_tabs_settings = PreviewTabsSettings::get_global(cx);
                                    let keep_old_preview = preview_tabs_settings
                                        .enable_keep_preview_on_code_navigation;
                                    let allow_new_preview = preview_tabs_settings
                                        .enable_preview_file_from_code_navigation;

                                    workspace.open_project_item(
                                        pane,
                                        target_buffer.clone(),
                                        true,
                                        true,
                                        keep_old_preview,
                                        allow_new_preview,
                                        window,
                                        cx,
                                    )
                                });
                            target_editor.update(cx, |target_editor, cx| {
                                // When selecting a definition in a different buffer, disable the nav history
                                // to avoid creating a history entry at the previous cursor location.
                                pane.update(cx, |pane, _| pane.disable_history());
                                target_editor.go_to_singleton_buffer_range(range, window, cx);
                                pane.update(cx, |pane, _| pane.enable_history());
                            });
                        });
                    }
                    Navigated::No
                });
            }

            workspace.update_in(cx, |workspace, window, cx| {
                let target = locations
                    .iter()
                    .flat_map(|(k, v)| iter::repeat(k.clone()).zip(v))
                    .map(|(buffer, location)| {
                        buffer
                            .read(cx)
                            .text_for_range(location.clone())
                            .collect::<String>()
                    })
                    .filter(|text| !text.contains('\n'))
                    .unique()
                    .take(3)
                    .join(", ");
                let title = if target.is_empty() {
                    "References".to_owned()
                } else {
                    format!("References to {target}")
                };
                let allow_preview = PreviewTabsSettings::get_global(cx)
                    .enable_preview_multibuffer_from_code_navigation;
                Self::open_locations_in_multibuffer(
                    workspace,
                    locations,
                    title,
                    false,
                    allow_preview,
                    MultibufferSelectionMode::First,
                    window,
                    cx,
                );
                Navigated::Yes
            })
        }))
    }

    pub(crate) fn navigation_entry(
        &self,
        cursor_anchor: Anchor,
        cx: &mut Context<Self>,
    ) -> Option<NavigationEntry> {
        let Some(history) = self.nav_history.clone() else {
            return None;
        };
        let data = self.navigation_data(cursor_anchor, cx);
        Some(history.navigation_entry(Some(Arc::new(data) as Arc<dyn Any + Send + Sync>)))
    }

    pub(crate) fn push_to_nav_history(
        &mut self,
        cursor_anchor: Anchor,
        new_position: Option<Point>,
        is_deactivate: bool,
        always: bool,
        cx: &mut Context<Self>,
    ) {
        let data = self.navigation_data(cursor_anchor, cx);
        if let Some(nav_history) = self.nav_history.as_mut() {
            if let Some(new_position) = new_position {
                let row_delta = (new_position.row as i64 - data.cursor_position.row as i64).abs();
                if row_delta == 0 || (row_delta < MIN_NAVIGATION_HISTORY_ROW_DELTA && !always) {
                    return;
                }
            }

            let cursor_row = data.cursor_position.row;
            nav_history.push(Some(data), Some(cursor_row), cx);
            cx.emit(EditorEvent::PushedToNavHistory {
                anchor: cursor_anchor,
                is_deactivate,
            })
        }
    }

    pub(crate) fn expand_excerpt(
        &mut self,
        excerpt_anchor: Anchor,
        direction: ExpandExcerptDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lines_to_expand = EditorSettings::get_global(cx).expand_excerpt_lines;

        if self.delegate_expand_excerpts {
            cx.emit(EditorEvent::ExpandExcerptsRequested {
                excerpt_anchors: vec![excerpt_anchor],
                lines: lines_to_expand,
                direction,
            });
            return;
        }

        let current_scroll_position = self.scroll_position(cx);
        let mut scroll = None;

        if direction == ExpandExcerptDirection::Down {
            let multi_buffer = self.buffer.read(cx);
            let snapshot = multi_buffer.snapshot(cx);
            if let Some((buffer_snapshot, excerpt_range)) =
                snapshot.excerpt_containing(excerpt_anchor..excerpt_anchor)
            {
                let excerpt_end_row =
                    Point::from_anchor(&excerpt_range.context.end, &buffer_snapshot).row;
                let last_row = buffer_snapshot.max_point().row;
                let lines_below = last_row.saturating_sub(excerpt_end_row);
                if lines_below >= lines_to_expand {
                    scroll = Some(
                        current_scroll_position
                            + gpui::Point::new(0.0, lines_to_expand as ScrollOffset),
                    );
                }
            }
        }
        if direction == ExpandExcerptDirection::Up
            && self
                .buffer
                .read(cx)
                .snapshot(cx)
                .excerpt_before(excerpt_anchor)
                .is_none()
        {
            scroll = Some(current_scroll_position);
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.expand_excerpts([excerpt_anchor], lines_to_expand, direction, cx)
        });

        if let Some(new_scroll_position) = scroll {
            self.set_scroll_position(new_scroll_position, window, cx);
        }
    }

    pub(crate) fn go_to_next_change(
        &mut self,
        _: &GoToNextChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selections) = self
            .change_list
            .next_change(1, Direction::Next)
            .map(|s| s.to_vec())
        {
            self.change_selections(Default::default(), window, cx, |s| {
                let map = s.display_snapshot();
                s.select_display_ranges(selections.iter().map(|a| {
                    let point = a.to_display_point(&map);
                    point..point
                }))
            })
        }
    }

    pub(crate) fn go_to_previous_change(
        &mut self,
        _: &GoToPreviousChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selections) = self
            .change_list
            .next_change(1, Direction::Prev)
            .map(|s| s.to_vec())
        {
            self.change_selections(Default::default(), window, cx, |s| {
                let map = s.display_snapshot();
                s.select_display_ranges(selections.iter().map(|a| {
                    let point = a.to_display_point(&map);
                    point..point
                }))
            })
        }
    }

    pub(crate) fn go_to_line<T: 'static>(
        &mut self,
        position: Anchor,
        highlight_color: fn(&App) -> Hsla,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(window, cx).display_snapshot;
        let position = position.to_point(&snapshot.buffer_snapshot());
        let start = snapshot
            .buffer_snapshot()
            .clip_point(Point::new(position.row, 0), Bias::Left);
        let end = start + Point::new(1, 0);
        let start = snapshot.buffer_snapshot().anchor_before(start);
        let end = snapshot.buffer_snapshot().anchor_before(end);

        self.highlight_rows::<T>(start..end, highlight_color, Default::default(), cx);

        if self.buffer.read(cx).is_singleton() {
            self.request_autoscroll(Autoscroll::center().for_anchor(start), cx);
        }
    }
}
