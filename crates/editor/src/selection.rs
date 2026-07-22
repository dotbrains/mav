use super::*;

mod core;
mod delimiters;
mod gesture;
mod lines;
mod matches;
mod syntax;
mod syntax_helpers;

impl Editor {
    pub fn add_selection_above(
        &mut self,
        action: &AddSelectionAbove,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_selection(true, action.skip_soft_wrap, window, cx);
    }

    pub fn add_selection_below(
        &mut self,
        action: &AddSelectionBelow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_selection(false, action.skip_soft_wrap, window, cx);
    }

    fn begin_columnar_selection(
        &mut self,
        position: DisplayPoint,
        goal_column: u32,
        reset: bool,
        mode: ColumnarMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            self.last_focused_descendant = None;
            window.focus(&self.focus_handle, cx);
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));

        if reset {
            let pointer_position = display_map
                .buffer_snapshot()
                .anchor_before(position.to_point(&display_map));

            self.change_selections(
                SelectionEffects::scroll(Autoscroll::newest()),
                window,
                cx,
                |s| {
                    s.clear_disjoint();
                    s.set_pending_anchor_range(
                        pointer_position..pointer_position,
                        SelectMode::Character,
                    );
                },
            );
        };

        let tail = self.selections.newest::<Point>(&display_map).tail();
        let selection_anchor = display_map.buffer_snapshot().anchor_before(tail);
        self.columnar_selection_state = match mode {
            ColumnarMode::FromMouse => Some(ColumnarSelectionState::FromMouse {
                selection_tail: selection_anchor,
                display_point: if reset {
                    if position.column() != goal_column {
                        Some(DisplayPoint::new(position.row(), goal_column))
                    } else {
                        None
                    }
                } else {
                    None
                },
            }),
            ColumnarMode::FromSelection => Some(ColumnarSelectionState::FromSelection {
                selection_tail: selection_anchor,
            }),
        };

        if !reset {
            self.select_columns(position, goal_column, &display_map, window, cx);
        }
    }

    fn select_columns(
        &mut self,
        head: DisplayPoint,
        goal_column: u32,
        display_map: &DisplaySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(columnar_state) = self.columnar_selection_state.as_ref() else {
            return;
        };

        let tail = match columnar_state {
            ColumnarSelectionState::FromMouse {
                selection_tail,
                display_point,
            } => display_point.unwrap_or_else(|| selection_tail.to_display_point(display_map)),
            ColumnarSelectionState::FromSelection { selection_tail } => {
                selection_tail.to_display_point(display_map)
            }
        };

        let start_row = cmp::min(tail.row(), head.row());
        let end_row = cmp::max(tail.row(), head.row());

        // Anchor the columnar rectangle in x pixels rather than byte columns so
        // rows with multi-byte characters (e.g. diacritics) stay visually aligned.
        let text_layout_details = self.text_layout_details(window, cx);

        // The mouse handlers encode drags past a line's end as extra columns
        // beyond the line length, in em layout widths (see
        // `PositionMap::point_for_position`). `x_for_display_point` clamps at
        // the line's width, so convert that overshoot back to pixels with the
        // same unit to keep the rectangle tracking the mouse past short lines.
        let font_id = text_layout_details
            .text_system
            .resolve_font(&text_layout_details.editor_style.text.font());
        let font_size = text_layout_details
            .editor_style
            .text
            .font_size
            .to_pixels(text_layout_details.rem_size);
        let em_layout_width = text_layout_details
            .text_system
            .em_layout_width(font_id, font_size);
        let x_for_unclipped_point = |point: DisplayPoint| {
            let line_len = display_map.line_len(point.row());
            if point.column() > line_len {
                let eol_x = display_map.x_for_display_point(
                    DisplayPoint::new(point.row(), line_len),
                    &text_layout_details,
                );
                eol_x + em_layout_width * (point.column() - line_len) as f32
            } else {
                display_map.x_for_display_point(point, &text_layout_details)
            }
        };

        let tail_x = x_for_unclipped_point(tail);
        let head_x = x_for_unclipped_point(DisplayPoint::new(head.row(), goal_column));
        let start_x = tail_x.min(head_x);
        let end_x = tail_x.max(head_x);
        let reversed = head_x < tail_x;

        let selection_ranges = (start_row.0..=end_row.0)
            .map(|row| start_row + (row - start_row.0))
            .filter_map(|row| {
                if display_map.is_block_line(row) {
                    return None;
                }

                let layout = display_map.layout_row(row, &text_layout_details);
                if matches!(columnar_state, ColumnarSelectionState::FromSelection { .. })
                    && start_x > layout.width
                {
                    return None;
                }

                let start_column = layout.closest_index_for_x(start_x) as u32;
                let end_column = layout.closest_index_for_x(end_x) as u32;

                let start = display_map
                    .clip_point(DisplayPoint::new(row, start_column), Bias::Left)
                    .to_point(display_map);
                let end = display_map
                    .clip_point(DisplayPoint::new(row, end_column), Bias::Right)
                    .to_point(display_map);
                if reversed {
                    Some(end..start)
                } else {
                    Some(start..end)
                }
            })
            .collect::<Vec<_>>();
        if selection_ranges.is_empty() {
            return;
        }

        let ranges = match columnar_state {
            ColumnarSelectionState::FromMouse { .. } => {
                let mut non_empty_ranges = selection_ranges
                    .iter()
                    .filter(|selection_range| selection_range.start != selection_range.end)
                    .peekable();
                if non_empty_ranges.peek().is_some() {
                    non_empty_ranges.cloned().collect()
                } else {
                    selection_ranges
                }
            }
            _ => selection_ranges,
        };

        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(ranges);
        });
        cx.notify();
    }

    fn pending_selection_and_mode(&self) -> Option<(Selection<Anchor>, SelectMode)> {
        Some((
            self.selections.pending_anchor()?.clone(),
            self.selections.pending_mode()?,
        ))
    }

    fn add_selection(
        &mut self,
        above: bool,
        skip_soft_wrap: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let all_selections = self.selections.all::<Point>(&display_map);
        let text_layout_details = self.text_layout_details(window, cx);

        let (mut columnar_selections, new_selections_to_columnarize) = {
            if let Some(state) = self.add_selections_state.as_ref() {
                let columnar_selection_ids: HashSet<_> = state
                    .groups
                    .iter()
                    .flat_map(|group| group.stack.iter())
                    .copied()
                    .collect();

                all_selections
                    .into_iter()
                    .partition(|s| columnar_selection_ids.contains(&s.id))
            } else {
                (Vec::new(), all_selections)
            }
        };

        let mut state = self
            .add_selections_state
            .take()
            .unwrap_or_else(|| AddSelectionsState { groups: Vec::new() });

        for selection in new_selections_to_columnarize {
            let range = selection.display_range(&display_map).sorted();
            let start_x = display_map.x_for_display_point(range.start, &text_layout_details);
            let end_x = display_map.x_for_display_point(range.end, &text_layout_details);
            let positions = start_x.min(end_x)..start_x.max(end_x);
            let mut stack = Vec::new();
            let first_row = range.start.row();
            for row in first_row.0..=range.end.row().0 {
                if let Some(selection) = self.selections.build_columnar_selection(
                    &display_map,
                    first_row + (row - first_row.0),
                    &positions,
                    selection.reversed,
                    &text_layout_details,
                ) {
                    stack.push(selection.id);
                    columnar_selections.push(selection);
                }
            }
            if !stack.is_empty() {
                if above {
                    stack.reverse();
                }
                state.groups.push(AddSelectionsGroup { above, stack });
            }
        }

        let mut final_selections = Vec::new();
        let end_row = if above {
            let max_row = display_map.max_point().row();
            max_row - max_row.0
        } else {
            display_map.max_point().row()
        };

        // When `skip_soft_wrap` is true, we use UTF-16 columns instead of pixel
        // positions to place new selections, so we need to keep track of the
        // column range of the oldest selection in each group, because
        // intermediate selections may have been clamped to shorter lines.
        let mut goal_columns_by_selection_id = if skip_soft_wrap {
            let mut map = HashMap::default();
            for group in state.groups.iter() {
                if let Some(oldest_id) = group.stack.first() {
                    if let Some(oldest_selection) =
                        columnar_selections.iter().find(|s| s.id == *oldest_id)
                    {
                        let snapshot = display_map.buffer_snapshot();
                        let start_col =
                            snapshot.point_to_point_utf16(oldest_selection.start).column;
                        let end_col = snapshot.point_to_point_utf16(oldest_selection.end).column;
                        let goal_columns = start_col.min(end_col)..start_col.max(end_col);
                        for id in &group.stack {
                            map.insert(*id, goal_columns.clone());
                        }
                    }
                }
            }
            map
        } else {
            HashMap::default()
        };

        let mut last_added_item_per_group = HashMap::default();
        for group in state.groups.iter_mut() {
            if let Some(last_id) = group.stack.last() {
                last_added_item_per_group.insert(*last_id, group);
            }
        }

        for selection in columnar_selections {
            if let Some(group) = last_added_item_per_group.get_mut(&selection.id) {
                if above == group.above {
                    let range = selection.display_range(&display_map).sorted();
                    debug_assert_eq!(range.start.row(), range.end.row());
                    let row = range.start.row();
                    let positions =
                        if let SelectionGoal::HorizontalRange { start, end } = selection.goal {
                            Pixels::from(start)..Pixels::from(end)
                        } else {
                            let start_x =
                                display_map.x_for_display_point(range.start, &text_layout_details);
                            let end_x =
                                display_map.x_for_display_point(range.end, &text_layout_details);
                            start_x.min(end_x)..start_x.max(end_x)
                        };

                    let maybe_new_selection = if skip_soft_wrap {
                        let goal_columns = goal_columns_by_selection_id
                            .remove(&selection.id)
                            .unwrap_or_else(|| {
                                let snapshot = display_map.buffer_snapshot();
                                let start_col =
                                    snapshot.point_to_point_utf16(selection.start).column;
                                let end_col = snapshot.point_to_point_utf16(selection.end).column;
                                start_col.min(end_col)..start_col.max(end_col)
                            });
                        self.selections.find_next_columnar_selection_by_buffer_row(
                            &display_map,
                            row,
                            end_row,
                            above,
                            &goal_columns,
                            selection.reversed,
                            &text_layout_details,
                        )
                    } else {
                        self.selections.find_next_columnar_selection_by_display_row(
                            &display_map,
                            row,
                            end_row,
                            above,
                            &positions,
                            selection.reversed,
                            &text_layout_details,
                        )
                    };

                    if let Some(new_selection) = maybe_new_selection {
                        group.stack.push(new_selection.id);
                        if above {
                            final_selections.push(new_selection);
                            final_selections.push(selection);
                        } else {
                            final_selections.push(selection);
                            final_selections.push(new_selection);
                        }
                    } else {
                        final_selections.push(selection);
                    }
                } else {
                    group.stack.pop();
                }
            } else {
                final_selections.push(selection);
            }
        }

        self.change_selections(Default::default(), window, cx, |s| {
            s.select(final_selections);
        });

        let final_selection_ids: HashSet<_> = self
            .selections
            .all::<Point>(&display_map)
            .iter()
            .map(|s| s.id)
            .collect();
        state.groups.retain_mut(|group| {
            // selections might get merged above so we remove invalid items from stacks
            group.stack.retain(|id| final_selection_ids.contains(id));

            // single selection in stack can be treated as initial state
            group.stack.len() > 1
        });

        if !state.groups.is_empty() {
            self.add_selections_state = Some(state);
        }
    }
}
