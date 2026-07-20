use super::*;

#[gpui::test]
async fn test_add_selection_above_below_multi_cursor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc!(
        r#"line onˇe
           liˇne two
           line three
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // test multiple cursors expand in the same direction
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           liˇne three
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // test multiple cursors expand below overflow
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           liˇne thˇree
           liˇne foˇur"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test multiple cursors retrieves back correctly
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           liˇne thˇree
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test multiple cursor groups maintain independent direction - first expands up, second shrinks above
    cx.assert_editor_state(indoc!(
        r#"liˇne onˇe
           liˇne two
           line three
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.undo_selection(&Default::default(), window, cx);
    });

    // test undo
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           line three
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.redo_selection(&Default::default(), window, cx);
    });

    // test redo
    cx.assert_editor_state(indoc!(
        r#"liˇne onˇe
           liˇne two
           line three
           line four"#
    ));

    cx.set_state(indoc!(
        r#"abcd
           ef«ghˇ»
           ijkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test multiple selections expand in the same direction
    cx.assert_editor_state(indoc!(
        r#"ab«cdˇ»
           ef«ghˇ»
           «iˇ»jkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test multiple selection upward overflow
    cx.assert_editor_state(indoc!(
        r#"ab«cdˇ»
           «eˇ»f«ghˇ»
           «iˇ»jkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // test multiple selection retrieves back correctly
    cx.assert_editor_state(indoc!(
        r#"abcd
           ef«ghˇ»
           «iˇ»jkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // test multiple cursor groups maintain independent direction - first shrinks down, second expands below
    cx.assert_editor_state(indoc!(
        r#"abcd
           ef«ghˇ»
           ij«klˇ»
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.undo_selection(&Default::default(), window, cx);
    });

    // test undo
    cx.assert_editor_state(indoc!(
        r#"abcd
           ef«ghˇ»
           «iˇ»jkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.redo_selection(&Default::default(), window, cx);
    });

    // test redo
    cx.assert_editor_state(indoc!(
        r#"abcd
           ef«ghˇ»
           ij«klˇ»
           «mˇ»nop"#
    ));
}

#[gpui::test]
async fn test_add_selection_above_below_multi_cursor_existing_state(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc!(
        r#"line onˇe
           liˇne two
           line three
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
        editor.add_selection_below(&Default::default(), window, cx);
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // initial state with two multi cursor groups
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           liˇne thˇree
           liˇne foˇur"#
    ));

    // add single cursor in middle - simulate opt click
    cx.update_editor(|editor, window, cx| {
        let new_cursor_point = DisplayPoint::new(DisplayRow(2), 4);
        editor.begin_selection(new_cursor_point, true, 1, window, cx);
        editor.end_selection(window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇne twˇo
           liˇneˇ thˇree
           liˇne foˇur"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test new added selection expands above and existing selection shrinks
    cx.assert_editor_state(indoc!(
        r#"line onˇe
           liˇneˇ twˇo
           liˇneˇ thˇree
           line four"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    // test new added selection expands above and existing selection shrinks
    cx.assert_editor_state(indoc!(
        r#"lineˇ onˇe
           liˇneˇ twˇo
           lineˇ three
           line four"#
    ));

    // intial state with two selection groups
    cx.set_state(indoc!(
        r#"abcd
           ef«ghˇ»
           ijkl
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"ab«cdˇ»
           «eˇ»f«ghˇ»
           «iˇ»jkl
           «mˇ»nop"#
    ));

    // add single selection in middle - simulate opt drag
    cx.update_editor(|editor, window, cx| {
        let new_cursor_point = DisplayPoint::new(DisplayRow(2), 3);
        editor.begin_selection(new_cursor_point, true, 1, window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(2), 4),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        editor.end_selection(window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"ab«cdˇ»
           «eˇ»f«ghˇ»
           «iˇ»jk«lˇ»
           «mˇ»nop"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // test new added selection expands below, others shrinks from above
    cx.assert_editor_state(indoc!(
        r#"abcd
           ef«ghˇ»
           «iˇ»jk«lˇ»
           «mˇ»no«pˇ»"#
    ));
}

#[gpui::test]
async fn test_add_selection_above_below_multibyte(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Cursor after "Häl" (byte column 4, char column 3) should align to
    // char column 3 on the ASCII line below, not byte column 4.
    cx.set_state(indoc!(
        r#"Hälˇlö
           Hallo"#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"Hälˇlö
           Halˇlo"#
    ));
}

#[gpui::test]
async fn test_select_next(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Enable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(true);
        settings.search = Some(search_settings);
    });

    cx.set_state("abc\nˇabc abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«abcˇ» «abcˇ»\ndefabc\nabc");

    cx.update_editor(|editor, window, cx| editor.undo_selection(&UndoSelection, window, cx));
    cx.assert_editor_state("abc\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|editor, window, cx| editor.redo_selection(&RedoSelection, window, cx));
    cx.assert_editor_state("abc\n«abcˇ» «abcˇ»\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«abcˇ» «abcˇ»\ndefabc\n«abcˇ»");

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ» «abcˇ»\ndefabc\n«abcˇ»");

    // Test selection direction should be preserved
    cx.set_state("abc\n«ˇabc» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«ˇabc» «ˇabc»\ndefabc\nabc");

    // Test case sensitivity
    cx.set_state("«ˇfoo»\nFOO\nFoo\nfoo");
    cx.update_editor(|e, window, cx| {
        e.select_next(&SelectNext::default(), window, cx).unwrap();
    });
    cx.assert_editor_state("«ˇfoo»\nFOO\nFoo\n«ˇfoo»");

    // Disable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(false);
        settings.search = Some(search_settings);
    });

    cx.set_state("«ˇfoo»\nFOO\nFoo");
    cx.update_editor(|e, window, cx| {
        e.select_next(&SelectNext::default(), window, cx).unwrap();
        e.select_next(&SelectNext::default(), window, cx).unwrap();
    });
    cx.assert_editor_state("«ˇfoo»\n«ˇFOO»\n«ˇFoo»");
}

#[gpui::test]
async fn test_select_all_matches(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Enable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(true);
        settings.search = Some(search_settings);
    });

    // Test caret-only selections
    cx.set_state("abc\nˇabc abc\ndefabc\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ» «abcˇ»\ndefabc\n«abcˇ»");

    // Test left-to-right selections
    cx.set_state("abc\n«abcˇ»\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ»\n«abcˇ»");

    // Test right-to-left selections
    cx.set_state("abc\n«ˇabc»\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("«ˇabc»\n«ˇabc»\n«ˇabc»");

    // Test selecting whitespace with caret selection
    cx.set_state("abc\nˇ   abc\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«   ˇ»abc\nabc");

    // Test selecting whitespace with left-to-right selection
    cx.set_state("abc\n«ˇ  »abc\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«ˇ  »abc\nabc");

    // Test no matches with right-to-left selection
    cx.set_state("abc\n«  ˇ»abc\nabc");
    cx.update_editor(|e, window, cx| e.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«  ˇ»abc\nabc");

    // Test with a single word and clip_at_line_ends=true (#29823)
    cx.set_state("aˇbc");
    cx.update_editor(|e, window, cx| {
        e.set_clip_at_line_ends(true, cx);
        e.select_all_matches(&SelectAllMatches, window, cx).unwrap();
        e.set_clip_at_line_ends(false, cx);
    });
    cx.assert_editor_state("«abcˇ»");

    // Test case sensitivity
    cx.set_state("fˇoo\nFOO\nFoo");
    cx.update_editor(|e, window, cx| {
        e.select_all_matches(&SelectAllMatches, window, cx).unwrap();
    });
    cx.assert_editor_state("«fooˇ»\nFOO\nFoo");

    // Disable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(false);
        settings.search = Some(search_settings);
    });

    cx.set_state("fˇoo\nFOO\nFoo");
    cx.update_editor(|e, window, cx| {
        e.select_all_matches(&SelectAllMatches, window, cx).unwrap();
    });
    cx.assert_editor_state("«fooˇ»\n«FOOˇ»\n«Fooˇ»");
}
