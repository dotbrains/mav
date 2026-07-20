use super::*;

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_single_cursor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Move to end of syntax nodes
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 16)..DisplayPoint::new(DisplayRow(1), 16)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(ˇ1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1ˇ, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2)ˇ;
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);ˇ
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                }ˇ
            "#},
            cx,
        );
    });

    // Test case 2: Move to start of syntax nodes
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 20)..DisplayPoint::new(DisplayRow(1), 20)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2ˇ);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = fooˇ(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = ˇfoo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    ˇlet x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() ˇ{
                    let x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                ˇfn main() {
                    let x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_two_cursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
            let y = bar(3, 4);
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Move to end of syntax nodes with two cursors
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 20)..DisplayPoint::new(DisplayRow(1), 20),
                DisplayPoint::new(DisplayRow(2), 20)..DisplayPoint::new(DisplayRow(2), 20),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2ˇ);
                    let y = bar(3, 4ˇ);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2)ˇ;
                    let y = bar(3, 4)ˇ;
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);ˇ
                    let y = bar(3, 4);ˇ
                }
            "#},
            cx,
        );
    });

    // Test case 2: Move to start of syntax nodes with two cursors
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 19)..DisplayPoint::new(DisplayRow(1), 19),
                DisplayPoint::new(DisplayRow(2), 19)..DisplayPoint::new(DisplayRow(2), 19),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, ˇ2);
                    let y = bar(3, ˇ4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = fooˇ(1, 2);
                    let y = barˇ(3, 4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = ˇfoo(1, 2);
                    let y = ˇbar(3, 4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    ˇlet x = foo(1, 2);
                    ˇlet y = bar(3, 4);
                }
            "#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_with_selections_and_strings(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
            let msg = "hello world";
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: With existing selection, move_to_end keeps selection
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 12)..DisplayPoint::new(DisplayRow(1), 21)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = «foo(1, 2)ˇ»;
                    let msg = "hello world";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = «foo(1, 2)ˇ»;
                    let msg = "hello world";
                }
            "#},
            cx,
        );
    });

    // Test case 2: Move to end within a string
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 15)..DisplayPoint::new(DisplayRow(2), 15)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "ˇhello world";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "hello worldˇ";
                }
            "#},
            cx,
        );
    });

    // Test case 3: Move to start within a string
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 21)..DisplayPoint::new(DisplayRow(2), 21)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "hello ˇworld";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "ˇhello world";
                }
            "#},
            cx,
        );
    });
}
