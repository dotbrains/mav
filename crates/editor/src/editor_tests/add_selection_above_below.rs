use super::*;

#[gpui::test]
async fn test_add_selection_above_below(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc!(
        r#"abc
           defˇghi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abcˇ
           defˇghi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abcˇ
            defˇghi

            jk
            nlmo
            "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.undo_selection(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abcˇ
           defˇghi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.redo_selection(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi
           ˇ
           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi
           ˇ
           jkˇ
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi
           ˇ
           jkˇ
           nlmˇo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           defˇghi
           ˇ
           jkˇ
           nlmˇo
           ˇ"#
    ));

    // change selections
    cx.set_state(indoc!(
        r#"abc
           def«ˇg»hi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           def«ˇg»hi

           jk
           nlm«ˇo»
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           def«ˇg»hi

           jk
           nlm«ˇo»
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           def«ˇg»hi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           def«ˇg»hi

           jk
           nlmo
           "#
    ));

    // Change selections again
    cx.set_state(indoc!(
        r#"a«bc
           defgˇ»hi

           jk
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"a«bcˇ»
           d«efgˇ»hi

           j«kˇ»
           nlmo
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });
    cx.assert_editor_state(indoc!(
        r#"a«bcˇ»
           d«efgˇ»hi

           j«kˇ»
           n«lmoˇ»
           "#
    ));
    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"a«bcˇ»
           d«efgˇ»hi

           j«kˇ»
           nlmo
           "#
    ));

    // Change selections again
    cx.set_state(indoc!(
        r#"abc
           d«ˇefghi

           jk
           nlm»o
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"a«ˇbc»
           d«ˇef»ghi

           j«ˇk»
           n«ˇlm»o
           "#
    ));

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        r#"abc
           d«ˇef»ghi

           j«ˇk»
           n«ˇlm»o
           "#
    ));

    // Assert that the oldest selection's goal column is used when adding more
    // selections, not the most recently added selection's actual column.
    cx.set_state(indoc! {"
        foo bar bazˇ
        foo
        foo bar
    "});

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        foo bar bazˇ
        fooˇ
        foo bar
    "});

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        foo bar bazˇ
        fooˇ
        foo barˇ
    "});

    cx.set_state(indoc! {"
        foo bar baz
        foo
        foo barˇ
    "});

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        foo bar baz
        fooˇ
        foo barˇ
    "});

    cx.update_editor(|editor, window, cx| {
        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        foo barˇ baz
        fooˇ
        foo barˇ
    "});
}
