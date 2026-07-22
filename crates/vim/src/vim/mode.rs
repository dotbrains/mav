use super::*;

impl Vim {
    pub(super) fn push_operator(
        &mut self,
        operator: Operator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if operator.starts_dot_recording() {
            self.start_recording(cx);
        }
        // Since these operations can only be entered with pre-operators,
        // we need to clear the previous operators when pushing,
        // so that the current stack is the most correct
        if matches!(
            operator,
            Operator::AddSurrounds { .. }
                | Operator::ChangeSurrounds { .. }
                | Operator::DeleteSurrounds
                | Operator::Exchange
        ) {
            self.operator_stack.clear();
        };
        self.operator_stack.push(operator);
        self.sync_vim_settings(window, cx);
    }

    pub fn switch_mode(
        &mut self,
        mode: Mode,
        leave_selections: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.temp_mode && mode == Mode::Normal {
            self.temp_mode = false;
            self.switch_mode(Mode::Normal, leave_selections, window, cx);
            self.switch_mode(Mode::Insert, false, window, cx);
            return;
        } else if self.temp_mode
            && !matches!(mode, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
        {
            self.temp_mode = false;
        }

        let last_mode = self.mode;
        let prior_mode = self.last_mode;
        let prior_tx = self.current_tx;
        self.last_mode = last_mode;
        self.mode = mode;
        self.operator_stack.clear();
        self.selected_register.take();
        self.cancel_running_command(window, cx);
        if mode == Mode::Normal || mode != last_mode {
            self.current_tx.take();
            self.current_anchor.take();
            self.update_editor(cx, |_, editor, _| {
                editor.clear_selection_drag_state();
            });
        }
        Vim::take_forced_motion(cx);
        if mode != Mode::Insert && mode != Mode::Replace {
            Vim::take_count(cx);
        }

        // Sync editor settings like clip mode
        self.sync_vim_settings(window, cx);

        if VimSettings::get_global(cx).toggle_relative_line_numbers
            && self.mode != self.last_mode
            && (self.mode == Mode::Insert || self.last_mode == Mode::Insert)
        {
            self.update_editor(cx, |vim, editor, cx| {
                let is_relative = vim.mode != Mode::Insert;
                editor.set_relative_line_number(Some(is_relative), cx)
            });
        }
        if HelixModeSetting::get_global(cx).0 {
            if self.mode == Mode::Normal {
                self.mode = Mode::HelixNormal
            } else if self.mode == Mode::Visual {
                self.mode = Mode::HelixSelect
            }
        }

        if leave_selections {
            return;
        }

        if !mode.is_visual() && last_mode.is_visual() && !last_mode.is_helix() {
            self.create_visual_marks(last_mode, window, cx);
        }

        // Adjust selections
        self.update_editor(cx, |vim, editor, cx| {
            if last_mode != Mode::VisualBlock && last_mode.is_visual() && mode == Mode::VisualBlock
            {
                vim.visual_block_motion(true, editor, window, cx, &mut |_, point, goal| {
                    Some((point, goal))
                })
            }
            if (last_mode == Mode::Insert || last_mode == Mode::Replace)
                && let Some(prior_tx) = prior_tx
            {
                editor.group_until_transaction(prior_tx, cx)
            }

            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                // we cheat with visual block mode and use multiple cursors.
                // the cost of this cheat is we need to convert back to a single
                // cursor whenever vim would.
                if last_mode == Mode::VisualBlock
                    && (mode != Mode::VisualBlock && mode != Mode::Insert)
                {
                    let tail = s.oldest_anchor().tail();
                    let head = s.newest_anchor().head();
                    s.select_anchor_ranges(vec![tail..head]);
                } else if last_mode == Mode::Insert
                    && prior_mode == Mode::VisualBlock
                    && mode != Mode::VisualBlock
                {
                    let pos = s.first_anchor().head();
                    s.select_anchor_ranges(vec![pos..pos])
                }

                let mut should_extend_pending = false;
                if !last_mode.is_visual()
                    && mode.is_visual()
                    && let Some(pending) = s.pending_anchor()
                {
                    let snapshot = s.display_snapshot();
                    let is_empty = pending
                        .start
                        .cmp(&pending.end, &snapshot.buffer_snapshot())
                        .is_eq();
                    should_extend_pending = pending.reversed
                        && !is_empty
                        && vim.extended_pending_selection_id != Some(pending.id);
                };

                if should_extend_pending {
                    let snapshot = s.display_snapshot();
                    s.change_with(&snapshot, |map| {
                        if let Some(pending) = map.pending_anchor_mut() {
                            let end = pending.end.to_point(&snapshot.buffer_snapshot());
                            let end = end.to_display_point(&snapshot);
                            let new_end = movement::right(&snapshot, end);
                            pending.end = snapshot
                                .buffer_snapshot()
                                .anchor_before(new_end.to_point(&snapshot));
                        }
                    });
                    vim.extended_pending_selection_id = s.pending_anchor().map(|p| p.id)
                }

                s.move_with(&mut |map, selection| {
                    if last_mode.is_visual() && !last_mode.is_helix() && !mode.is_visual() {
                        let mut point = selection.head();
                        if !selection.reversed && !selection.is_empty() {
                            point = movement::left(map, selection.head());
                        } else if selection.is_empty() {
                            point = map.clip_point(point, Bias::Left);
                        }
                        selection.collapse_to(point, selection.goal)
                    } else if !last_mode.is_visual() && mode.is_visual() {
                        if selection.is_empty() {
                            selection.end = movement::right(map, selection.start);
                        }
                    }
                });
            })
        });
    }

    pub fn take_count(cx: &mut App) -> Option<usize> {
        let global_state = cx.global_mut::<VimGlobals>();
        if global_state.dot_replaying {
            return global_state.recorded_count;
        }

        let count = if global_state.post_count.is_none() && global_state.pre_count.is_none() {
            return None;
        } else {
            Some(
                global_state.post_count.take().unwrap_or(1)
                    * global_state.pre_count.take().unwrap_or(1),
            )
        };

        if global_state.dot_recording {
            global_state.recording_count = count;
        }
        count
    }

    pub fn take_forced_motion(cx: &mut App) -> bool {
        let global_state = cx.global_mut::<VimGlobals>();
        let forced_motion = global_state.forced_motion;
        global_state.forced_motion = false;
        forced_motion
    }

    pub fn cursor_shape(&self, cx: &App) -> CursorShape {
        let cursor_shape = VimSettings::get_global(cx).cursor_shape;
        match self.mode {
            Mode::Normal => {
                if let Some(operator) = self.operator_stack.last() {
                    match operator {
                        // Vim jump labels are transient navigation, so keep the
                        // user's normal cursor shape while waiting for the label.
                        Operator::HelixJump { .. } => cursor_shape.normal,

                        // Navigation operators -> Block cursor
                        Operator::FindForward { .. }
                        | Operator::FindBackward { .. }
                        | Operator::Mark
                        | Operator::Jump { .. }
                        | Operator::Register
                        | Operator::RecordRegister
                        | Operator::ReplayRegister => CursorShape::Block,

                        // All other operators -> Underline cursor
                        _ => CursorShape::Underline,
                    }
                } else {
                    cursor_shape.normal
                }
            }
            Mode::HelixNormal => cursor_shape.normal,
            Mode::Replace => cursor_shape.replace,
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock | Mode::HelixSelect => {
                cursor_shape.visual
            }
            Mode::Insert => match cursor_shape.insert {
                InsertModeCursorShape::Explicit(shape) => shape,
                InsertModeCursorShape::Inherit => {
                    let editor_settings = EditorSettings::get_global(cx);
                    editor_settings.cursor_shape.unwrap_or_default()
                }
            },
        }
    }

    pub(super) fn expects_character_input(&self) -> bool {
        if let Some(operator) = self.operator_stack.last() {
            if operator.is_waiting(self.mode) {
                return true;
            }
        }
        self.editor_input_enabled()
    }

    pub fn editor_input_enabled(&self) -> bool {
        match self.mode {
            Mode::Insert => {
                if let Some(operator) = self.operator_stack.last() {
                    !operator.is_waiting(self.mode)
                } else {
                    true
                }
            }
            Mode::Normal
            | Mode::HelixNormal
            | Mode::Replace
            | Mode::Visual
            | Mode::VisualLine
            | Mode::VisualBlock
            | Mode::HelixSelect => false,
        }
    }

    pub fn should_autoindent(&self) -> bool {
        !(self.mode == Mode::Insert && self.last_mode == Mode::VisualBlock)
    }

    pub fn clip_at_line_ends(&self) -> bool {
        match self.mode {
            Mode::Insert
            | Mode::Visual
            | Mode::VisualLine
            | Mode::VisualBlock
            | Mode::Replace
            | Mode::HelixNormal
            | Mode::HelixSelect => false,
            Mode::Normal => true,
        }
    }

    pub fn extend_key_context(&self, context: &mut KeyContext, cx: &App) {
        let mut mode = match self.mode {
            Mode::Normal => "normal",
            Mode::Visual | Mode::VisualLine | Mode::VisualBlock => "visual",
            Mode::Insert => "insert",
            Mode::Replace => "replace",
            Mode::HelixNormal => "helix_normal",
            Mode::HelixSelect => "helix_select",
        }
        .to_string();

        let mut operator_id = "none";

        let active_operator = self.active_operator();
        if active_operator.is_none() && cx.global::<VimGlobals>().pre_count.is_some()
            || active_operator.is_some() && cx.global::<VimGlobals>().post_count.is_some()
        {
            context.add("VimCount");
        }

        if let Some(active_operator) = active_operator {
            if active_operator.is_waiting(self.mode) {
                if matches!(active_operator, Operator::Literal { .. }) {
                    mode = "literal".to_string();
                } else {
                    mode = "waiting".to_string();
                }
            } else if matches!(
                active_operator,
                Operator::HelixNext { .. } | Operator::HelixPrevious { .. }
            ) {
                // Helix `[`/`]` take a curated, keymap-dispatched selector key
                // rather than a motion over a range, so they keep `operator_id`
                // set (so `vim_operator == helix_next/previous` context must
                // resolve) but must not use the `operator` mode, as that adds
                // `VimControl` and the `vim_mode == operator` context, whose `g
                // ...` bindings would make a single-key follow-up like `g` a
                // multi-key prefix and leave `] g` waiting for more input.
                // Setting the mode to `waiting` carries none of those
                // conflicting bindings and still provides bindings for
                // `escape`/`ctrl-c` to `ClearOperators`.
                operator_id = active_operator.id();
                mode = "waiting".to_string();
            } else {
                operator_id = active_operator.id();
                mode = "operator".to_string();
            }
        }

        if mode == "normal"
            || mode == "visual"
            || mode == "operator"
            || mode == "helix_normal"
            || mode == "helix_select"
        {
            context.add("VimControl");
        }
        context.set("vim_mode", mode);
        context.set("vim_operator", operator_id);
    }
}
