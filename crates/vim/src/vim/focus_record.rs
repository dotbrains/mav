use super::*;

impl Vim {
    pub(super) fn focused(
        &mut self,
        preserve_selection: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // If editor gains focus while search bar is still open (not dismissed),
        // the user has explicitly navigated away - clear prior_selections so we
        // don't restore to the old position if they later dismiss the search.
        if !self.search.prior_selections.is_empty() {
            if let Some(pane) = self.pane(window, cx) {
                let search_still_open = pane
                    .read(cx)
                    .toolbar()
                    .read(cx)
                    .item_of_type::<BufferSearchBar>()
                    .is_some_and(|bar| !bar.read(cx).is_dismissed());
                if search_still_open {
                    self.search.prior_selections.clear();
                }
            }
        }

        let Some(editor) = self.editor() else {
            return;
        };
        let newest_selection_empty = editor.update(cx, |editor, cx| {
            editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx))
                .is_empty()
        });
        let editor = editor.read(cx);
        let editor_mode = editor.mode();

        if editor_mode.is_full()
            && !newest_selection_empty
            && self.mode == Mode::Normal
            // When following someone, don't switch vim mode.
            && editor.leader_id().is_none()
        {
            if preserve_selection {
                self.switch_mode(Mode::Visual, true, window, cx);
            } else {
                self.update_editor(cx, |_, editor, cx| {
                    editor.set_clip_at_line_ends(false, cx);
                    editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                        s.move_with(&mut |_, selection| {
                            selection.collapse_to(selection.start, selection.goal)
                        })
                    });
                });
            }
        }

        cx.emit(VimEvent::Focused);
        self.sync_vim_settings(window, cx);

        if VimSettings::get_global(cx).toggle_relative_line_numbers {
            if let Some(old_vim) = Vim::globals(cx).focused_vim() {
                if old_vim.entity_id() != cx.entity().entity_id() {
                    old_vim.update(cx, |vim, cx| {
                        vim.update_editor(cx, |_, editor, cx| {
                            editor.set_relative_line_number(None, cx)
                        });
                    });

                    self.update_editor(cx, |vim, editor, cx| {
                        let is_relative = vim.mode != Mode::Insert;
                        editor.set_relative_line_number(Some(is_relative), cx)
                    });
                }
            } else {
                self.update_editor(cx, |vim, editor, cx| {
                    let is_relative = vim.mode != Mode::Insert;
                    editor.set_relative_line_number(Some(is_relative), cx)
                });
            }
        }
        Vim::globals(cx).focused_vim = Some(cx.entity().downgrade());
    }

    pub(super) fn blurred(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_recording_immediately(NormalBefore.boxed_clone(), cx);
        self.store_visual_marks(window, cx);
        self.clear_operator(window, cx);
        self.update_editor(cx, |vim, editor, cx| {
            if vim.cursor_shape(cx) == CursorShape::Block {
                editor.set_cursor_shape(CursorShape::Hollow, cx);
            }
        });
    }

    pub(super) fn cursor_shape_changed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.update_editor(cx, |vim, editor, cx| {
            editor.set_cursor_shape(vim.cursor_shape(cx), cx);
        });
    }

    pub(super) fn update_editor<S>(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, &mut Editor, &mut Context<Editor>) -> S,
    ) -> Option<S> {
        let editor = self.editor.upgrade()?;
        Some(editor.update(cx, |editor, cx| update(self, editor, cx)))
    }

    pub(super) fn editor_selections(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Range<Anchor>> {
        self.update_editor(cx, |_, editor, _| {
            editor
                .selections
                .disjoint_anchors_arc()
                .iter()
                .map(|selection| selection.tail()..selection.head())
                .collect()
        })
        .unwrap_or_default()
    }

    /// When doing an action that modifies the buffer, we start recording so that `.`
    /// will replay the action.
    pub fn start_recording(&mut self, cx: &mut Context<Self>) {
        Vim::update_globals(cx, |globals, cx| {
            if !globals.dot_replaying {
                globals.dot_recording = true;
                globals.recording_actions = Default::default();
                globals.recording_count = None;
                globals.recording_register_for_dot = self.selected_register;

                let selections = self.editor().map(|editor| {
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.display_snapshot(cx);

                        (
                            editor.selections.oldest::<Point>(&snapshot),
                            editor.selections.newest::<Point>(&snapshot),
                        )
                    })
                });

                if let Some((oldest, newest)) = selections {
                    globals.recorded_selection = match self.mode {
                        Mode::Visual if newest.end.row == newest.start.row => {
                            RecordedSelection::SingleLine {
                                cols: newest.end.column - newest.start.column,
                            }
                        }
                        Mode::Visual => RecordedSelection::Visual {
                            rows: newest.end.row - newest.start.row,
                            cols: newest.end.column,
                        },
                        Mode::VisualLine => RecordedSelection::VisualLine {
                            rows: newest.end.row - newest.start.row,
                        },
                        Mode::VisualBlock => RecordedSelection::VisualBlock {
                            rows: newest.end.row.abs_diff(oldest.start.row),
                            cols: newest.end.column.abs_diff(oldest.start.column),
                        },
                        _ => RecordedSelection::None,
                    }
                } else {
                    globals.recorded_selection = RecordedSelection::None;
                }
            }
        })
    }

    pub fn stop_replaying(&mut self, cx: &mut Context<Self>) {
        let globals = Vim::globals(cx);
        globals.dot_replaying = false;
        if let Some(replayer) = globals.replayer.take() {
            replayer.stop();
        }
    }

    /// When finishing an action that modifies the buffer, stop recording.
    /// as you usually call this within a keystroke handler we also ensure that
    /// the current action is recorded.
    pub fn stop_recording(&mut self, cx: &mut Context<Self>) {
        let globals = Vim::globals(cx);
        if globals.dot_recording {
            globals.stop_recording_after_next_action = true;
        }
        self.exit_temporary_mode = self.temp_mode;
    }

    /// Stops recording actions immediately rather than waiting until after the
    /// next action to stop recording.
    ///
    /// This doesn't include the current action.
    pub fn stop_recording_immediately(&mut self, action: Box<dyn Action>, cx: &mut Context<Self>) {
        let globals = Vim::globals(cx);
        if globals.dot_recording {
            globals
                .recording_actions
                .push(ReplayableAction::Action(action.boxed_clone()));
            globals.recorded_actions = mem::take(&mut globals.recording_actions);
            globals.recorded_count = globals.recording_count.take();
            globals.dot_recording = false;
            globals.stop_recording_after_next_action = false;
        }
        self.exit_temporary_mode = self.temp_mode;
    }

    /// Explicitly record one action (equivalents to start_recording and stop_recording)
    pub fn record_current_action(&mut self, cx: &mut Context<Self>) {
        self.start_recording(cx);
        self.stop_recording(cx);
    }

    pub(super) fn push_count_digit(
        &mut self,
        number: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_operator().is_some() {
            let post_count = Vim::globals(cx).post_count.unwrap_or(0);

            Vim::globals(cx).post_count = Some(
                post_count
                    .checked_mul(10)
                    .and_then(|post_count| post_count.checked_add(number))
                    .filter(|post_count| *post_count < isize::MAX as usize)
                    .unwrap_or(post_count),
            )
        } else {
            let pre_count = Vim::globals(cx).pre_count.unwrap_or(0);

            Vim::globals(cx).pre_count = Some(
                pre_count
                    .checked_mul(10)
                    .and_then(|pre_count| pre_count.checked_add(number))
                    .filter(|pre_count| *pre_count < isize::MAX as usize)
                    .unwrap_or(pre_count),
            )
        }
        // update the keymap so that 0 works
        self.sync_vim_settings(window, cx)
    }

    pub(super) fn select_register(
        &mut self,
        register: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if register.chars().count() == 1 {
            self.selected_register
                .replace(register.chars().next().unwrap());
        }
        self.operator_stack.clear();
        self.sync_vim_settings(window, cx);
    }

    pub(super) fn maybe_pop_operator(&mut self) -> Option<Operator> {
        self.operator_stack.pop()
    }

    pub(super) fn pop_operator(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Operator {
        let popped_operator = self.operator_stack.pop()
            .expect("Operator popped when no operator was on the stack. This likely means there is an invalid keymap config");
        self.sync_vim_settings(window, cx);
        popped_operator
    }

    pub(super) fn clear_operator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.active_operator(), Some(Operator::HelixJump { .. })) {
            self.clear_helix_jump_ui(window, cx);
        }
        Vim::take_count(cx);
        Vim::take_forced_motion(cx);
        self.selected_register.take();
        self.operator_stack.clear();
        self.sync_vim_settings(window, cx);
    }
}
