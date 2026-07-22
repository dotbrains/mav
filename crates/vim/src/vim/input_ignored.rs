use super::*;

impl Vim {
    pub(super) fn input_ignored(
        &mut self,
        text: Arc<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if text.is_empty() {
            return;
        }

        match self.active_operator() {
            Some(Operator::FindForward { before, multiline }) => {
                let find = Motion::FindForward {
                    before,
                    char: text.chars().next().unwrap(),
                    mode: if multiline {
                        FindRange::MultiLine
                    } else {
                        FindRange::SingleLine
                    },
                    smartcase: VimSettings::get_global(cx).use_smartcase_find,
                };
                Vim::globals(cx).last_find = Some(find.clone());
                self.motion(find, window, cx)
            }
            Some(Operator::FindBackward { after, multiline }) => {
                let find = Motion::FindBackward {
                    after,
                    char: text.chars().next().unwrap(),
                    mode: if multiline {
                        FindRange::MultiLine
                    } else {
                        FindRange::SingleLine
                    },
                    smartcase: VimSettings::get_global(cx).use_smartcase_find,
                };
                Vim::globals(cx).last_find = Some(find.clone());
                self.motion(find, window, cx)
            }
            Some(Operator::Sneak { first_char }) => {
                if let Some(first_char) = first_char {
                    if let Some(second_char) = text.chars().next() {
                        let sneak = Motion::Sneak {
                            first_char,
                            second_char,
                            smartcase: VimSettings::get_global(cx).use_smartcase_find,
                        };
                        Vim::globals(cx).last_find = Some(sneak.clone());
                        self.motion(sneak, window, cx)
                    }
                } else {
                    let first_char = text.chars().next();
                    self.pop_operator(window, cx);
                    self.push_operator(Operator::Sneak { first_char }, window, cx);
                }
            }
            Some(Operator::SneakBackward { first_char }) => {
                if let Some(first_char) = first_char {
                    if let Some(second_char) = text.chars().next() {
                        let sneak = Motion::SneakBackward {
                            first_char,
                            second_char,
                            smartcase: VimSettings::get_global(cx).use_smartcase_find,
                        };
                        Vim::globals(cx).last_find = Some(sneak.clone());
                        self.motion(sneak, window, cx)
                    }
                } else {
                    let first_char = text.chars().next();
                    self.pop_operator(window, cx);
                    self.push_operator(Operator::SneakBackward { first_char }, window, cx);
                }
            }
            Some(operator @ Operator::HelixJump { .. }) => {
                if let Some(input_char) = text.chars().next() {
                    self.handle_helix_jump_input(operator, input_char, window, cx);
                }
            }
            Some(Operator::Replace) => match self.mode {
                Mode::Normal => self.normal_replace(text, window, cx),
                Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                    self.visual_replace(text, window, cx)
                }
                Mode::HelixNormal | Mode::HelixSelect => self.helix_replace(&text, window, cx),
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::Digraph { first_char }) => {
                if let Some(first_char) = first_char {
                    if let Some(second_char) = text.chars().next() {
                        self.insert_digraph(first_char, second_char, window, cx);
                    }
                } else {
                    let first_char = text.chars().next();
                    self.pop_operator(window, cx);
                    self.push_operator(Operator::Digraph { first_char }, window, cx);
                }
            }
            Some(Operator::Literal { prefix }) => {
                self.handle_literal_input(prefix.unwrap_or_default(), &text, window, cx)
            }
            Some(Operator::AddSurrounds { target }) => match self.mode {
                Mode::Normal => {
                    if let Some(target) = target {
                        self.add_surrounds(text, target, window, cx);
                        self.clear_operator(window, cx);
                    }
                }
                Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                    self.add_surrounds(text, SurroundsType::Selection, window, cx);
                    self.clear_operator(window, cx);
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::ChangeSurrounds {
                target,
                opening,
                bracket_anchors,
            }) => match self.mode {
                Mode::Normal => {
                    if let Some(target) = target {
                        self.change_surrounds(text, target, opening, bracket_anchors, window, cx);
                        self.clear_operator(window, cx);
                    }
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::DeleteSurrounds) => match self.mode {
                Mode::Normal => {
                    self.delete_surrounds(text, window, cx);
                    self.clear_operator(window, cx);
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::HelixSurroundAdd) => match self.mode {
                Mode::HelixNormal | Mode::HelixSelect => {
                    self.update_editor(cx, |_, editor, cx| {
                        editor.change_selections(Default::default(), window, cx, |s| {
                            s.move_with(&mut |map, selection| {
                                if selection.is_empty() {
                                    selection.end = movement::right(map, selection.start);
                                }
                            });
                        });
                    });
                    self.helix_surround_add(&text, window, cx);
                    self.switch_mode(Mode::HelixNormal, false, window, cx);
                    self.clear_operator(window, cx);
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::HelixSurroundReplace {
                replaced_char: Some(old),
            }) => match self.mode {
                Mode::HelixNormal | Mode::HelixSelect => {
                    if let Some(new_char) = text.chars().next() {
                        self.helix_surround_replace(old, new_char, window, cx);
                    }
                    self.clear_operator(window, cx);
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::HelixSurroundReplace {
                replaced_char: None,
            }) => match self.mode {
                Mode::HelixNormal | Mode::HelixSelect => {
                    if let Some(ch) = text.chars().next() {
                        self.pop_operator(window, cx);
                        self.push_operator(
                            Operator::HelixSurroundReplace {
                                replaced_char: Some(ch),
                            },
                            window,
                            cx,
                        );
                    }
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::HelixSurroundDelete) => match self.mode {
                Mode::HelixNormal | Mode::HelixSelect => {
                    if let Some(ch) = text.chars().next() {
                        self.helix_surround_delete(ch, window, cx);
                    }
                    self.clear_operator(window, cx);
                }
                _ => self.clear_operator(window, cx),
            },
            Some(Operator::Mark) => self.create_mark(text, window, cx),
            Some(Operator::RecordRegister) => {
                self.record_register(text.chars().next().unwrap(), window, cx)
            }
            Some(Operator::ReplayRegister) => {
                self.replay_register(text.chars().next().unwrap(), window, cx)
            }
            Some(Operator::Register) => match self.mode {
                Mode::Insert => {
                    self.update_editor(cx, |_, editor, cx| {
                        if let Some(register) = Vim::update_globals(cx, |globals, cx| {
                            globals.read_register(text.chars().next(), Some(editor), cx)
                        }) {
                            editor.do_paste(
                                &register.text.to_string(),
                                register.clipboard_selections,
                                false,
                                window,
                                cx,
                            )
                        }
                    });
                    self.clear_operator(window, cx);
                }
                _ => {
                    self.select_register(text, window, cx);
                }
            },
            Some(Operator::Jump { line }) => self.jump(text, line, true, window, cx),
            _ => {
                if self.mode == Mode::Replace {
                    self.multi_replace(text, window, cx)
                }

                if self.mode == Mode::Normal {
                    self.update_editor(cx, |_, editor, cx| {
                        editor.accept_edit_prediction(
                            &editor::actions::AcceptEditPrediction {},
                            window,
                            cx,
                        );
                    });
                }
            }
        }
    }
}
