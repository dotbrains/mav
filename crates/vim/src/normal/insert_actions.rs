use super::*;

impl Vim {
    pub(crate) fn insert_after(
        &mut self,
        _: &InsertAfter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_cursors_with(&mut |map, cursor, _| {
                    (right(map, cursor, 1), SelectionGoal::None)
                });
            });
        });
    }

    pub(crate) fn insert_before(
        &mut self,
        _: &InsertBefore,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        if self.mode.is_visual() {
            let current_mode = self.mode;
            self.update_editor(cx, |_, editor, cx| {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        if current_mode == Mode::VisualLine {
                            let start_of_line = motion::start_of_line(map, false, selection.start);
                            selection.collapse_to(start_of_line, SelectionGoal::None)
                        } else {
                            selection.collapse_to(selection.start, SelectionGoal::None)
                        }
                    });
                });
            });
        }
        self.switch_mode(Mode::Insert, false, window, cx);
    }

    pub(crate) fn insert_first_non_whitespace(
        &mut self,
        _: &InsertFirstNonWhitespace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_cursors_with(&mut |map, cursor, _| {
                    (
                        first_non_whitespace(map, false, cursor),
                        SelectionGoal::None,
                    )
                });
            });
        });
    }

    pub(crate) fn insert_end_of_line(
        &mut self,
        _: &InsertEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.move_cursors_with(&mut |map, cursor, _| {
                    (next_line_end(map, cursor, 1), SelectionGoal::None)
                });
            });
        });
    }

    pub(crate) fn insert_at_previous(
        &mut self,
        _: &InsertAtPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |vim, editor, cx| {
            if let Some(Mark::Local(marks)) = vim.get_mark("^", editor, window, cx)
                && !marks.is_empty()
            {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.select_anchor_ranges(marks.iter().map(|mark| *mark..*mark))
                });
            }
        });
    }

    pub(crate) fn insert_line_above(
        &mut self,
        _: &InsertLineAbove,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let selections = editor.selections.all::<Point>(&editor.display_snapshot(cx));
                let snapshot = editor.buffer().read(cx).snapshot(cx);

                let selection_start_rows: BTreeSet<u32> = selections
                    .into_iter()
                    .map(|selection| selection.start.row)
                    .collect();

                let mut auto_indent_edits = Vec::new();
                let mut plain_edits = Vec::new();

                for row in selection_start_rows {
                    let auto_indent_mode = snapshot
                        .language_settings_at(Point::new(row, 0), cx)
                        .auto_indent;
                    let indent = if auto_indent_mode == AutoIndentMode::None {
                        String::new()
                    } else {
                        let indent_size = snapshot.indent_size_for_line(MultiBufferRow(row)).len;
                        let first_char = snapshot.chars_at(Point::new(row, indent_size)).next();
                        let indent_row = if matches!(first_char, Some('}') | Some(')')) {
                            snapshot
                                .prev_non_blank_row(MultiBufferRow(row))
                                .map(|r| r.0)
                                .unwrap_or(row)
                        } else {
                            row
                        };
                        snapshot.indent_and_comment_for_line(MultiBufferRow(indent_row), cx)
                    };
                    let start_of_line = Point::new(row, 0);
                    let edit = (start_of_line..start_of_line, indent + "\n");
                    if auto_indent_mode == AutoIndentMode::None {
                        plain_edits.push(edit);
                    } else {
                        auto_indent_edits.push(edit);
                    }
                }

                if !plain_edits.is_empty() {
                    editor.edit(plain_edits, cx);
                }
                if !auto_indent_edits.is_empty() {
                    editor.edit_with_autoindent(auto_indent_edits, cx);
                }

                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let previous_line = map.start_of_relative_buffer_row(selection.start, -1);
                        let insert_point = motion::end_of_line(map, false, previous_line, 1);
                        selection.collapse_to(insert_point, SelectionGoal::None)
                    });
                });
            });
        });
    }

    pub(crate) fn insert_line_below(
        &mut self,
        _: &InsertLineBelow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_recording(cx);
        self.switch_mode(Mode::Insert, false, window, cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let selections = editor.selections.all::<Point>(&editor.display_snapshot(cx));
                let snapshot = editor.buffer().read(cx).snapshot(cx);

                let selection_end_rows: BTreeSet<u32> = selections
                    .into_iter()
                    .map(|selection| {
                        if !selection.is_empty() && selection.end.column == 0 {
                            selection.end.row.saturating_sub(1)
                        } else {
                            selection.end.row
                        }
                    })
                    .collect();

                let mut auto_indent_edits = Vec::new();
                let mut plain_edits = Vec::new();

                for row in selection_end_rows {
                    let auto_indent_mode = snapshot
                        .language_settings_at(Point::new(row, 0), cx)
                        .auto_indent;
                    let indent = if auto_indent_mode == AutoIndentMode::None {
                        String::new()
                    } else {
                        snapshot.indent_and_comment_for_line(MultiBufferRow(row), cx)
                    };
                    let end_of_line = Point::new(row, snapshot.line_len(MultiBufferRow(row)));
                    let edit = (end_of_line..end_of_line, "\n".to_string() + &indent);
                    if auto_indent_mode == AutoIndentMode::None {
                        plain_edits.push(edit);
                    } else {
                        auto_indent_edits.push(edit);
                    }
                }

                editor.change_selections(Default::default(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let current_line = if !selection.is_empty() && selection.end.column() == 0 {
                            // If this is an insert after a selection to the end of the line, the
                            // cursor needs to be bumped back, because it'll be at the start of the
                            // *next* line.
                            map.start_of_relative_buffer_row(selection.end, -1)
                        } else {
                            selection.end
                        };
                        let insert_point = motion::end_of_line(map, false, current_line, 1);
                        selection.collapse_to(insert_point, SelectionGoal::None)
                    });
                });

                if !plain_edits.is_empty() {
                    editor.edit(plain_edits, cx);
                }
                if !auto_indent_edits.is_empty() {
                    editor.edit_with_autoindent(auto_indent_edits, cx);
                }
            });
        });
    }

    pub(crate) fn insert_empty_line_above(
        &mut self,
        _: &InsertEmptyLineAbove,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_current_action(cx);
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, _, cx| {
                let selections = editor.selections.all::<Point>(&editor.display_snapshot(cx));

                let selection_start_rows: BTreeSet<u32> = selections
                    .into_iter()
                    .map(|selection| selection.start.row)
                    .collect();
                let edits = selection_start_rows
                    .into_iter()
                    .map(|row| {
                        let start_of_line = Point::new(row, 0);
                        (start_of_line..start_of_line, "\n".repeat(count))
                    })
                    .collect::<Vec<_>>();
                editor.edit(edits, cx);
            });
        });
    }

    pub(crate) fn insert_empty_line_below(
        &mut self,
        _: &InsertEmptyLineBelow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_current_action(cx);
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let display_map = editor.display_snapshot(cx);
                let selections = editor.selections.all::<Point>(&display_map);
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                let display_selections = editor.selections.all_display(&display_map);
                let original_positions = display_selections
                    .iter()
                    .map(|s| (s.id, s.head()))
                    .collect::<HashMap<_, _>>();

                let selection_end_rows: BTreeSet<u32> = selections
                    .into_iter()
                    .map(|selection| selection.end.row)
                    .collect();
                let edits = selection_end_rows
                    .into_iter()
                    .map(|row| {
                        let end_of_line = Point::new(row, snapshot.line_len(MultiBufferRow(row)));
                        (end_of_line..end_of_line, "\n".repeat(count))
                    })
                    .collect::<Vec<_>>();
                editor.edit(edits, cx);

                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.move_with(&mut |_, selection| {
                        if let Some(position) = original_positions.get(&selection.id) {
                            selection.collapse_to(*position, SelectionGoal::None);
                        }
                    });
                });
            });
        });
    }
}
