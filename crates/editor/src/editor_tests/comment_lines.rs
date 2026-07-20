use super::*;

#[gpui::test]
async fn test_toggle_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into(), "//! ".into(), "/// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // If multiple selections intersect a line, the line is only toggled once.
    cx.set_state(indoc! {"
        fn a() {
            «//b();
            ˇ»// «c();
            //ˇ»  d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «b();
            ˇ»«c();
            ˇ» d();
        }
    "});

    // The comment prefix is inserted at the same column for every line in a
    // selection.
    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // «b();
            ˇ»// «c();
            ˇ» // d();
        }
    "});

    // If a selection ends at the beginning of a line, that line is not toggled.
    cx.set_selections_state(indoc! {"
        fn a() {
            // b();
            «// c();
        ˇ»     // d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // b();
            «c();
        ˇ»     // d();
        }
    "});

    // If a selection span a single line and is empty, the line is toggled.
    cx.set_state(indoc! {"
        fn a() {
            a();
            b();
        ˇ
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            a();
            b();
        //•ˇ
        }
    "});

    // If a selection span multiple lines, empty lines are not toggled.
    cx.set_state(indoc! {"
        fn a() {
            «a();

            c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // «a();

            // c();ˇ»
        }
    "});

    // If a selection includes multiple comment prefixes, all lines are uncommented.
    cx.set_state(indoc! {"
        fn a() {
            «// a();
            /// b();
            //! c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «a();
            b();
            c();ˇ»
        }
    "});
}

#[gpui::test]
async fn test_toggle_comment_ignore_indent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into(), "//! ".into(), "/// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    let toggle_comments = &ToggleComments {
        advance_downwards: false,
        ignore_indent: true,
    };

    // If multiple selections intersect a line, the line is only toggled once.
    cx.set_state(indoc! {"
        fn a() {
        //    «b();
        //    c();
        //    ˇ» d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «b();
            c();
            ˇ» d();
        }
    "});

    // The comment prefix is inserted at the beginning of each line
    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    «b();
        //    c();
        //    ˇ» d();
        }
    "});

    // If a selection ends at the beginning of a line, that line is not toggled.
    cx.set_selections_state(indoc! {"
        fn a() {
        //    b();
        //    «c();
        ˇ»//     d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    b();
            «c();
        ˇ»//     d();
        }
    "});

    // If a selection span a single line and is empty, the line is toggled.
    cx.set_state(indoc! {"
        fn a() {
            a();
            b();
        ˇ
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            a();
            b();
        //ˇ
        }
    "});

    // If a selection span multiple lines, empty lines are not toggled.
    cx.set_state(indoc! {"
        fn a() {
            «a();

            c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    «a();

        //    c();ˇ»
        }
    "});

    // If a selection includes multiple comment prefixes, all lines are uncommented.
    cx.set_state(indoc! {"
        fn a() {
        //    «a();
        ///    b();
        //!    c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «a();
            b();
            c();ˇ»
        }
    "});
}

#[gpui::test]
async fn test_advance_downward_on_toggle_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let mut cx = EditorTestContext::new(cx).await;

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    let toggle_comments = &ToggleComments {
        advance_downwards: true,
        ignore_indent: false,
    };

    // Single cursor on one line -> advance
    // Cursor moves horizontally 3 characters as well on non-blank line
    cx.set_state(indoc!(
        "fn a() {
             ˇdog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
             catˇ();
        }"
    ));

    // Single selection on one line -> don't advance
    cx.set_state(indoc!(
        "fn a() {
             «dog()ˇ»;
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // «dog()ˇ»;
             cat();
        }"
    ));

    // Multiple cursors on one line -> advance
    cx.set_state(indoc!(
        "fn a() {
             ˇdˇog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
             catˇ(ˇ);
        }"
    ));

    // Multiple cursors on one line, with selection -> don't advance
    cx.set_state(indoc!(
        "fn a() {
             ˇdˇog«()ˇ»;
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // ˇdˇog«()ˇ»;
             cat();
        }"
    ));

    // Single cursor on one line -> advance
    // Cursor moves to column 0 on blank line
    cx.set_state(indoc!(
        "fn a() {
             ˇdog();

             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
        ˇ
             cat();
        }"
    ));

    // Single cursor on one line -> advance
    // Cursor starts and ends at column 0
    cx.set_state(indoc!(
        "fn a() {
         ˇ    dog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
         ˇ    cat();
        }"
    ));
}
