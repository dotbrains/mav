use super::*;

#[gpui::test]
fn test_autoindent_multi_line_insertion(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = "
            const a: usize = 1;
            fn b() {
                if c {
                    let d = 2;
                }
            }
        "
        .unindent();

        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);
        buffer.edit(
            [(Point::new(3, 0)..Point::new(3, 0), "e(\n    f()\n);\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
                const a: usize = 1;
                fn b() {
                    if c {
                        e(
                            f()
                        );
                        let d = 2;
                    }
                }
            "
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_block_mode(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            fn a() {
                b();
            }
        "#
        .unindent();
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        // When this text was copied, both of the quotation marks were at the same
        // indent level, but the indentation of the first line was not included in
        // the copied text. This information is retained in the
        // 'original_indent_columns' vector.
        let original_indent_columns = vec![Some(4)];
        let inserted_text = r#"
            "
                  c
                    d
                      e
                "
        "#
        .unindent();

        // Insert the block at column zero. The entire block is indented
        // so that the first line matches the previous line's indentation.
        buffer.edit(
            [(Point::new(2, 0)..Point::new(2, 0), inserted_text.clone())],
            Some(AutoindentMode::Block {
                original_indent_columns: original_indent_columns.clone(),
            }),
            cx,
        );
        assert_eq!(
            buffer.text(),
            r#"
            fn a() {
                b();
                "
                  c
                    d
                      e
                "
            }
            "#
            .unindent()
        );

        // Grouping is disabled in tests, so we need 2 undos
        buffer.undo(cx); // Undo the auto-indent
        buffer.undo(cx); // Undo the original edit

        // Insert the block at a deeper indent level. The entire block is outdented.
        buffer.edit([(Point::new(2, 0)..Point::new(2, 0), "        ")], None, cx);
        buffer.edit(
            [(Point::new(2, 8)..Point::new(2, 8), inserted_text)],
            Some(AutoindentMode::Block {
                original_indent_columns,
            }),
            cx,
        );
        assert_eq!(
            buffer.text(),
            r#"
            fn a() {
                b();
                "
                  c
                    d
                      e
                "
            }
            "#
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_block_mode_with_newline(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            fn a() {
                b();
            }
        "#
        .unindent();
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        // First line contains just '\n', it's indentation is stored in "original_indent_columns"
        let original_indent_columns = vec![Some(4)];
        let inserted_text = r#"

                c();
                    d();
                        e();
        "#
        .unindent();
        buffer.edit(
            [(Point::new(2, 0)..Point::new(2, 0), inserted_text)],
            Some(AutoindentMode::Block {
                original_indent_columns,
            }),
            cx,
        );

        // While making edit, we ignore first line as it only contains '\n'
        // hence second line indent is used to calculate delta
        assert_eq!(
            buffer.text(),
            r#"
            fn a() {
                b();

                c();
                    d();
                        e();
            }
            "#
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_block_mode_without_original_indent_columns(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = r#"
            fn a() {
                if b() {

                }
            }
        "#
        .unindent();
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        // The original indent columns are not known, so this text is
        // auto-indented in a block as if the first line was copied in
        // its entirety.
        let original_indent_columns = Vec::new();
        let inserted_text = "    c\n        .d()\n        .e();";

        // Insert the block at column zero. The entire block is indented
        // so that the first line matches the previous line's indentation.
        buffer.edit(
            [(Point::new(2, 0)..Point::new(2, 0), inserted_text)],
            Some(AutoindentMode::Block {
                original_indent_columns,
            }),
            cx,
        );
        assert_eq!(
            buffer.text(),
            r#"
            fn a() {
                if b() {
                    c
                        .d()
                        .e();
                }
            }
            "#
            .unindent()
        );

        // Grouping is disabled in tests, so we need 2 undos
        buffer.undo(cx); // Undo the auto-indent
        buffer.undo(cx); // Undo the original edit

        // Insert the block at a deeper indent level. The entire block is outdented.
        buffer.edit(
            [(Point::new(2, 0)..Point::new(2, 0), " ".repeat(12))],
            None,
            cx,
        );
        buffer.edit(
            [(Point::new(2, 12)..Point::new(2, 12), inserted_text)],
            Some(AutoindentMode::Block {
                original_indent_columns: Vec::new(),
            }),
            cx,
        );
        assert_eq!(
            buffer.text(),
            r#"
            fn a() {
                if b() {
                    c
                        .d()
                        .e();
                }
            }
            "#
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_block_mode_multiple_adjacent_ranges(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let (text, ranges_to_replace) = marked_text_ranges(
            &"
            mod numbers {
                «fn one() {
                    1
                }
            »
                «fn two() {
                    2
                }
            »
                «fn three() {
                    3
                }
            »}
            "
            .unindent(),
            false,
        );

        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        buffer.edit(
            [
                (ranges_to_replace[0].clone(), "fn one() {\n    101\n}\n"),
                (ranges_to_replace[1].clone(), "fn two() {\n    102\n}\n"),
                (ranges_to_replace[2].clone(), "fn three() {\n    103\n}\n"),
            ],
            Some(AutoindentMode::Block {
                original_indent_columns: vec![Some(0), Some(0), Some(0)],
            }),
            cx,
        );

        assert_eq!(
            buffer.text(),
            "
            mod numbers {
                fn one() {
                    101
                }

                fn two() {
                    102
                }

                fn three() {
                    103
                }
            }
            "
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_language_without_indents_query(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = "
            * one
                - a
                - b
            * two
        "
        .unindent();

        let mut buffer = Buffer::local(text, cx).with_language(
            Arc::new(Language::new(
                LanguageConfig {
                    name: "Markdown".into(),
                    auto_indent_using_last_non_empty_line: false,
                    ..Default::default()
                },
                Some(tree_sitter_json::LANGUAGE.into()),
            )),
            cx,
        );
        buffer.edit(
            [(Point::new(3, 0)..Point::new(3, 0), "\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            * one
                - a
                - b

            * two
            "
            .unindent()
        );
        buffer
    });
}
