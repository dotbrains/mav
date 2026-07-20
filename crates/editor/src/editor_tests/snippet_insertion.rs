use super::*;

#[gpui::test]
async fn test_snippet_tabstop_navigation_with_placeholders(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    fn assert_state(editor: &mut Editor, cx: &mut Context<Editor>, marked_text: &str) {
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

    let (text, insertion_ranges) = marked_text_ranges(
        indoc! {"
            ˇ
        "},
        false,
    );

    let buffer = cx.update(|cx| MultiBuffer::build_simple(&text, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    _ = editor.update_in(cx, |editor, window, cx| {
        let snippet = Snippet::parse("type ${1|,i32,u32|} = $2; $3").unwrap();

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

        assert_state(
            editor,
            cx,
            indoc! {"
            type «» = ;•
            "},
        );

        assert!(
            editor.context_menu_visible(),
            "Context menu should be visible for placeholder choices"
        );

        editor.next_snippet_tabstop(&NextSnippetTabstop, window, cx);

        assert_state(
            editor,
            cx,
            indoc! {"
            type  = «»;•
            "},
        );

        assert!(
            !editor.context_menu_visible(),
            "Context menu should be hidden after moving to next tabstop"
        );

        editor.next_snippet_tabstop(&NextSnippetTabstop, window, cx);

        assert_state(
            editor,
            cx,
            indoc! {"
            type  = ; ˇ
            "},
        );

        editor.next_snippet_tabstop(&NextSnippetTabstop, window, cx);

        assert_state(
            editor,
            cx,
            indoc! {"
            type  = ; ˇ
            "},
        );
    });

    _ = editor.update_in(cx, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        editor.backspace(&Backspace, window, cx);

        let snippet = Snippet::parse("fn ${1|,foo,bar|} = ${2:value}; $3").unwrap();
        let insertion_ranges = editor
            .selections
            .all(&editor.display_snapshot(cx))
            .iter()
            .map(|s| s.range())
            .collect::<Vec<_>>();

        editor
            .insert_snippet(&insertion_ranges, snippet, window, cx)
            .unwrap();

        assert_state(editor, cx, "fn «» = value;•");

        assert!(
            editor.context_menu_visible(),
            "Context menu should be visible for placeholder choices"
        );

        editor.next_snippet_tabstop(&NextSnippetTabstop, window, cx);

        assert_state(editor, cx, "fn  = «valueˇ»;•");

        editor.previous_snippet_tabstop(&PreviousSnippetTabstop, window, cx);

        assert_state(editor, cx, "fn «» = value;•");

        assert!(
            editor.context_menu_visible(),
            "Context menu should be visible again after returning to first tabstop"
        );

        editor.previous_snippet_tabstop(&PreviousSnippetTabstop, window, cx);

        assert_state(editor, cx, "fn «» = value;•");
    });
}

#[gpui::test]
async fn test_snippets(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        a.ˇ b
        a.ˇ b
        a.ˇ b
    "});

    cx.update_editor(|editor, window, cx| {
        let snippet = Snippet::parse("f(${1:one}, ${2:two}, ${1:three})$0").unwrap();
        let insertion_ranges = editor
            .selections
            .all(&editor.display_snapshot(cx))
            .iter()
            .map(|s| s.range())
            .collect::<Vec<_>>();
        editor
            .insert_snippet(&insertion_ranges, snippet, window, cx)
            .unwrap();
    });

    cx.assert_editor_state(indoc! {"
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
    "});

    // Can't move earlier than the first tab stop
    cx.update_editor(|editor, window, cx| {
        assert!(!editor.move_to_prev_snippet_tabstop(window, cx))
    });
    cx.assert_editor_state(indoc! {"
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
    "});

    cx.update_editor(|editor, window, cx| assert!(editor.move_to_next_snippet_tabstop(window, cx)));
    cx.assert_editor_state(indoc! {"
        a.f(one, «twoˇ», three) b
        a.f(one, «twoˇ», three) b
        a.f(one, «twoˇ», three) b
    "});

    cx.update_editor(|editor, window, cx| assert!(editor.move_to_prev_snippet_tabstop(window, cx)));
    cx.assert_editor_state(indoc! {"
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
        a.f(«oneˇ», two, «threeˇ») b
    "});

    cx.update_editor(|editor, window, cx| assert!(editor.move_to_next_snippet_tabstop(window, cx)));
    cx.assert_editor_state(indoc! {"
        a.f(one, «twoˇ», three) b
        a.f(one, «twoˇ», three) b
        a.f(one, «twoˇ», three) b
    "});
    cx.update_editor(|editor, window, cx| assert!(editor.move_to_next_snippet_tabstop(window, cx)));
    cx.assert_editor_state(indoc! {"
        a.f(one, two, three)ˇ b
        a.f(one, two, three)ˇ b
        a.f(one, two, three)ˇ b
    "});

    // As soon as the last tab stop is reached, snippet state is gone
    cx.update_editor(|editor, window, cx| {
        assert!(!editor.move_to_prev_snippet_tabstop(window, cx))
    });
    cx.assert_editor_state(indoc! {"
        a.f(one, two, three)ˇ b
        a.f(one, two, three)ˇ b
        a.f(one, two, three)ˇ b
    "});
}

#[gpui::test]
async fn test_snippet_indentation(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.update_editor(|editor, window, cx| {
        let snippet = Snippet::parse(indoc! {"
            /*
             * Multiline comment with leading indentation
             *
             * $1
             */
            $0"})
        .unwrap();
        let insertion_ranges = editor
            .selections
            .all(&editor.display_snapshot(cx))
            .iter()
            .map(|s| s.range())
            .collect::<Vec<_>>();
        editor
            .insert_snippet(&insertion_ranges, snippet, window, cx)
            .unwrap();
    });

    cx.assert_editor_state(indoc! {"
        /*
         * Multiline comment with leading indentation
         *
         * ˇ
         */
    "});

    cx.update_editor(|editor, window, cx| assert!(editor.move_to_next_snippet_tabstop(window, cx)));
    cx.assert_editor_state(indoc! {"
        /*
         * Multiline comment with leading indentation
         *
         *•
         */
        ˇ"});
}

#[gpui::test]
async fn test_snippet_with_multi_word_prefix(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_editor(|editor, _, cx| {
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                let snippet = project::snippet_provider::Snippet {
                    prefix: vec!["multi word".to_string()],
                    body: "this is many words".to_string(),
                    description: Some("description".to_string()),
                    name: "multi-word snippet test".to_string(),
                };
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );
            });
        })
    });

    for (input_to_simulate, should_match_snippet) in [
        ("m", true),
        ("m ", true),
        ("m w", true),
        ("aa m w", true),
        ("aa m g", false),
    ] {
        cx.set_state("ˇ");
        cx.simulate_input(input_to_simulate); // fails correctly

        cx.update_editor(|editor, _, _| {
            let Some(CodeContextMenu::Completions(context_menu)) = &*editor.context_menu.borrow()
            else {
                assert!(!should_match_snippet); // no completions! don't even show the menu
                return;
            };
            assert!(context_menu.visible());
            let completions = context_menu.completions.borrow();

            assert_eq!(!completions.is_empty(), should_match_snippet);
        });
    }
}
