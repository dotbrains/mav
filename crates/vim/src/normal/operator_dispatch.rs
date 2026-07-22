use super::*;

impl Vim {
    pub fn normal_motion(
        &mut self,
        motion: Motion,
        operator: Option<Operator>,
        times: Option<usize>,
        forced_motion: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match operator {
            None => self.move_cursor(motion, times, window, cx),
            Some(Operator::Change) => self.change_motion(motion, times, forced_motion, window, cx),
            Some(Operator::Delete) => self.delete_motion(motion, times, forced_motion, window, cx),
            Some(Operator::Yank) => self.yank_motion(motion, times, forced_motion, window, cx),
            Some(Operator::AddSurrounds { target: None }) => {}
            Some(Operator::Indent) => self.indent_motion(
                motion,
                times,
                forced_motion,
                IndentDirection::In,
                window,
                cx,
            ),
            Some(Operator::Rewrap) => self.rewrap_motion(motion, times, forced_motion, window, cx),
            Some(Operator::Outdent) => self.indent_motion(
                motion,
                times,
                forced_motion,
                IndentDirection::Out,
                window,
                cx,
            ),
            Some(Operator::AutoIndent) => self.indent_motion(
                motion,
                times,
                forced_motion,
                IndentDirection::Auto,
                window,
                cx,
            ),
            Some(Operator::ShellCommand) => {
                self.shell_command_motion(motion, times, forced_motion, window, cx)
            }
            Some(Operator::Lowercase) => self.convert_motion(
                motion,
                times,
                forced_motion,
                ConvertTarget::LowerCase,
                window,
                cx,
            ),
            Some(Operator::Uppercase) => self.convert_motion(
                motion,
                times,
                forced_motion,
                ConvertTarget::UpperCase,
                window,
                cx,
            ),
            Some(Operator::OppositeCase) => self.convert_motion(
                motion,
                times,
                forced_motion,
                ConvertTarget::OppositeCase,
                window,
                cx,
            ),
            Some(Operator::Rot13) => self.convert_motion(
                motion,
                times,
                forced_motion,
                ConvertTarget::Rot13,
                window,
                cx,
            ),
            Some(Operator::Rot47) => self.convert_motion(
                motion,
                times,
                forced_motion,
                ConvertTarget::Rot47,
                window,
                cx,
            ),
            Some(Operator::ToggleComments) => {
                self.toggle_comments_motion(motion, times, forced_motion, window, cx)
            }
            Some(Operator::ToggleBlockComments) => {
                self.toggle_block_comments_motion(motion, times, forced_motion, window, cx)
            }
            Some(Operator::ReplaceWithRegister) => {
                self.replace_with_register_motion(motion, times, forced_motion, window, cx)
            }
            Some(Operator::Exchange) => {
                self.exchange_motion(motion, times, forced_motion, window, cx)
            }
            Some(operator) => {
                // Can't do anything for text objects, Ignoring
                error!("Unexpected normal mode motion operator: {:?}", operator)
            }
        }
        // Exit temporary normal mode (if active).
        self.exit_temporary_normal(window, cx);
    }

    pub fn normal_object(
        &mut self,
        object: Object,
        times: Option<usize>,
        opening: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut waiting_operator: Option<Operator> = None;
        match self.maybe_pop_operator() {
            Some(Operator::Object { around }) => match self.maybe_pop_operator() {
                Some(Operator::Change) => self.change_object(object, around, times, window, cx),
                Some(Operator::Delete) => self.delete_object(object, around, times, window, cx),
                Some(Operator::Yank) => self.yank_object(object, around, times, window, cx),
                Some(Operator::Indent) => {
                    self.indent_object(object, around, IndentDirection::In, times, window, cx)
                }
                Some(Operator::Outdent) => {
                    self.indent_object(object, around, IndentDirection::Out, times, window, cx)
                }
                Some(Operator::AutoIndent) => {
                    self.indent_object(object, around, IndentDirection::Auto, times, window, cx)
                }
                Some(Operator::ShellCommand) => {
                    self.shell_command_object(object, around, window, cx);
                }
                Some(Operator::Rewrap) => self.rewrap_object(object, around, times, window, cx),
                Some(Operator::Lowercase) => {
                    self.convert_object(object, around, ConvertTarget::LowerCase, times, window, cx)
                }
                Some(Operator::Uppercase) => {
                    self.convert_object(object, around, ConvertTarget::UpperCase, times, window, cx)
                }
                Some(Operator::OppositeCase) => self.convert_object(
                    object,
                    around,
                    ConvertTarget::OppositeCase,
                    times,
                    window,
                    cx,
                ),
                Some(Operator::Rot13) => {
                    self.convert_object(object, around, ConvertTarget::Rot13, times, window, cx)
                }
                Some(Operator::Rot47) => {
                    self.convert_object(object, around, ConvertTarget::Rot47, times, window, cx)
                }
                Some(Operator::AddSurrounds { target: None }) => {
                    waiting_operator = Some(Operator::AddSurrounds {
                        target: Some(SurroundsType::Object(object, around)),
                    });
                }
                Some(Operator::ToggleComments) => {
                    self.toggle_comments_object(object, around, times, window, cx)
                }
                Some(Operator::ToggleBlockComments) => {
                    self.toggle_block_comments_object(object, around, times, window, cx)
                }
                Some(Operator::ReplaceWithRegister) => {
                    self.replace_with_register_object(object, around, window, cx)
                }
                Some(Operator::Exchange) => self.exchange_object(object, around, window, cx),
                Some(Operator::HelixMatch) => {
                    self.select_current_object(object, around, window, cx)
                }
                _ => {
                    // Can't do anything for namespace operators. Ignoring
                }
            },
            Some(Operator::HelixNext { around }) => {
                self.select_next_object(object, around, window, cx);
            }
            Some(Operator::HelixPrevious { around }) => {
                self.select_previous_object(object, around, window, cx);
            }
            Some(Operator::DeleteSurrounds) => {
                waiting_operator = Some(Operator::DeleteSurrounds);
            }
            Some(Operator::ChangeSurrounds { target: None, .. }) => {
                let bracket_anchors =
                    self.prepare_and_move_to_valid_bracket_pair(object, window, cx);
                if !bracket_anchors.is_empty() {
                    waiting_operator = Some(Operator::ChangeSurrounds {
                        target: Some(object),
                        opening,
                        bracket_anchors,
                    });
                }
            }
            _ => {
                // Can't do anything with change/delete/yank/surrounds and text objects. Ignoring
            }
        }
        self.clear_operator(window, cx);
        if let Some(operator) = waiting_operator {
            self.push_operator(operator, window, cx);
        }
    }

    pub(crate) fn move_cursor(
        &mut self,
        motion: Motion,
        times: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_editor(cx, |vim, editor, cx| {
            let text_layout_details = editor.text_layout_details(window, cx);

            // If vim is in temporary mode and the motion being used is
            // `EndOfLine` ($), we'll want to disable clipping at line ends so
            // that the newline character can be selected so that, when moving
            // back to visual mode, the cursor will be placed after the last
            // character and not before it.
            let clip_at_line_ends = editor.clip_at_line_ends(cx);
            let should_disable_clip = matches!(motion, Motion::EndOfLine { .. }) && vim.temp_mode;

            if should_disable_clip {
                editor.set_clip_at_line_ends(false, cx)
            };

            editor.change_selections(
                SelectionEffects::default().nav_history(motion.push_to_jump_list()),
                window,
                cx,
                |s| {
                    s.move_cursors_with(&mut |map, cursor, goal| {
                        motion
                            .move_point(map, cursor, goal, times, &text_layout_details)
                            .unwrap_or((cursor, goal))
                    })
                },
            );

            if should_disable_clip {
                editor.set_clip_at_line_ends(clip_at_line_ends, cx);
            };
        });
    }
}
