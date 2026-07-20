use super::*;

#[gpui::test]
async fn test_snippet_placeholder_choices(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (text, insertion_ranges) = marked_text_ranges(
        indoc! {"
            ˇ
        "},
        false,
    );

    let buffer = cx.update(|cx| MultiBuffer::build_simple(&text, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    _ = editor.update_in(cx, |editor, window, cx| {
        let snippet = Snippet::parse("type ${1|,i32,u32|} = $2").unwrap();

        editor
            .insert_snippet(
                &insertion_ranges
                    .iter()
                    .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
                    .collect::<Vec<_>>(),
                snippet,
                window,
                cx,
            )
            .unwrap();

        fn assert(editor: &mut Editor, cx: &mut Context<Editor>, marked_text: &str) {
            let (expected_text, selection_ranges) = marked_text_ranges(marked_text, false);
            assert_eq!(editor.text(cx), expected_text);
            assert_eq!(
                editor.selections.ranges(&editor.display_snapshot(cx)),
                selection_ranges
                    .iter()
                    .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
                    .collect::<Vec<_>>()
            );
        }

        assert(
            editor,
            cx,
            indoc! {"
            type «» =•
            "},
        );

        assert!(editor.context_menu_visible(), "There should be a matches");
    });
}

#[gpui::test]
async fn test_snippet_choices_menu_survives_completion_refresh(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (text, insertion_ranges) = marked_text_ranges(
        indoc! {"
            ˇ
        "},
        false,
    );

    let buffer = cx.update(|cx| MultiBuffer::build_simple(&text, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    _ = editor.update_in(cx, |editor, window, cx| {
        let snippet = Snippet::parse("type ${1|i32,u32|} = $2").unwrap();

        editor
            .insert_snippet(
                &insertion_ranges
                    .iter()
                    .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
                    .collect::<Vec<_>>(),
                snippet,
                window,
                cx,
            )
            .unwrap();

        assert!(
            editor.context_menu_visible(),
            "Snippet choices menu should be visible after inserting the choice tabstop"
        );

        editor.open_or_update_completions_menu(None, None, false, window, cx);

        assert!(
            editor.context_menu_visible(),
            "Snippet choices menu should remain visible after a completion refresh with an empty query"
        );
    });
}
