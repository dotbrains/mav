use super::support::*;
use super::*;

#[gpui::test]
async fn test_inline_edit_prediction_keybind_selection_cases(cx: &mut gpui::TestAppContext) {
    enum InlineKeybindState {
        Normal,
        ShowingCompletions,
        InLeadingWhitespace,
        ShowingCompletionsAndLeadingWhitespace,
    }

    enum ExpectedKeystroke {
        DefaultAccept,
        DefaultPreview,
        Literal(&'static str),
    }

    struct InlineKeybindCase {
        name: &'static str,
        use_default_keymap: bool,
        mode: EditPredictionsMode,
        extra_bindings: Vec<KeyBinding>,
        state: InlineKeybindState,
        expected_accept_keystroke: ExpectedKeystroke,
        expected_preview_keystroke: ExpectedKeystroke,
        expected_displayed_keystroke: ExpectedKeystroke,
    }

    init_test(cx, |_| {});
    load_default_keymap(cx);
    let mut default_cx = EditorTestContext::new(cx).await;
    let provider = default_cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut default_cx);
    default_cx.set_state("let x = ˇ;");
    propose_edits(&provider, vec![(8..8, "42")], &mut default_cx);
    default_cx
        .update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    let (default_accept_keystroke, default_preview_keystroke) =
        default_cx.update_editor(|editor, window, cx| {
            let keybind_display = editor.edit_prediction_keybind_display(
                EditPredictionKeybindSurface::Inline,
                window,
                cx,
            );
            let accept_keystroke = keybind_display
                .accept_keystroke
                .as_ref()
                .expect("default inline edit prediction should have an accept binding")
                .clone();
            let preview_keystroke = keybind_display
                .preview_keystroke
                .as_ref()
                .expect("default inline edit prediction should have a preview binding")
                .clone();
            (accept_keystroke, preview_keystroke)
        });

    let cases = [
        InlineKeybindCase {
            name: "default setup prefers tab over alt-tab for accept",
            use_default_keymap: true,
            mode: EditPredictionsMode::Eager,
            extra_bindings: Vec::new(),
            state: InlineKeybindState::Normal,
            expected_accept_keystroke: ExpectedKeystroke::DefaultAccept,
            expected_preview_keystroke: ExpectedKeystroke::DefaultPreview,
            expected_displayed_keystroke: ExpectedKeystroke::DefaultAccept,
        },
        InlineKeybindCase {
            name: "subtle mode displays preview binding inline",
            use_default_keymap: true,
            mode: EditPredictionsMode::Subtle,
            extra_bindings: Vec::new(),
            state: InlineKeybindState::Normal,
            expected_accept_keystroke: ExpectedKeystroke::DefaultPreview,
            expected_preview_keystroke: ExpectedKeystroke::DefaultPreview,
            expected_displayed_keystroke: ExpectedKeystroke::DefaultPreview,
        },
        InlineKeybindCase {
            name: "removing default tab binding still displays tab",
            use_default_keymap: true,
            mode: EditPredictionsMode::Eager,
            extra_bindings: vec![KeyBinding::new(
                "tab",
                NoAction,
                Some("Editor && edit_prediction && edit_prediction_mode == eager"),
            )],
            state: InlineKeybindState::Normal,
            expected_accept_keystroke: ExpectedKeystroke::DefaultPreview,
            expected_preview_keystroke: ExpectedKeystroke::DefaultPreview,
            expected_displayed_keystroke: ExpectedKeystroke::DefaultPreview,
        },
        InlineKeybindCase {
            name: "custom-only rebound accept key uses replacement key",
            use_default_keymap: true,
            mode: EditPredictionsMode::Eager,
            extra_bindings: vec![KeyBinding::new(
                "ctrl-enter",
                AcceptEditPrediction,
                Some("Editor && edit_prediction"),
            )],
            state: InlineKeybindState::Normal,
            expected_accept_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_preview_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_displayed_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
        },
        InlineKeybindCase {
            name: "showing completions restores conflict-context binding",
            use_default_keymap: true,
            mode: EditPredictionsMode::Eager,
            extra_bindings: vec![KeyBinding::new(
                "ctrl-enter",
                AcceptEditPrediction,
                Some("Editor && edit_prediction && showing_completions"),
            )],
            state: InlineKeybindState::ShowingCompletions,
            expected_accept_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_preview_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_displayed_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
        },
        InlineKeybindCase {
            name: "leading whitespace restores conflict-context binding",
            use_default_keymap: false,
            mode: EditPredictionsMode::Eager,
            extra_bindings: vec![KeyBinding::new(
                "ctrl-enter",
                AcceptEditPrediction,
                Some("Editor && edit_prediction && in_leading_whitespace"),
            )],
            state: InlineKeybindState::InLeadingWhitespace,
            expected_accept_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_preview_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_displayed_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
        },
        InlineKeybindCase {
            name: "showing completions and leading whitespace restore combined conflict binding",
            use_default_keymap: false,
            mode: EditPredictionsMode::Eager,
            extra_bindings: vec![KeyBinding::new(
                "ctrl-enter",
                AcceptEditPrediction,
                Some("Editor && edit_prediction && showing_completions && in_leading_whitespace"),
            )],
            state: InlineKeybindState::ShowingCompletionsAndLeadingWhitespace,
            expected_accept_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_preview_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
            expected_displayed_keystroke: ExpectedKeystroke::Literal("ctrl-enter"),
        },
    ];

    for case in cases {
        init_test(cx, |_| {});
        if case.use_default_keymap {
            load_default_keymap(cx);
        }
        update_test_language_settings(cx, &|settings| {
            settings.edit_predictions.get_or_insert_default().mode = Some(case.mode);
        });

        if !case.extra_bindings.is_empty() {
            cx.update(|cx| cx.bind_keys(case.extra_bindings.clone()));
        }

        let mut cx = EditorTestContext::new(cx).await;
        let provider = cx.new(|_| FakeEditPredictionDelegate::default());
        assign_editor_completion_provider(provider.clone(), &mut cx);

        match case.state {
            InlineKeybindState::Normal | InlineKeybindState::ShowingCompletions => {
                cx.set_state("let x = ˇ;");
            }
            InlineKeybindState::InLeadingWhitespace
            | InlineKeybindState::ShowingCompletionsAndLeadingWhitespace => {
                cx.set_state(indoc! {"
                    fn main() {
                        ˇ
                    }
                "});
            }
        }

        propose_edits(&provider, vec![(8..8, "42")], &mut cx);
        cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

        if matches!(
            case.state,
            InlineKeybindState::ShowingCompletions
                | InlineKeybindState::ShowingCompletionsAndLeadingWhitespace
        ) {
            assign_editor_completion_menu_provider(&mut cx);
            cx.update_editor(|editor, window, cx| {
                editor.show_completions(&ShowCompletions, window, cx);
            });
            cx.run_until_parked();
        }

        cx.update_editor(|editor, window, cx| {
            assert!(
                editor.has_active_edit_prediction(),
                "case '{}' should have an active edit prediction",
                case.name
            );

            let keybind_display = editor.edit_prediction_keybind_display(
                EditPredictionKeybindSurface::Inline,
                window,
                cx,
            );
            let accept_keystroke = keybind_display
                .accept_keystroke
                .as_ref()
                .unwrap_or_else(|| panic!("case '{}' should have an accept binding", case.name));
            let preview_keystroke = keybind_display
                .preview_keystroke
                .as_ref()
                .unwrap_or_else(|| panic!("case '{}' should have a preview binding", case.name));
            let displayed_keystroke = keybind_display
                .displayed_keystroke
                .as_ref()
                .unwrap_or_else(|| panic!("case '{}' should have a displayed binding", case.name));

            let expected_accept_keystroke = match case.expected_accept_keystroke {
                ExpectedKeystroke::DefaultAccept => default_accept_keystroke.clone(),
                ExpectedKeystroke::DefaultPreview => default_preview_keystroke.clone(),
                ExpectedKeystroke::Literal(keystroke) => KeybindingKeystroke::from_keystroke(
                    Keystroke::parse(keystroke).expect("expected test keystroke to parse"),
                ),
            };
            let expected_preview_keystroke = match case.expected_preview_keystroke {
                ExpectedKeystroke::DefaultAccept => default_accept_keystroke.clone(),
                ExpectedKeystroke::DefaultPreview => default_preview_keystroke.clone(),
                ExpectedKeystroke::Literal(keystroke) => KeybindingKeystroke::from_keystroke(
                    Keystroke::parse(keystroke).expect("expected test keystroke to parse"),
                ),
            };
            let expected_displayed_keystroke = match case.expected_displayed_keystroke {
                ExpectedKeystroke::DefaultAccept => default_accept_keystroke.clone(),
                ExpectedKeystroke::DefaultPreview => default_preview_keystroke.clone(),
                ExpectedKeystroke::Literal(keystroke) => KeybindingKeystroke::from_keystroke(
                    Keystroke::parse(keystroke).expect("expected test keystroke to parse"),
                ),
            };

            assert_eq!(
                accept_keystroke, &expected_accept_keystroke,
                "case '{}' selected the wrong accept binding",
                case.name
            );
            assert_eq!(
                preview_keystroke, &expected_preview_keystroke,
                "case '{}' selected the wrong preview binding",
                case.name
            );
            assert_eq!(
                displayed_keystroke, &expected_displayed_keystroke,
                "case '{}' selected the wrong displayed binding",
                case.name
            );

            if matches!(case.mode, EditPredictionsMode::Subtle) {
                assert!(
                    editor.edit_prediction_requires_modifier(),
                    "case '{}' should require a modifier",
                    case.name
                );
            }
        });
    }
}

#[gpui::test]
async fn test_tab_accepts_edit_prediction_over_completion(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    load_default_keymap(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    assign_editor_completion_provider(provider.clone(), &mut cx);
    cx.set_state("let x = ˇ;");

    propose_edits(&provider, vec![(8..8, "42")], &mut cx);
    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));

    assert_editor_active_edit_completion(&mut cx, |_, edits| {
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].1.as_ref(), "42");
    });

    cx.simulate_keystroke("tab");
    cx.run_until_parked();

    cx.assert_editor_state("let x = 42ˇ;");
}
