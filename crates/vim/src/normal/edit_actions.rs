use super::*;

impl Vim {
    pub(crate) fn join_lines_impl(
        &mut self,
        insert_whitespace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_current_action(cx);
        let mut times = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        if self.mode.is_visual() {
            times = 1;
        } else if times > 1 {
            // 2J joins two lines together (same as J or 1J)
            times -= 1;
        }

        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                for _ in 0..times {
                    editor.join_lines_impl(insert_whitespace, window, cx)
                }
            })
        });
        if self.mode.is_visual() {
            self.switch_mode(Mode::Normal, true, window, cx)
        }
    }

    pub(crate) fn yank_line(&mut self, _: &YankLine, window: &mut Window, cx: &mut Context<Self>) {
        let count = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        self.yank_motion(
            motion::Motion::CurrentLine,
            count,
            forced_motion,
            window,
            cx,
        )
    }

    pub(crate) fn yank_to_end_of_line(
        &mut self,
        _: &YankToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        self.yank_motion(
            motion::Motion::EndOfLine {
                display_lines: false,
            },
            count,
            forced_motion,
            window,
            cx,
        )
    }

    pub(crate) fn show_location(
        &mut self,
        _: &ShowLocation,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = Vim::take_count(cx);
        Vim::take_forced_motion(cx);
        self.update_editor(cx, |vim, editor, cx| {
            let selection = editor.selections.newest_anchor();
            let Some((buffer, point)) = editor
                .buffer()
                .read(cx)
                .point_to_buffer_point(selection.head(), cx)
            else {
                return;
            };
            let filename = if let Some(file) = buffer.read(cx).file() {
                if count.is_some() {
                    if let Some(local) = file.as_local() {
                        local.abs_path(cx).to_string_lossy().into_owned()
                    } else {
                        file.full_path(cx).to_string_lossy().into_owned()
                    }
                } else {
                    file.path().display(file.path_style(cx)).into_owned()
                }
            } else {
                "[No Name]".into()
            };
            let buffer = buffer.read(cx);
            let lines = buffer.max_point().row + 1;
            let current_line = point.row;
            let percentage = current_line as f32 / lines as f32;
            let modified = if buffer.is_dirty() { " [modified]" } else { "" };
            vim.set_status_label(
                format!(
                    "{}{} {} lines --{:.0}%--",
                    filename,
                    modified,
                    lines,
                    percentage * 100.0,
                ),
                cx,
            );
        });
    }

    pub(crate) fn toggle_comments(
        &mut self,
        _: &ToggleComments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_current_action(cx);
        self.store_visual_marks(window, cx);
        self.update_editor(cx, |vim, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let original_positions = vim.save_selection_starts(editor, cx);
                editor.toggle_comments(&Default::default(), window, cx);
                vim.restore_selection_cursors(editor, window, cx, original_positions);
            });
        });
        if self.mode.is_visual() {
            self.switch_mode(Mode::Normal, true, window, cx)
        }
    }

    pub(crate) fn toggle_block_comments(
        &mut self,
        _: &ToggleBlockComments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_current_action(cx);
        self.store_visual_marks(window, cx);
        let is_visual_line = self.mode == Mode::VisualLine;
        self.update_editor(cx, |vim, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                let original_positions = vim.save_selection_starts(editor, cx);
                if is_visual_line {
                    editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                        s.move_with(&mut |map, selection| {
                            let start_row = selection.start.to_point(map).row;
                            let end_row = selection.end.to_point(map).row;
                            let end_col = map.buffer_snapshot().line_len(MultiBufferRow(end_row));
                            selection.start = Point::new(start_row, 0).to_display_point(map);
                            selection.end = Point::new(end_row, end_col).to_display_point(map);
                        });
                    });
                }
                editor.toggle_block_comments(&Default::default(), window, cx);
                vim.restore_selection_cursors(editor, window, cx, original_positions);
            });
        });
        if self.mode.is_visual() {
            self.switch_mode(Mode::Normal, true, window, cx)
        }
    }

    pub(crate) fn normal_replace(
        &mut self,
        text: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // We need to use `text.chars().count()` instead of `text.len()` here as
        // `len()` counts bytes, not characters.
        let char_count = text.chars().count();
        let count = Vim::take_count(cx).unwrap_or(char_count);
        let is_return_char = text == "\n".into() || text == "\r".into();
        let repeat_count = match (is_return_char, char_count) {
            (true, _) => 0,
            (_, 1) => count,
            (_, _) => 1,
        };

        Vim::take_forced_motion(cx);
        self.stop_recording(cx);
        self.update_editor(cx, |_, editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.set_clip_at_line_ends(false, cx);
                let display_map = editor.display_snapshot(cx);
                let display_selections = editor.selections.all_display(&display_map);

                let mut edits = Vec::with_capacity(display_selections.len());
                for selection in &display_selections {
                    let mut range = selection.range();
                    for _ in 0..count {
                        let new_point = movement::saturating_right(&display_map, range.end);
                        if range.end == new_point {
                            return;
                        }
                        range.end = new_point;
                    }

                    edits.push((
                        range.start.to_offset(&display_map, Bias::Left)
                            ..range.end.to_offset(&display_map, Bias::Left),
                        text.repeat(repeat_count),
                    ));
                }

                editor.edit(edits, cx);
                if is_return_char {
                    editor.newline(&editor::actions::Newline, window, cx);
                }
                editor.set_clip_at_line_ends(true, cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.move_with(&mut |map, selection| {
                        let point = movement::saturating_left(map, selection.head());
                        selection.collapse_to(point, SelectionGoal::None)
                    });
                });
            });
        });
        self.pop_operator(window, cx);
    }

    pub fn save_selection_starts(
        &self,
        editor: &Editor,
        cx: &mut Context<Editor>,
    ) -> HashMap<usize, Anchor> {
        let display_map = editor.display_snapshot(cx);
        let selections = editor.selections.all_display(&display_map);
        selections
            .iter()
            .map(|selection| {
                (
                    selection.id,
                    display_map.display_point_to_anchor(selection.start, Bias::Right),
                )
            })
            .collect::<HashMap<_, _>>()
    }

    pub fn restore_selection_cursors(
        &self,
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut Context<Editor>,
        mut positions: HashMap<usize, Anchor>,
    ) {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.move_with(&mut |map, selection| {
                if let Some(anchor) = positions.remove(&selection.id) {
                    selection.collapse_to(anchor.to_display_point(map), SelectionGoal::None);
                }
            });
        });
    }

    pub(crate) fn exit_temporary_normal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.temp_mode {
            self.switch_mode(Mode::Insert, true, window, cx);
        }
    }
}
