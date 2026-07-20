use super::*;

#[gpui::test]
async fn test_select_larger_syntax_node_for_cursor_at_end(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = "let a = 2;";

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Cursor at end of word
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 5)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, "let aˇ = 2;", cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, "let «ˇa» = 2;", cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, "«ˇlet a = 2;»", cx);
    });

    // Test case 2: Cursor at end of statement
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 11)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, "let a = 2;ˇ", cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, "«ˇlet a = 2;»", cx);
    });
}

#[gpui::test]
async fn test_select_larger_syntax_node_for_cursor_at_symbol(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));

    let text = r#"
        let a = {
            key: "value",
        };
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Cursor after '{'
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 9)..DisplayPoint::new(DisplayRow(0), 9)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = {ˇ
                    key: "value",
                };
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = «ˇ{
                    key: "value",
                }»;
            "#},
            cx,
        );
    });

    // Test case 2: Cursor after ':'
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 8)..DisplayPoint::new(DisplayRow(1), 8)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = {
                    key:ˇ "value",
                };
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = {
                    «ˇkey: "value"»,
                };
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = «ˇ{
                    key: "value",
                }»;
            "#},
            cx,
        );
    });

    // Test case 3: Cursor after ','
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 17)..DisplayPoint::new(DisplayRow(1), 17)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = {
                    key: "value",ˇ
                };
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = «ˇ{
                    key: "value",
                }»;
            "#},
            cx,
        );
    });

    // Test case 4: Cursor after ';'
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let a = {
                    key: "value",
                };ˇ
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                «ˇlet a = {
                    key: "value",
                };
                »"#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_select_larger_smaller_syntax_node_for_string(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        use mod1::mod2::{mod3, mod4};

        fn fn_1(param1: bool, param2: &str) {
            let var1 = "hello world";
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test 1: Cursor on a letter of a string word
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 17)..DisplayPoint::new(DisplayRow(3), 17)
            ]);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "hˇello world";
                }
            "#},
            cx,
        );
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«ˇhello» world";
                }
            "#},
            cx,
        );
    });

    // Test 2: Partial selection within a word
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 17)..DisplayPoint::new(DisplayRow(3), 19)
            ]);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "h«elˇ»lo world";
                }
            "#},
            cx,
        );
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«ˇhello» world";
                }
            "#},
            cx,
        );
    });

    // Test 3: Complete word already selected
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 16)..DisplayPoint::new(DisplayRow(3), 21)
            ]);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«helloˇ» world";
                }
            "#},
            cx,
        );
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«hello worldˇ»";
                }
            "#},
            cx,
        );
    });

    // Test 4: Selection spanning across words
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 19)..DisplayPoint::new(DisplayRow(3), 24)
            ]);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "hel«lo woˇ»rld";
                }
            "#},
            cx,
        );
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    let var1 = "«ˇhello world»";
                }
            "#},
            cx,
        );
    });

    // Test 5: Expansion beyond string
    editor.update_in(cx, |editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
        assert_text_with_selections(
            editor,
            indoc! {r#"
                use mod1::mod2::{mod3, mod4};

                fn fn_1(param1: bool, param2: &str) {
                    «ˇlet var1 = "hello world";»
                }
            "#},
            cx,
        );
    });
}
