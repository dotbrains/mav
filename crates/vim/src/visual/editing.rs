use super::*;

impl Vim {
    fn visual_insert_end_of_line(
        &mut self,
        _: &VisualInsertEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |_, editor, cx| {
            editor.split_selection_into_lines(&Default::default(), window, cx);
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_cursors_with(&mut |map, cursor, _| {
                    (next_line_end(map, cursor, 1), SelectionGoal::None)
                });
            });
        });

        self.switch_mode(Mode::Insert, false, window, cx);
    }

    fn visual_insert_first_non_white_space(
        &mut self,
        _: &VisualInsertFirstNonWhiteSpace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |_, editor, cx| {
            editor.split_selection_into_lines(&Default::default(), window, cx);
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_cursors_with(&mut |map, cursor, _| {
                    (
                        first_non_whitespace(map, false, cursor),
                        SelectionGoal::None,
                    )
                });
            });
        });

        self.switch_mode(Mode::Insert, false, window, cx);
    }

    fn toggle_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            self.switch_mode(Mode::Normal, false, window, cx);
        } else {
            self.switch_mode(mode, false, window, cx);
        }
    }

    pub fn other_end(&mut self, _: &OtherEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_, selection| {
                    selection.reversed = !selection.reversed;
                });
            })
        });
    }

    pub fn other_end_row_aware(
        &mut self,
        _: &OtherEndRowAware,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.mode;
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_with(&mut |_, selection| {
                    selection.reversed = !selection.reversed;
                });
                if mode == Mode::VisualBlock {
                    s.reverse_selections();
                }
            })
        });
    }

    pub fn visual_delete(
        &mut self,
        line_mode: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        self.store_visual_marks(window, cx);
        let transaction_id = self.update_editor(cx, |vim, editor, cx| {
            let mut original_columns: HashMap<_, _> = Default::default();
            let line_mode = line_mode || editor.selections.line_mode();
            editor.selections.set_line_mode(false);

            editor.transact(window, cx, |editor, window, cx| {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        if line_mode {
                            let mut position = selection.head();
                            if !selection.reversed {
                                position = movement::left(map, position);
                            }
                            original_columns.insert(selection.id, position.to_point(map).column);
                            if vim.mode == Mode::VisualBlock {
                                *selection.end.column_mut() = map.line_len(selection.end.row())
                            } else {
                                let start = selection.start.to_point(map);
                                let end = selection.end.to_point(map);
                                selection.start = map.prev_line_boundary(start).1;
                                if end.column == 0 && end > start {
                                    let row = end.row.saturating_sub(1);
                                    selection.end = Point::new(
                                        row,
                                        map.buffer_snapshot().line_len(MultiBufferRow(row)),
                                    )
                                    .to_display_point(map)
                                } else {
                                    selection.end = map.next_line_boundary(end).1;
                                }
                            }
                        }
                        selection.goal = SelectionGoal::None;
                    });
                });
                let kind = if line_mode {
                    MotionKind::Linewise
                } else {
                    MotionKind::Exclusive
                };
                vim.copy_selections_content(editor, kind, window, cx);

                if line_mode && vim.mode != Mode::VisualBlock {
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            let end = selection.end.to_point(map);
                            let start = selection.start.to_point(map);
                            if end.row < map.buffer_snapshot().max_point().row {
                                selection.end = Point::new(end.row + 1, 0).to_display_point(map)
                            } else if start.row > 0 {
                                selection.start = Point::new(
                                    start.row - 1,
                                    map.buffer_snapshot()
                                        .line_len(MultiBufferRow(start.row - 1)),
                                )
                                .to_display_point(map)
                            }
                        });
                    });
                }
                editor.delete_selections_with_linked_edits(window, cx);

                // Fixup cursor position after the deletion
                editor.set_clip_at_line_ends(true, cx);
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let mut cursor = selection.head().to_point(map);

                        if let Some(column) = original_columns.get(&selection.id) {
                            cursor.column = *column
                        }
                        let cursor = map.clip_point(cursor.to_display_point(map), Bias::Left);
                        selection.collapse_to(cursor, selection.goal)
                    });
                    if vim.mode == Mode::VisualBlock {
                        s.select_anchors(vec![s.first_anchor()])
                    }
                });
            })
        });
        let transaction_id = transaction_id.flatten();
        self.switch_mode(Mode::Normal, true, window, cx);
        transaction_id
    }

    pub fn visual_yank(&mut self, line_mode: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.store_visual_marks(window, cx);
        self.update_editor(cx, |vim, editor, cx| {
            let line_mode = line_mode || editor.selections.line_mode();

            // For visual line mode, adjust selections to avoid yanking the next line when on \n
            if line_mode && vim.mode != Mode::VisualBlock {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let start = selection.start.to_point(map);
                        let end = selection.end.to_point(map);
                        if end.column == 0 && end > start {
                            let row = end.row.saturating_sub(1);
                            selection.end = Point::new(
                                row,
                                map.buffer_snapshot().line_len(MultiBufferRow(row)),
                            )
                            .to_display_point(map);
                        }
                    });
                });
            }

            editor.selections.set_line_mode(line_mode);
            let kind = if line_mode {
                MotionKind::Linewise
            } else {
                MotionKind::Exclusive
            };
            vim.yank_selections_content(editor, kind, window, cx);
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if line_mode {
                        selection.start = start_of_line(map, false, selection.start);
                    };
                    selection.collapse_to(selection.start, SelectionGoal::None)
                });
                if vim.mode == Mode::VisualBlock {
                    s.select_anchors(vec![s.first_anchor()])
                }
            });
        });
        self.switch_mode(Mode::Normal, true, window, cx);
    }

    pub(crate) fn visual_replace(
        &mut self,
        text: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_recording(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let display_map = editor.display_snapshot(cx);
                let selections = editor.selections.all_adjusted_display(&display_map);

                // Selections are biased right at the start. So we need to store
                // anchors that are biased left so that we can restore the selections
                // after the change
                let stable_anchors = editor
                    .selections
                    .disjoint_anchors_arc()
                    .iter()
                    .map(|selection| {
                        let start = selection.start.bias_left(&display_map.buffer_snapshot());
                        start..start
                    })
                    .collect::<Vec<_>>();

                let mut edits = Vec::new();
                for selection in selections.iter() {
                    let selection = selection.clone();
                    for row_range in
                        movement::split_display_range_by_lines(&display_map, selection.range())
                    {
                        let range = row_range.start.to_offset(&display_map, Bias::Right)
                            ..row_range.end.to_offset(&display_map, Bias::Right);
                        let grapheme_count = display_map
                            .buffer_snapshot()
                            .grapheme_count_for_range(&range);
                        let text = text.repeat(grapheme_count);
                        edits.push((range, text));
                    }
                }

                editor.edit(edits, cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(stable_anchors)
                });
            });
        });
        self.switch_mode(Mode::Normal, false, window, cx);
    }
}
