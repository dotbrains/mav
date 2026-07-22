use super::*;

impl Vim {
    pub fn visual_motion(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |vim, editor, cx| {
            let text_layout_details = editor.text_layout_details(window, cx);
            if vim.mode == Mode::VisualBlock
                && !matches!(
                    motion,
                    Motion::EndOfLine {
                        display_lines: false
                    }
                )
            {
                let is_up_or_down = matches!(motion, Motion::Up { .. } | Motion::Down { .. });
                vim.visual_block_motion(
                    is_up_or_down,
                    editor,
                    window,
                    cx,
                    &mut |map, point, goal| {
                        motion.move_point(map, point, goal, times, &text_layout_details)
                    },
                )
            } else {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let was_reversed = selection.reversed;
                        let mut current_head = selection.head();

                        // our motions assume the current character is after the cursor,
                        // but in (forward) visual mode the current character is just
                        // before the end of the selection.

                        // If the file ends with a newline (which is common) we don't do this.
                        // so that if you go to the end of such a file you can use "up" to go
                        // to the previous line and have it work somewhat as expected.
                        if !selection.reversed
                            && !selection.is_empty()
                            && !(selection.end.column() == 0 && selection.end == map.max_point())
                        {
                            current_head = movement::left(map, selection.end)
                        }

                        let Some((new_head, goal)) = motion.move_point(
                            map,
                            current_head,
                            selection.goal,
                            times,
                            &text_layout_details,
                        ) else {
                            return;
                        };

                        selection.set_head(new_head, goal);

                        // ensure the current character is included in the selection.
                        if !selection.reversed {
                            let next_point = if vim.mode == Mode::VisualBlock {
                                movement::saturating_right(map, selection.end)
                            } else {
                                movement::right(map, selection.end)
                            };

                            if !(next_point.column() == 0 && next_point == map.max_point()) {
                                selection.end = next_point;
                            }
                        }

                        // vim always ensures the anchor character stays selected.
                        // if our selection has reversed, we need to move the opposite end
                        // to ensure the anchor is still selected.
                        if was_reversed && !selection.reversed {
                            selection.start = movement::left(map, selection.start);
                        } else if !was_reversed && selection.reversed {
                            selection.end = movement::right(map, selection.end);
                        }
                    })
                });
            }
        });
    }

    pub fn visual_block_motion(
        &mut self,
        preserve_goal: bool,
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut Context<Editor>,
        move_selection: &mut dyn FnMut(
            &DisplaySnapshot,
            DisplayPoint,
            SelectionGoal,
        ) -> Option<(DisplayPoint, SelectionGoal)>,
    ) {
        let text_layout_details = editor.text_layout_details(window, cx);
        editor.change_selections(Default::default(), window, cx, |s| {
            let map = &s.display_snapshot();
            let mut head = s.newest_anchor().head().to_display_point(map);
            let mut tail = s.oldest_anchor().tail().to_display_point(map);

            let mut head_x = map.x_for_display_point(head, &text_layout_details);
            let mut tail_x = map.x_for_display_point(tail, &text_layout_details);

            let (start, end) = match s.newest_anchor().goal {
                SelectionGoal::HorizontalRange { start, end } if preserve_goal => (start, end),
                SelectionGoal::HorizontalPosition(start) if preserve_goal => (start, start),
                _ => (tail_x.into(), head_x.into()),
            };
            let mut goal = SelectionGoal::HorizontalRange { start, end };

            let was_reversed = tail_x > head_x;
            if !was_reversed && !preserve_goal {
                head = movement::saturating_left(map, head);
            }

            let reverse_aware_goal = if was_reversed {
                SelectionGoal::HorizontalRange {
                    start: end,
                    end: start,
                }
            } else {
                goal
            };

            let Some((new_head, _)) = move_selection(map, head, reverse_aware_goal) else {
                return;
            };
            head = new_head;
            head_x = map.x_for_display_point(head, &text_layout_details);

            let is_reversed = tail_x > head_x;
            if was_reversed && !is_reversed {
                tail = movement::saturating_left(map, tail);
                tail_x = map.x_for_display_point(tail, &text_layout_details);
            } else if !was_reversed && is_reversed {
                tail = movement::saturating_right(map, tail);
                tail_x = map.x_for_display_point(tail, &text_layout_details);
            }
            if !is_reversed && !preserve_goal {
                head = movement::saturating_right(map, head);
                head_x = map.x_for_display_point(head, &text_layout_details);
            }

            let positions = if is_reversed {
                head_x..tail_x
            } else {
                tail_x..head_x
            };

            if !preserve_goal {
                goal = SelectionGoal::HorizontalRange {
                    start: f64::from(positions.start),
                    end: f64::from(positions.end),
                };
            }

            let mut selections = Vec::new();
            let mut row = tail.row();
            let going_up = tail.row() > head.row();
            let direction = if going_up { -1 } else { 1 };

            loop {
                let laid_out_line = map.layout_row(row, &text_layout_details);
                let start = DisplayPoint::new(
                    row,
                    laid_out_line.closest_index_for_x(positions.start) as u32,
                );
                let mut end =
                    DisplayPoint::new(row, laid_out_line.closest_index_for_x(positions.end) as u32);
                if end <= start {
                    if start.column() == map.line_len(start.row()) {
                        end = start;
                    } else {
                        end = movement::saturating_right(map, start);
                    }
                }

                if positions.start <= laid_out_line.width {
                    let selection = Selection {
                        id: s.new_selection_id(),
                        start: start.to_point(map),
                        end: end.to_point(map),
                        reversed: is_reversed &&
                                    // For neovim parity: cursor is not reversed when column is a single character
                                    end.column() - start.column() > 1,
                        goal,
                    };

                    selections.push(selection);
                }

                // When dealing with soft wrapped lines, it's possible that
                // `row` ends up being set to a value other than `head.row()` as
                // `head.row()` might be a `DisplayPoint` mapped to a soft
                // wrapped line, hence the need for `<=` and `>=` instead of
                // `==`.
                if going_up && row <= head.row() || !going_up && row >= head.row() {
                    break;
                }

                // Find the next or previous buffer row where the `row` should
                // be moved to, so that wrapped lines are skipped.
                row = map
                    .start_of_relative_buffer_row(DisplayPoint::new(row, 0), direction)
                    .row();
            }

            s.select(selections);
        })
    }

    pub fn visual_object(
        &mut self,
        object: Object,
        count: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Vim>,
    ) {
        if let Some(Operator::Object { around }) = self.active_operator() {
            self.pop_operator(window, cx);
            let current_mode = self.mode;
            let target_mode = object.target_visual_mode(current_mode, around);
            if target_mode != current_mode {
                self.switch_mode(target_mode, true, window, cx);
            }

            self.update_editor(cx, |_, editor, cx| {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let mut mut_selection = selection.clone();

                        // all our motions assume that the current character is
                        // after the cursor; however in the case of a visual selection
                        // the current character is before the cursor.
                        // But this will affect the judgment of the html tag
                        // so the html tag needs to skip this logic.
                        if !selection.reversed && object != Object::Tag {
                            mut_selection.set_head(
                                movement::left(map, mut_selection.head()),
                                mut_selection.goal,
                            );
                        }

                        let original_point = selection.tail().to_point(map);

                        if let Some(range) = object.range(map, mut_selection, around, count) {
                            if !range.is_empty() {
                                let expand_both_ways = object.always_expands_both_ways()
                                    || selection.is_empty()
                                    || movement::right(map, selection.start) == selection.end;

                                if expand_both_ways {
                                    if selection.start == range.start
                                        && selection.end == range.end
                                        && object.always_expands_both_ways()
                                    {
                                        if let Some(range) =
                                            object.range(map, selection.clone(), around, count)
                                        {
                                            selection.start = range.start;
                                            selection.end = range.end;
                                        }
                                    } else {
                                        selection.start = range.start;
                                        selection.end = range.end;
                                    }
                                } else if selection.reversed {
                                    selection.start = range.start;
                                } else {
                                    selection.end = range.end;
                                }
                            }

                            // In the visual selection result of a paragraph object, the cursor is
                            // placed at the start of the last line. And in the visual mode, the
                            // selection end is located after the end character. So, adjustment of
                            // selection end is needed.
                            //
                            // We don't do this adjustment for a one-line blank paragraph since the
                            // trailing newline is included in its selection from the beginning.
                            if object == Object::Paragraph && range.start != range.end {
                                let row_of_selection_end_line = selection.end.to_point(map).row;
                                let new_selection_end = if map
                                    .buffer_snapshot()
                                    .line_len(MultiBufferRow(row_of_selection_end_line))
                                    == 0
                                {
                                    Point::new(row_of_selection_end_line + 1, 0)
                                } else {
                                    Point::new(row_of_selection_end_line, 1)
                                };
                                selection.end = new_selection_end.to_display_point(map);
                            }

                            // To match vim, if the range starts of the same line as it originally
                            // did, we keep the tail of the selection in the same place instead of
                            // snapping it to the start of the line
                            if target_mode == Mode::VisualLine {
                                let new_start_point = selection.start.to_point(map);
                                if new_start_point.row == original_point.row {
                                    if selection.end.to_point(map).row > new_start_point.row {
                                        if original_point.column
                                            == map
                                                .buffer_snapshot()
                                                .line_len(MultiBufferRow(original_point.row))
                                        {
                                            selection.start = movement::saturating_left(
                                                map,
                                                original_point.to_display_point(map),
                                            )
                                        } else {
                                            selection.start = original_point.to_display_point(map)
                                        }
                                    } else {
                                        let original_display_point =
                                            original_point.to_display_point(map);
                                        if selection.end <= original_display_point {
                                            selection.end = movement::saturating_right(
                                                map,
                                                original_display_point,
                                            );
                                            if original_point.column > 0 {
                                                selection.reversed = true
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            });
        }
    }
}
