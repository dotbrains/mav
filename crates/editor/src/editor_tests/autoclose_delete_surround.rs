use super::*;

#[gpui::test]
async fn test_autoclose_with_embedded_language(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let html_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "HTML".into(),
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "<".into(),
                            end: ">".into(),
                            close: true,
                            ..Default::default()
                        },
                        BracketPair {
                            start: "{".into(),
                            end: "}".into(),
                            close: true,
                            ..Default::default()
                        },
                        BracketPair {
                            start: "(".into(),
                            end: ")".into(),
                            close: true,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                autoclose_before: "})]>".into(),
                ..Default::default()
            },
            Some(tree_sitter_html::LANGUAGE.into()),
        )
        .with_injection_query(
            r#"
            (script_element
                (raw_text) @injection.content
                (#set! injection.language "javascript"))
            "#,
        )
        .unwrap(),
    );

    let javascript_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "/*".into(),
                        end: " */".into(),
                        close: true,
                        ..Default::default()
                    },
                    BracketPair {
                        start: "{".into(),
                        end: "}".into(),
                        close: true,
                        ..Default::default()
                    },
                    BracketPair {
                        start: "(".into(),
                        end: ")".into(),
                        close: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            autoclose_before: "})]>".into(),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));

    cx.language_registry().add(html_language.clone());
    cx.language_registry().add(javascript_language);
    cx.executor().run_until_parked();

    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(html_language), cx);
    });

    cx.set_state(
        &r#"
            <body>ˇ
                <script>
                    var x = 1;ˇ
                </script>
            </body>ˇ
        "#
        .unindent(),
    );

    // Precondition: different languages are active at different locations.
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let cursors = editor
            .selections
            .ranges::<MultiBufferOffset>(&editor.display_snapshot(cx));
        let languages = cursors
            .iter()
            .map(|c| snapshot.language_at(c.start).unwrap().name())
            .collect::<Vec<_>>();
        assert_eq!(
            languages,
            &[
                LanguageName::from("HTML"),
                LanguageName::from("JavaScript"),
                LanguageName::from("HTML"),
            ]
        );
    });

    // Angle brackets autoclose in HTML, but not JavaScript.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("<", window, cx);
        editor.handle_input("a", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body><aˇ>
                <script>
                    var x = 1;<aˇ
                </script>
            </body><aˇ>
        "#
        .unindent(),
    );

    // Curly braces and parens autoclose in both HTML and JavaScript.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(" b=", window, cx);
        editor.handle_input("{", window, cx);
        editor.handle_input("c", window, cx);
        editor.handle_input("(", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body><a b={c(ˇ)}>
                <script>
                    var x = 1;<a b={c(ˇ)}
                </script>
            </body><a b={c(ˇ)}>
        "#
        .unindent(),
    );

    // Brackets that were already autoclosed are skipped.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(")", window, cx);
        editor.handle_input("d", window, cx);
        editor.handle_input("}", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body><a b={c()d}ˇ>
                <script>
                    var x = 1;<a b={c()d}ˇ
                </script>
            </body><a b={c()d}ˇ>
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(">", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body><a b={c()d}>ˇ
                <script>
                    var x = 1;<a b={c()d}>ˇ
                </script>
            </body><a b={c()d}>ˇ
        "#
        .unindent(),
    );

    // Reset
    cx.set_state(
        &r#"
            <body>ˇ
                <script>
                    var x = 1;ˇ
                </script>
            </body>ˇ
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("<", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body><ˇ>
                <script>
                    var x = 1;<ˇ
                </script>
            </body><ˇ>
        "#
        .unindent(),
    );

    // When backspacing, the closing angle brackets are removed.
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body>ˇ
                <script>
                    var x = 1;ˇ
                </script>
            </body>ˇ
        "#
        .unindent(),
    );

    // Block comments autoclose in JavaScript, but not HTML.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("/", window, cx);
        editor.handle_input("*", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            <body>/*ˇ
                <script>
                    var x = 1;/*ˇ */
                </script>
            </body>/*ˇ
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_autoclose_with_overrides(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let rust_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Rust".into(),
                brackets: serde_json::from_value(json!([
                    { "start": "{", "end": "}", "close": true, "newline": true },
                    { "start": "\"", "end": "\"", "close": true, "newline": false, "not_in": ["string"] },
                ]))
                .unwrap(),
                autoclose_before: "})]>".into(),
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_override_query("(string_literal) @string")
        .unwrap(),
    );

    cx.language_registry().add(rust_language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(rust_language), cx);
    });

    cx.set_state(
        &r#"
            let x = ˇ
        "#
        .unindent(),
    );

    // Inserting a quotation mark. A closing quotation mark is automatically inserted.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            let x = "ˇ"
        "#
        .unindent(),
    );

    // Inserting another quotation mark. The cursor moves across the existing
    // automatically-inserted quotation mark.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            let x = ""ˇ
        "#
        .unindent(),
    );

    // Reset
    cx.set_state(
        &r#"
            let x = ˇ
        "#
        .unindent(),
    );

    // Inserting a quotation mark inside of a string. A second quotation mark is not inserted.
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
        editor.handle_input(" ", window, cx);
        editor.move_left(&Default::default(), window, cx);
        editor.handle_input("\\", window, cx);
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            let x = "\"ˇ "
        "#
        .unindent(),
    );

    // Inserting a closing quotation mark at the position of an automatically-inserted quotation
    // mark. Nothing is inserted.
    cx.update_editor(|editor, window, cx| {
        editor.move_right(&Default::default(), window, cx);
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(
        &r#"
            let x = "\" "ˇ
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_autoclose_quotes_with_scope_awareness(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // Double quote inside single-quoted string
    cx.set_state(indoc! {r#"
        def main():
            items = ['"', ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ['"', "ˇ"]
    "#});

    // Two double quotes inside single-quoted string
    cx.set_state(indoc! {r#"
        def main():
            items = ['""', ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ['""', "ˇ"]
    "#});

    // Single quote inside double-quoted string
    cx.set_state(indoc! {r#"
        def main():
            items = ["'", ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("'", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ["'", 'ˇ']
    "#});

    // Two single quotes inside double-quoted string
    cx.set_state(indoc! {r#"
        def main():
            items = ["''", ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("'", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ["''", 'ˇ']
    "#});

    // Mixed quotes on same line
    cx.set_state(indoc! {r#"
        def main():
            items = ['"""', "'''''", ˇ]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ['"""', "'''''", "ˇ"]
    "#});
    cx.update_editor(|editor, window, cx| {
        editor.move_right(&MoveRight, window, cx);
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(", ", window, cx);
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("'", window, cx);
    });
    cx.assert_editor_state(indoc! {r#"
        def main():
            items = ['"""', "'''''", "", 'ˇ']
    "#});
}
