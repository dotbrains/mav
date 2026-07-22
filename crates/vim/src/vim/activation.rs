use super::*;

impl Vim {
    pub(super) fn register(
        editor: &mut Editor,
        window: Option<&mut Window>,
        cx: &mut Context<Editor>,
    ) {
        let Some(window) = window else {
            return;
        };

        if !editor.use_modal_editing() {
            return;
        }

        let mut was_enabled = Vim::enabled(cx);
        let mut was_helix_enabled = HelixModeSetting::get_global(cx).0;
        let mut was_toggle = VimSettings::get_global(cx).toggle_relative_line_numbers;
        cx.observe_global_in::<SettingsStore>(window, move |editor, window, cx| {
            let enabled = Vim::enabled(cx);
            let helix_enabled = HelixModeSetting::get_global(cx).0;
            let toggle = VimSettings::get_global(cx).toggle_relative_line_numbers;
            if enabled && was_enabled && (toggle != was_toggle) {
                if toggle {
                    let is_relative = editor
                        .addon::<VimAddon>()
                        .map(|vim| vim.entity.read(cx).mode != Mode::Insert);
                    editor.set_relative_line_number(is_relative, cx)
                } else {
                    editor.set_relative_line_number(None, cx)
                }
            }
            let helix_changed = was_helix_enabled != helix_enabled;
            was_toggle = toggle;
            was_helix_enabled = helix_enabled;

            let state_changed = (was_enabled != enabled) || (was_enabled && helix_changed);
            if !state_changed {
                return;
            }
            if was_enabled {
                Self::deactivate(editor, cx);
            }
            was_enabled = enabled;
            if enabled {
                Self::activate(editor, window, cx);
            }
        })
        .detach();
        if was_enabled {
            Self::activate(editor, window, cx)
        }
    }

    pub(super) fn activate(editor: &mut Editor, window: &mut Window, cx: &mut Context<Editor>) {
        let vim = Vim::new(window, cx);
        let state = vim.update(cx, |vim, cx| {
            if !editor.use_modal_editing() {
                vim.mode = Mode::Insert;
            }

            vim.state_for_editor_settings(cx)
        });

        Vim::sync_vim_settings_to_editor(&state, editor, window, cx);

        editor.register_addon(VimAddon {
            entity: vim.clone(),
        });

        vim.update(cx, |_, cx| {
            Vim::action(editor, cx, |vim, _: &SwitchToNormalMode, window, cx| {
                vim.switch_mode(Mode::Normal, false, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &SwitchToInsertMode, window, cx| {
                vim.switch_mode(Mode::Insert, false, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &SwitchToReplaceMode, window, cx| {
                vim.switch_mode(Mode::Replace, false, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &SwitchToVisualMode, window, cx| {
                vim.switch_mode(Mode::Visual, false, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &SwitchToVisualLineMode, window, cx| {
                vim.switch_mode(Mode::VisualLine, false, window, cx)
            });

            Vim::action(
                editor,
                cx,
                |vim, _: &SwitchToVisualBlockMode, window, cx| {
                    vim.switch_mode(Mode::VisualBlock, false, window, cx)
                },
            );

            Vim::action(
                editor,
                cx,
                |vim, _: &SwitchToHelixNormalMode, window, cx| {
                    vim.switch_mode(Mode::HelixNormal, true, window, cx)
                },
            );
            Vim::action(editor, cx, |_, _: &PushForcedMotion, _, cx| {
                Vim::globals(cx).forced_motion = true;
            });
            Vim::action(editor, cx, |vim, action: &PushObject, window, cx| {
                vim.push_operator(
                    Operator::Object {
                        around: action.around,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, action: &PushFindForward, window, cx| {
                vim.push_operator(
                    Operator::FindForward {
                        before: action.before,
                        multiline: action.multiline,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, action: &PushFindBackward, window, cx| {
                vim.push_operator(
                    Operator::FindBackward {
                        after: action.after,
                        multiline: action.multiline,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, action: &PushSneak, window, cx| {
                vim.push_operator(
                    Operator::Sneak {
                        first_char: action.first_char,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, action: &PushSneakBackward, window, cx| {
                vim.push_operator(
                    Operator::SneakBackward {
                        first_char: action.first_char,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, _: &PushAddSurrounds, window, cx| {
                vim.push_operator(Operator::AddSurrounds { target: None }, window, cx)
            });

            Vim::action(
                editor,
                cx,
                |vim, action: &PushChangeSurrounds, window, cx| {
                    vim.push_operator(
                        Operator::ChangeSurrounds {
                            target: action.target,
                            opening: false,
                            bracket_anchors: Vec::new(),
                        },
                        window,
                        cx,
                    )
                },
            );

            Vim::action(editor, cx, |vim, action: &PushJump, window, cx| {
                vim.push_operator(Operator::Jump { line: action.line }, window, cx)
            });

            Vim::action(editor, cx, |vim, action: &PushDigraph, window, cx| {
                vim.push_operator(
                    Operator::Digraph {
                        first_char: action.first_char,
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, action: &PushLiteral, window, cx| {
                vim.push_operator(
                    Operator::Literal {
                        prefix: action.prefix.clone(),
                    },
                    window,
                    cx,
                )
            });

            Vim::action(editor, cx, |vim, _: &PushChange, window, cx| {
                vim.push_operator(Operator::Change, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushDelete, window, cx| {
                vim.push_operator(Operator::Delete, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushYank, window, cx| {
                vim.push_operator(Operator::Yank, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushReplace, window, cx| {
                vim.push_operator(Operator::Replace, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushDeleteSurrounds, window, cx| {
                vim.push_operator(Operator::DeleteSurrounds, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushMark, window, cx| {
                vim.push_operator(Operator::Mark, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushIndent, window, cx| {
                vim.push_operator(Operator::Indent, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushOutdent, window, cx| {
                vim.push_operator(Operator::Outdent, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushAutoIndent, window, cx| {
                vim.push_operator(Operator::AutoIndent, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushRewrap, window, cx| {
                vim.push_operator(Operator::Rewrap, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushShellCommand, window, cx| {
                vim.push_operator(Operator::ShellCommand, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushLowercase, window, cx| {
                vim.push_operator(Operator::Lowercase, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushUppercase, window, cx| {
                vim.push_operator(Operator::Uppercase, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushOppositeCase, window, cx| {
                vim.push_operator(Operator::OppositeCase, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushRot13, window, cx| {
                vim.push_operator(Operator::Rot13, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushRot47, window, cx| {
                vim.push_operator(Operator::Rot47, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushRegister, window, cx| {
                vim.push_operator(Operator::Register, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushRecordRegister, window, cx| {
                vim.push_operator(Operator::RecordRegister, window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushReplayRegister, window, cx| {
                vim.push_operator(Operator::ReplayRegister, window, cx)
            });

            Vim::action(
                editor,
                cx,
                |vim, _: &PushReplaceWithRegister, window, cx| {
                    vim.push_operator(Operator::ReplaceWithRegister, window, cx)
                },
            );

            Vim::action(editor, cx, |vim, _: &Exchange, window, cx| {
                if vim.mode.is_visual() {
                    vim.exchange_visual(window, cx)
                } else {
                    vim.push_operator(Operator::Exchange, window, cx)
                }
            });

            Vim::action(editor, cx, |vim, _: &ClearExchange, window, cx| {
                vim.clear_exchange(window, cx)
            });

            Vim::action(editor, cx, |vim, _: &PushToggleComments, window, cx| {
                vim.push_operator(Operator::ToggleComments, window, cx)
            });

            Vim::action(
                editor,
                cx,
                |vim, _: &PushToggleBlockComments, window, cx| {
                    vim.push_operator(Operator::ToggleBlockComments, window, cx)
                },
            );

            Vim::action(editor, cx, |vim, _: &ClearOperators, window, cx| {
                vim.clear_operator(window, cx)
            });
            Vim::action(editor, cx, |vim, n: &Number, window, cx| {
                vim.push_count_digit(n.0, window, cx);
            });
            Vim::action(editor, cx, |vim, _: &Tab, window, cx| {
                vim.input_ignored(" ".into(), window, cx)
            });
            Vim::action(
                editor,
                cx,
                |vim, action: &editor::actions::AcceptEditPrediction, window, cx| {
                    vim.update_editor(cx, |_, editor, cx| {
                        editor.accept_edit_prediction(action, window, cx);
                    });
                    // In non-insertion modes, predictions will be hidden and instead a jump will be
                    // displayed (and performed by `accept_edit_prediction`). This switches to
                    // insert mode so that the prediction is displayed after the jump.
                    match vim.mode {
                        Mode::Replace => {}
                        _ => vim.switch_mode(Mode::Insert, true, window, cx),
                    };
                },
            );
            Vim::action(editor, cx, |vim, _: &Enter, window, cx| {
                vim.input_ignored("\n".into(), window, cx)
            });
            Vim::action(editor, cx, |vim, _: &PushHelixMatch, window, cx| {
                vim.push_operator(Operator::HelixMatch, window, cx)
            });
            Vim::action(editor, cx, |vim, action: &PushHelixNext, window, cx| {
                vim.push_operator(
                    Operator::HelixNext {
                        around: action.around,
                    },
                    window,
                    cx,
                );
            });
            Vim::action(editor, cx, |vim, action: &PushHelixPrevious, window, cx| {
                vim.push_operator(
                    Operator::HelixPrevious {
                        around: action.around,
                    },
                    window,
                    cx,
                );
            });

            Vim::action(
                editor,
                cx,
                |vim, _: &editor::actions::Paste, window, cx| match vim.mode {
                    Mode::Replace => vim.paste_replace(window, cx),
                    Mode::Visual | Mode::VisualLine | Mode::VisualBlock => {
                        vim.selected_register.replace('+');
                        let mut action = VimPaste::default();
                        action.preserve_clipboard = true;
                        vim.paste(&action, window, cx);
                    }
                    _ => {
                        vim.update_editor(cx, |_, editor, cx| editor.paste(&Paste, window, cx));
                    }
                },
            );

            normal::register(editor, cx);
            insert::register(editor, cx);
            helix::register(editor, cx);
            motion::register(editor, cx);
            command::register(editor, cx);
            replace::register(editor, cx);
            indent::register(editor, cx);
            rewrap::register(editor, cx);
            object::register(editor, cx);
            visual::register(editor, cx);
            change_list::register(editor, cx);
            digraph::register(editor, cx);

            if editor.is_focused(window) {
                cx.defer_in(window, |vim, window, cx| {
                    vim.focused(false, window, cx);
                })
            }
        })
    }

    pub(super) fn deactivate(editor: &mut Editor, cx: &mut Context<Editor>) {
        editor.set_cursor_shape(
            EditorSettings::get_global(cx)
                .cursor_shape
                .unwrap_or_default(),
            cx,
        );
        editor.set_clip_at_line_ends(false, cx);
        editor.set_collapse_matches(false);
        editor.set_input_enabled(true);
        editor.set_expects_character_input(true);
        editor.set_autoindent(true);
        editor.selections.set_line_mode(false);
        editor.unregister_addon::<VimAddon>();
        editor.set_relative_line_number(None, cx);
        if let Some(vim) = Vim::globals(cx).focused_vim()
            && vim.entity_id() == cx.entity().entity_id()
        {
            Vim::globals(cx).focused_vim = None;
        }
    }
}
