use super::*;

#[gpui::test]
async fn test_fold_with_unindented_multiline_raw_string_includes_closing_bracket(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
    cx.set_state(indoc! {"
        ˇfn main() {
            let s = r#\"
        a
        b
        c
        \"#;
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {"
                fn main() {⋯}
            "},
        );
    });
}

#[gpui::test]
async fn test_fold_with_unindented_multiline_block_comment(cx: &mut TestAppContext) {
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
            let x = 1;
            /*
        unindented comment line
            */
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

#[gpui::test]
async fn test_fold_with_unindented_multiline_block_comment_includes_closing_bracket(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
    cx.set_state(indoc! {"
        ˇfn main() {
            let x = 1;
            /*
        unindented comment line
            */
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {"
                fn main() {⋯}
            "},
        );
    });
}

#[gpui::test]
async fn test_fold_preserves_top_level_comments_between_python_classes(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_python::LANGUAGE.into()),
        )
        .with_queries(LanguageQueries {
            overrides: Some(Cow::from(indoc! {"
                (comment) @comment.inclusive
                (string) @string
            "})),
            ..Default::default()
        })
        .expect("Could not parse queries"),
    );

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {"
        class Foo:
            def bar(self):
                pass


        # SECTION SEPARATOR

        class Baz:
            def qux(self):
                passˇ
    "});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {"
                class Foo:⋯


                # SECTION SEPARATOR

                class Baz:
                    def qux(self):
                        pass
            "},
        );
    });
}

#[gpui::test]
async fn test_fold_preserves_top_level_comments_between_rust_functions(cx: &mut TestAppContext) {
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
        fn foo() {
            bar();
        }


        // SECTION SEPARATOR


        fn baz() {
            qux();ˇ
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {"
                fn foo() {⋯
                }


                // SECTION SEPARATOR


                fn baz() {
                    qux();
                }
            "},
        );
    });
}

#[gpui::test]
async fn test_fold_terminates_at_top_level_multiline_string_between_python_classes(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_python::LANGUAGE.into()),
        )
        .with_queries(LanguageQueries {
            overrides: Some(Cow::from(indoc! {"
                (comment) @comment.inclusive
                (string) @string
            "})),
            ..Default::default()
        })
        .expect("Could not parse queries"),
    );

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {r#"
        class Foo:
            def bar(self):
                pass


        """
        top-level docstring at zero indent
        """


        class Baz:
            def qux(self):
                passˇ
    "#});

    cx.update_editor(|editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            indoc! {r#"
                class Foo:⋯


                """
                top-level docstring at zero indent
                """


                class Baz:
                    def qux(self):
                        pass
            "#},
        );
    });
}

#[gpui::test]
fn test_fold_at_level(cx: &mut TestAppContext) {
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


                class Bar:
                    # World!

                    def a():
                        print(1)

                    def b():
                        print(2)


            "
            .unindent(),
            cx,
        );
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.fold_at_level(&FoldAtLevel(2), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():⋯

                    def b():⋯


                class Bar:
                    # World!

                    def a():⋯

                    def b():⋯


            "
            .unindent(),
        );

        editor.fold_at_level(&FoldAtLevel(1), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:⋯


                class Bar:⋯


            "
            .unindent(),
        );

        editor.unfold_all(&UnfoldAll, window, cx);
        editor.fold_at_level(&FoldAtLevel(0), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():
                        print(1)

                    def b():
                        print(2)


                class Bar:
                    # World!

                    def a():
                        print(1)

                    def b():
                        print(2)


            "
            .unindent(),
        );

        assert_eq!(
            editor.display_text(cx),
            editor.buffer.read(cx).read(cx).text()
        );
        let (_, positions) = marked_text_ranges(
            &"
                       class Foo:
                           # Hello!

                           def a():
                              print(1)

                           def b():
                               p«riˇ»nt(2)


                       class Bar:
                           # World!

                           def a():
                               «ˇprint(1)

                           def b():
                               print(2)»


                   "
            .unindent(),
            true,
        );

        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges(
                positions
                    .iter()
                    .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end)),
            )
        });

        editor.fold_at_level(&FoldAtLevel(2), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
                class Foo:
                    # Hello!

                    def a():⋯

                    def b():
                        print(2)


                class Bar:
                    # World!

                    def a():
                        print(1)

                    def b():
                        print(2)


            "
            .unindent(),
        );
    });
}
