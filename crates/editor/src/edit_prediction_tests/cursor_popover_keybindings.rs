use super::support::*;
use super::*;

#[gpui::test]
async fn test_cursor_popover_edit_prediction_keybind_cases(cx: &mut gpui::TestAppContext) {
    enum CursorPopoverPredictionKind {
        SingleLine,
        MultiLine,
        SingleLineWithPreview,
        MultiLineWithPreview,
        DeleteSingleNewline,
        StaleSingleLineAfterMultiLine,
    }

    struct CursorPopoverCase {
        name: &'static str,
        prediction_kind: CursorPopoverPredictionKind,
        expected_action: EditPredictionKeybindAction,
    }

    let cases = [
        CursorPopoverCase {
            name: "single line prediction uses accept action",
            prediction_kind: CursorPopoverPredictionKind::SingleLine,
            expected_action: EditPredictionKeybindAction::Accept,
        },
        CursorPopoverCase {
            name: "multi line prediction uses preview action",
            prediction_kind: CursorPopoverPredictionKind::MultiLine,
            expected_action: EditPredictionKeybindAction::Preview,
        },
        CursorPopoverCase {
            name: "single line prediction with preview still uses accept action",
            prediction_kind: CursorPopoverPredictionKind::SingleLineWithPreview,
            expected_action: EditPredictionKeybindAction::Accept,
        },
        CursorPopoverCase {
            name: "multi line prediction with preview uses preview action",
            prediction_kind: CursorPopoverPredictionKind::MultiLineWithPreview,
            expected_action: EditPredictionKeybindAction::Preview,
        },
        CursorPopoverCase {
            name: "single line newline deletion uses accept action",
            prediction_kind: CursorPopoverPredictionKind::DeleteSingleNewline,
            expected_action: EditPredictionKeybindAction::Accept,
        },
        CursorPopoverCase {
            name: "stale multi line prediction does not force preview action",
            prediction_kind: CursorPopoverPredictionKind::StaleSingleLineAfterMultiLine,
            expected_action: EditPredictionKeybindAction::Accept,
        },
    ];

    for case in cases {
        init_test(cx, |_| {});
        load_default_keymap(cx);

        let mut cx = EditorTestContext::new(cx).await;
        let provider = cx.new(|_| FakeEditPredictionDelegate::default());
        assign_editor_completion_provider(provider.clone(), &mut cx);

        match case.prediction_kind {
            CursorPopoverPredictionKind::SingleLine => {
                cx.set_state("let x = ˇ;");
                propose_edits(&provider, vec![(8..8, "42")], &mut cx);
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
            CursorPopoverPredictionKind::MultiLine => {
                cx.set_state("let x = ˇ;");
                propose_edits(&provider, vec![(8..8, "42\n43")], &mut cx);
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
            CursorPopoverPredictionKind::SingleLineWithPreview => {
                cx.set_state("let x = ˇ;");
                propose_edits_with_preview(&provider, vec![(8..8, "42")], &mut cx).await;
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
            CursorPopoverPredictionKind::MultiLineWithPreview => {
                cx.set_state("let x = ˇ;");
                propose_edits_with_preview(&provider, vec![(8..8, "42\n43")], &mut cx).await;
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
            CursorPopoverPredictionKind::DeleteSingleNewline => {
                cx.set_state(indoc! {"
                    fn main() {
                        let value = 1;
                        ˇprintln!(\"done\");
                    }
                "});
                propose_edits(
                    &provider,
                    vec![(Point::new(1, 18)..Point::new(2, 17), "")],
                    &mut cx,
                );
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
            CursorPopoverPredictionKind::StaleSingleLineAfterMultiLine => {
                cx.set_state("let x = ˇ;");
                propose_edits(&provider, vec![(8..8, "42\n43")], &mut cx);
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
                cx.update_editor(|editor, _window, cx| {
                    assert!(editor.active_edit_prediction.is_some());
                    assert!(editor.stale_edit_prediction_in_menu.is_none());
                    editor.take_active_edit_prediction(true, cx);
                    assert!(editor.active_edit_prediction.is_none());
                    assert!(editor.stale_edit_prediction_in_menu.is_some());
                });

                propose_edits(&provider, vec![(8..8, "42")], &mut cx);
                cx.update_editor(|editor, window, cx| {
                    editor.update_visible_edit_prediction(window, cx)
                });
            }
        }

        cx.update_editor(|editor, window, cx| {
            assert!(
                editor.has_active_edit_prediction(),
                "case '{}' should have an active edit prediction",
                case.name
            );

            let keybind_display = editor.edit_prediction_keybind_display(
                EditPredictionKeybindSurface::CursorPopoverExpanded,
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

            assert_eq!(
                keybind_display.action, case.expected_action,
                "case '{}' selected the wrong cursor popover action",
                case.name
            );
            assert_eq!(
                accept_keystroke.key(),
                "tab",
                "case '{}' selected the wrong accept binding",
                case.name
            );
            assert!(
                preview_keystroke.modifiers().modified(),
                "case '{}' should use a modified preview binding",
                case.name
            );

            if matches!(
                case.prediction_kind,
                CursorPopoverPredictionKind::StaleSingleLineAfterMultiLine
            ) {
                assert!(
                    editor.stale_edit_prediction_in_menu.is_none(),
                    "case '{}' should clear stale menu state",
                    case.name
                );
            }
        });
    }
}
