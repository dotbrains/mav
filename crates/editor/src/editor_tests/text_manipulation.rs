use super::*;

#[gpui::test]
async fn test_custom_newlines_cause_no_false_positive_diffs(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state("Line 0\r\nLine 1\rˇ\nLine 2\r\nLine 3");
    cx.set_head_text("Line 0\r\nLine 1\r\nLine 2\r\nLine 3");
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        assert_eq!(
            snapshot
                .buffer_snapshot()
                .diff_hunks_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
                .collect::<Vec<_>>(),
            Vec::new(),
            "Should not have any diffs for files with custom newlines"
        );
    });
}

#[gpui::test]
async fn test_manipulate_immutable_lines_with_single_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Test sort_lines_case_insensitive()
    cx.set_state(indoc! {"
        «z
        y
        x
        Z
        Y
        Xˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_insensitive(&SortLinesCaseInsensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «x
        X
        y
        Y
        z
        Zˇ»
    "});

    // Test sort_lines_by_length()
    //
    // Demonstrates:
    // - ∞ is 3 bytes UTF-8, but sorted by its char count (1)
    // - sort is stable
    cx.set_state(indoc! {"
        «123
        æ
        12
        ∞
        1
        æˇ»
    "});
    cx.update_editor(|e, window, cx| e.sort_lines_by_length(&SortLinesByLength, window, cx));
    cx.assert_editor_state(indoc! {"
        «æ
        ∞
        1
        æ
        12
        123ˇ»
    "});

    // Test reverse_lines()
    cx.set_state(indoc! {"
        «5
        4
        3
        2
        1ˇ»
    "});
    cx.update_editor(|e, window, cx| e.reverse_lines(&ReverseLines, window, cx));
    cx.assert_editor_state(indoc! {"
        «1
        2
        3
        4
        5ˇ»
    "});

    // Skip testing shuffle_line()

    // From here on out, test more complex cases of manipulate_immutable_lines() with a single driver method: sort_lines_case_sensitive()
    // Since all methods calling manipulate_immutable_lines() are doing the exact same general thing (reordering lines)

    // Don't manipulate when cursor is on single line, but expand the selection
    cx.set_state(indoc! {"
        ddˇdd
        ccc
        bb
        a
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_sensitive(&SortLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «ddddˇ»
        ccc
        bb
        a
    "});

    // Basic manipulate case
    // Start selection moves to column 0
    // End of selection shrinks to fit shorter line
    cx.set_state(indoc! {"
        dd«d
        ccc
        bb
        aaaaaˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_sensitive(&SortLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «aaaaa
        bb
        ccc
        dddˇ»
    "});

    // Manipulate case with newlines
    cx.set_state(indoc! {"
        dd«d
        ccc

        bb
        aaaaa

        ˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.sort_lines_case_sensitive(&SortLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «

        aaaaa
        bb
        ccc
        dddˇ»

    "});

    // Adding new line
    cx.set_state(indoc! {"
        aa«a
        bbˇ»b
    "});
    cx.update_editor(|e, window, cx| {
        e.manipulate_immutable_lines(window, cx, |lines| lines.push("added_line"))
    });
    cx.assert_editor_state(indoc! {"
        «aaa
        bbb
        added_lineˇ»
    "});

    // Removing line
    cx.set_state(indoc! {"
        aa«a
        bbbˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.manipulate_immutable_lines(window, cx, |lines| {
            lines.pop();
        })
    });
    cx.assert_editor_state(indoc! {"
        «aaaˇ»
    "});

    // Removing all lines
    cx.set_state(indoc! {"
        aa«a
        bbbˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.manipulate_immutable_lines(window, cx, |lines| {
            lines.drain(..);
        })
    });
    cx.assert_editor_state(indoc! {"
        ˇ
    "});
}

#[gpui::test]
async fn test_unique_lines_multi_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Consider continuous selection as single selection
    cx.set_state(indoc! {"
        Aaa«aa
        cˇ»c«c
        bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.unique_lines_case_sensitive(&UniqueLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «Aaaaa
        ccc
        bb
        aaaaaˇ»
    "});

    cx.set_state(indoc! {"
        Aaa«aa
        cˇ»c«c
        bb
        aaaˇ»aa
    "});
    cx.update_editor(|e, window, cx| {
        e.unique_lines_case_insensitive(&UniqueLinesCaseInsensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «Aaaaa
        ccc
        bbˇ»
    "});

    // Consider non continuous selection as distinct dedup operations
    cx.set_state(indoc! {"
        «aaaaa
        bb
        aaaaa
        aaaaaˇ»

        aaa«aaˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.unique_lines_case_sensitive(&UniqueLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «aaaaa
        bbˇ»

        «aaaaaˇ»
    "});
}

#[gpui::test]
async fn test_unique_lines_single_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        «Aaa
        aAa
        Aaaˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.unique_lines_case_sensitive(&UniqueLinesCaseSensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «Aaa
        aAaˇ»
    "});

    cx.set_state(indoc! {"
        «Aaa
        aAa
        aaAˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.unique_lines_case_insensitive(&UniqueLinesCaseInsensitive, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        «Aaaˇ»
    "});
}

#[gpui::test]
async fn test_wrap_in_tag_single_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let js_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            wrap_characters: Some(language::WrapCharactersConfig {
                start_prefix: "<".into(),
                start_suffix: ">".into(),
                end_prefix: "</".into(),
                end_suffix: ">".into(),
            }),
            ..LanguageConfig::default()
        },
        None,
    ));

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(js_language), cx));

    cx.set_state(indoc! {"
        «testˇ»
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
        <«ˇ»>test</«ˇ»>
    "});

    cx.set_state(indoc! {"
        «test
         testˇ»
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
        <«ˇ»>test
         test</«ˇ»>
    "});

    cx.set_state(indoc! {"
        teˇst
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
        te<«ˇ»></«ˇ»>st
    "});
}

#[gpui::test]
async fn test_wrap_in_tag_multi_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let js_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            wrap_characters: Some(language::WrapCharactersConfig {
                start_prefix: "<".into(),
                start_suffix: ">".into(),
                end_prefix: "</".into(),
                end_suffix: ">".into(),
            }),
            ..LanguageConfig::default()
        },
        None,
    ));

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(js_language), cx));

    cx.set_state(indoc! {"
        «testˇ»
        «testˇ» «testˇ»
        «testˇ»
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
        <«ˇ»>test</«ˇ»>
        <«ˇ»>test</«ˇ»> <«ˇ»>test</«ˇ»>
        <«ˇ»>test</«ˇ»>
    "});

    cx.set_state(indoc! {"
        «test
         testˇ»
        «test
         testˇ»
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
        <«ˇ»>test
         test</«ˇ»>
        <«ˇ»>test
         test</«ˇ»>
    "});
}

#[gpui::test]
async fn test_wrap_in_tag_does_nothing_in_unsupported_languages(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let plaintext_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Plain Text".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(plaintext_language), cx));

    cx.set_state(indoc! {"
        «testˇ»
    "});
    cx.update_editor(|e, window, cx| e.wrap_selections_in_tag(&WrapSelectionsInTag, window, cx));
    cx.assert_editor_state(indoc! {"
      «testˇ»
    "});
}
