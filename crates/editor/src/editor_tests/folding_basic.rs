use super::*;
async fn test_fold_action(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
    cx.set_state(indoc! {"
        impl Foo {
            // Hello!

            fn a() {
                1
            }

            fn b() {
                2
            }

            fn c() {
                3
            }
        }ˇ
    "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(7), 0)..DisplayPoint::new(DisplayRow(12), 0)
            ]);
        });
        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {
                    // Hello!

                    fn a() {
                        1
                    }

                    fn b() {⋯}

                    fn c() {⋯}
                }
            "
            .unindent(),
        );

        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {⋯}
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {
                    // Hello!

                    fn a() {
                        1
                    }

                    fn b() {⋯}

                    fn c() {⋯}
                }
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            editor.buffer.read(cx).read(cx).text()
        );
    });
}

#[gpui::test]
fn test_fold_action_without_language(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(
            &"
                impl Foo {
                    // Hello!

                    fn a() {
                        1
                    }

                    fn b() {
                        2
                    }

                    fn c() {
                        3
                    }
                }
            "
            .unindent(),
            cx,
        );
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(7), 0)..DisplayPoint::new(DisplayRow(12), 0)
            ]);
        });
        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {
                    // Hello!

                    fn a() {
                        1
                    }

                    fn b() {⋯
                    }

                    fn c() {⋯
                    }
                }
            "
            .unindent(),
        );

        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {⋯
                }
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                impl Foo {
                    // Hello!

                    fn a() {
                        1
                    }

                    fn b() {⋯
                    }

                    fn c() {⋯
                    }
                }
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            editor.buffer.read(cx).read(cx).text()
        );
    });
}

#[gpui::test]
fn test_fold_action_whitespace_sensitive_language(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(
            &"
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():
                        print(2)

                    def c():
                        print(3)
            "
            .unindent(),
            cx,
        );
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(6), 0)..DisplayPoint::new(DisplayRow(10), 0)
            ]);
        });
        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():⋯

                    def c():⋯
            "
            .unindent(),
        );

        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:⋯
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():⋯

                    def c():⋯
            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            editor.buffer.read(cx).read(cx).text()
        );
    });
}

#[gpui::test]
fn test_fold_action_multiple_line_breaks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(
            &"
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():
                        print(2)


                    def c():
                        print(3)


            "
            .unindent(),
            cx,
        );
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(6), 0)..DisplayPoint::new(DisplayRow(11), 0)
            ]);
        });
        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():⋯


                    def c():⋯


            "
            .unindent(),
        );

        editor.fold(&Fold, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:⋯


            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():⋯


                    def c():⋯


            "
            .unindent(),
        );

        editor.unfold_lines(&UnfoldLines, window, cx);
        assert_eq!(
            editor.display_text(cx),
            editor.buffer.read(cx).read(cx).text()
        );
    });
}

#[gpui::test]
async fn test_fold_with_unindented_multiline_raw_string(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_queries(LanguageQueries {
            overrides: Some(Cow::from(indoc! {"
                [
                  (string_literal)
                  (raw_string_literal)
                ] @string
                [
                  (line_comment)
                  (block_comment)
                ] @comment.inclusive
            "})),
            ..Default::default()
        })
        .expect("Could not parse queries"),
    );

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {"
        fn main() {
            let s = r#\"
        a
        b
        c
        \"#;
        }ˇ
    "});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {"
                fn main() {⋯
                }
            "},
        );
    });
}
