use super::*;

#[gpui::test]
async fn test_unwrap_syntax_nodes(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(indoc! { r#"use mod1::{mod2::{«mod3ˇ», mod4}, mod5::{mod6, «mod7ˇ»}};"# });
    cx.update_editor(|editor, window, cx| {
        editor.unwrap_syntax_node(&UnwrapSyntaxNode, window, cx);
    });

    cx.assert_editor_state(indoc! { r#"use mod1::{mod2::«mod3ˇ», mod5::«mod7ˇ»};"# });

    cx.set_state(indoc! { r#"fn a() {
          // what
          // a
          // ˇlong
          // method
          // I
          // sure
          // hope
          // it
          // works
    }"# });

    let buffer = cx.update_multibuffer(|multibuffer, _| multibuffer.as_singleton().unwrap());
    let multi_buffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    cx.update(|_, cx| {
        multi_buffer.update(cx, |multi_buffer, cx| {
            multi_buffer.set_excerpts_for_path(
                PathKey::for_buffer(&buffer, cx),
                buffer,
                [Point::new(1, 0)..Point::new(1, 0)],
                3,
                cx,
            );
        });
    });

    let editor2 = cx.new_window_entity(|window, cx| {
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor2, &mut cx).await;
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(3, 0)..Point::new(3, 0)]);
        })
    });

    cx.assert_editor_state(indoc! { "
        fn a() {
              // what
              // a
        ˇ      // long
              // method"});

    cx.update_editor(|editor, window, cx| {
        editor.unwrap_syntax_node(&UnwrapSyntaxNode, window, cx);
    });

    // Although we could potentially make the action work when the syntax node
    // is half-hidden, it seems a bit dangerous as you can't easily tell what it
    // did. Maybe we could also expand the excerpt to contain the range?
    cx.assert_editor_state(indoc! { "
        fn a() {
              // what
              // a
        ˇ      // long
              // method"});
}

#[gpui::test]
async fn test_fold_function_bodies(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let base_text = r#"
        impl A {
            // this is an uncommitted comment

            fn b() {
                c();
            }

            // this is another uncommitted comment

            fn d() {
                // e
                // f
            }
        }

        fn g() {
            // h
        }
    "#
    .unindent();

    let text = r#"
        ˇimpl A {

            fn b() {
                c();
            }

            fn d() {
                // e
                // f
            }
        }

        fn g() {
            // h
        }
    "#
    .unindent();

    let mut cx = EditorLspTestContext::new_rust(Default::default(), cx).await;
    cx.set_state(&text);
    cx.set_head_text(&base_text);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&Default::default(), window, cx);
    });

    cx.assert_state_with_diff(
        "
        ˇimpl A {
      -     // this is an uncommitted comment

            fn b() {
                c();
            }
      -
      -     // this is another uncommitted comment

            fn d() {
                // e
                // f
            }
        }

        fn g() {
            // h
        }
    "
        .unindent(),
    );

    let expected_display_text = "
        impl A {
            // this is an uncommitted comment

            fn b() {
                ⋯
            }

            // this is another uncommitted comment

            fn d() {
                ⋯
            }
        }

        fn g() {
            ⋯
        }
        "
    .unindent();

    cx.update_editor(|editor, window, cx| {
        editor.fold_function_bodies(&FoldFunctionBodies, window, cx);
        assert_eq!(editor.display_text(cx), expected_display_text);
    });
}

#[gpui::test]
async fn test_autoindent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".to_string(),
                            end: "}".to_string(),
                            close: false,
                            surround: false,
                            newline: true,
                        },
                        BracketPair {
                            start: "(".to_string(),
                            end: ")".to_string(),
                            close: false,
                            surround: false,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(
            r#"
                (_ "(" ")" @end) @indent
                (_ "{" "}" @end) @indent
            "#,
        )
        .unwrap(),
    );

    let text = "fn a() {}";

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(5)..MultiBufferOffset(5),
                MultiBufferOffset(8)..MultiBufferOffset(8),
                MultiBufferOffset(9)..MultiBufferOffset(9),
            ])
        });
        editor.newline(&Newline, window, cx);
        assert_eq!(editor.text(cx), "fn a(\n    \n) {\n    \n}\n");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                Point::new(1, 4)..Point::new(1, 4),
                Point::new(3, 4)..Point::new(3, 4),
                Point::new(5, 0)..Point::new(5, 0)
            ]
        );
    });
}

#[gpui::test]
async fn test_autoindent_disabled(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::None)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".to_string(),
                            end: "}".to_string(),
                            close: false,
                            surround: false,
                            newline: true,
                        },
                        BracketPair {
                            start: "(".to_string(),
                            end: ")".to_string(),
                            close: false,
                            surround: false,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(
            r#"
                (_ "(" ")" @end) @indent
                (_ "{" "}" @end) @indent
            "#,
        )
        .unwrap(),
    );

    let text = "fn a() {}";

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(5)..MultiBufferOffset(5),
                MultiBufferOffset(8)..MultiBufferOffset(8),
                MultiBufferOffset(9)..MultiBufferOffset(9),
            ])
        });
        editor.newline(&Newline, window, cx);
        assert_eq!(
            editor.text(cx),
            indoc!(
                "
                fn a(

                ) {

                }
                "
            )
        );
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                Point::new(1, 0)..Point::new(1, 0),
                Point::new(3, 0)..Point::new(3, 0),
                Point::new(5, 0)..Point::new(5, 0)
            ]
        );
    });
}

#[gpui::test]
async fn test_autoindent_none_does_not_preserve_indentation_on_newline(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::None)
    });

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        hello
            indented lineˇ
        world
    "});

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });

    cx.assert_editor_state(indoc! {"
        hello
            indented line
        ˇ
        world
    "});
}

#[gpui::test]
async fn test_autoindent_preserve_indent_maintains_indentation_on_newline(cx: &mut TestAppContext) {
    // When auto_indent is "preserve_indent", pressing Enter on an indented line
    // should preserve the indentation but not adjust based on syntax.
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::PreserveIndent)
    });

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        hello
            indented lineˇ
        world
    "});

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });

    // The new line SHOULD have the same indentation as the previous line
    cx.assert_editor_state(indoc! {"
        hello
            indented line
            ˇ
        world
    "});
}

#[gpui::test]
async fn test_autoindent_preserve_indent_does_not_apply_syntax_indent(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::PreserveIndent)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: false,
                        surround: false,
                        newline: false, // Disable extra newline behavior to isolate syntax indent test
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(r#"(_ "{" "}" @end) @indent"#)
        .unwrap(),
    );

    let buffer =
        cx.new(|cx| Buffer::local("fn foo() {\n}", cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Position cursor at end of line containing `{`
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(10)..MultiBufferOffset(10)]) // After "fn foo() {"
        });
        editor.newline(&Newline, window, cx);

        // With PreserveIndent, the new line should have 0 indentation (same as the fn line)
        // NOT 4 spaces (which tree-sitter would add for being inside `{}`)
        assert_eq!(editor.text(cx), "fn foo() {\n\n}");
    });
}
