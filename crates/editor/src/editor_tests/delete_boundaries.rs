use super::*;

#[gpui::test]
async fn test_delete_to_line_boundary(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    for selection in ["one «two threeˇ» four", "one «ˇtwo three» four"] {
        cx.set_state(selection);
        cx.update_editor(|editor, window, cx| {
            editor.delete_to_beginning_of_line(
                &DeleteToBeginningOfLine {
                    stop_at_indent: false,
                },
                window,
                cx,
            );
            assert_eq!(editor.text(cx), " four");
        });

        cx.set_state(selection);
        cx.update_editor(|editor, window, cx| {
            editor.delete_to_end_of_line(&DeleteToEndOfLine, window, cx);
            assert_eq!(editor.text(cx), "one ");
        });
    }
}

#[gpui::test]
async fn test_delete_to_word_boundary(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // For an empty selection, the preceding word fragment is deleted.
    // For non-empty selections, only selected characters are deleted.
    cx.set_state("onˇe two t«hreˇ»e four");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: false,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇe two tˇe four");

    cx.set_state("e tˇwo te «fˇ»our");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: false,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("e tˇ te ˇour");
}

#[gpui::test]
async fn test_delete_whitespaces(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("here is some text    ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: false,
                ignore_brackets: true,
            },
            window,
            cx,
        );
    });
    // Continuous whitespace sequences are removed entirely, words behind them are not affected by the deletion action.
    cx.assert_editor_state("here is some textˇwith a space");

    cx.set_state("here is some text    ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: false,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("here is some textˇwith a space");

    cx.set_state("here is some textˇ    with a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: false,
                ignore_brackets: true,
            },
            window,
            cx,
        );
    });
    // Same happens in the other direction.
    cx.assert_editor_state("here is some textˇwith a space");

    cx.set_state("here is some textˇ    with a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: false,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("here is some textˇwith a space");

    cx.set_state("here is some textˇ    with a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("here is some textˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("here is some ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    // Single whitespaces are removed with the word behind them.
    cx.assert_editor_state("here is ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("here ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇwith a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    // Same happens in the other direction.
    cx.assert_editor_state("ˇ a space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇ space");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇ");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇ");
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state("ˇ");
}
