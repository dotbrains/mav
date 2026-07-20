use super::*;

#[gpui::test]
async fn test_autoindent_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    {
        let mut cx = EditorLspTestContext::new_rust(Default::default(), cx).await;
        cx.set_state(indoc! {"
            impl A {

                fn b() {}

            «fn c() {

            }ˇ»
            }
        "});

        cx.update_editor(|editor, window, cx| {
            editor.autoindent(&Default::default(), window, cx);
        });
        cx.wait_for_autoindent_applied().await;

        cx.assert_editor_state(indoc! {"
            impl A {

                fn b() {}

                «fn c() {

                }ˇ»
            }
        "});
    }

    {
        let mut cx = EditorTestContext::new_multibuffer(
            cx,
            [indoc! { "
                impl A {
                «
                // a
                fn b(){}
                »
                «
                    }
                    fn c(){}
                »
            "}],
        );

        let buffer = cx.update_editor(|editor, _, cx| {
            let buffer = editor.buffer().update(cx, |buffer, _| {
                buffer.all_buffers().iter().next().unwrap().clone()
            });
            buffer.update(cx, |buffer, cx| buffer.set_language(Some(rust_lang()), cx));
            buffer
        });

        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            editor.select_all(&Default::default(), window, cx);
            editor.autoindent(&Default::default(), window, cx)
        });
        cx.run_until_parked();

        cx.update(|_, cx| {
            assert_eq!(
                buffer.read(cx).text(),
                indoc! { "
                    impl A {

                        // a
                        fn b(){}


                    }
                    fn c(){}

                " }
            )
        });
    }
}

#[gpui::test]
async fn test_autoclose_and_auto_surround_pairs(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(Language::new(
        LanguageConfig {
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "/*".to_string(),
                        end: " */".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: false,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "\"".to_string(),
                        end: "\"".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "<".to_string(),
                        end: ">".to_string(),
                        close: false,
                        surround: true,
                        newline: true,
                    },
                ],
                ..Default::default()
            },
            autoclose_before: "})]".to_string(),
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(
        &r#"
            🏀ˇ
            εˇ
            ❤️ˇ
        "#
        .unindent(),
    );

    // autoclose multiple nested brackets at multiple cursors
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
        editor.handle_input("{", window, cx);
    });
    cx.assert_editor_state(
        &"
            🏀{{{ˇ}}}
            ε{{{ˇ}}}
            ❤️{{{ˇ}}}
        "
        .unindent(),
    );

    // insert a different closing bracket
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(")", window, cx);
    });
    cx.assert_editor_state(
        &"
            🏀{{{)ˇ}}}
            ε{{{)ˇ}}}
            ❤️{{{)ˇ}}}
        "
        .unindent(),
    );

    // skip over the auto-closed brackets when typing a closing bracket
    cx.update_editor(|editor, window, cx| {
        editor.move_right(&MoveRight, window, cx);
        editor.handle_input("}", window, cx);
        editor.handle_input("}", window, cx);
        editor.handle_input("}", window, cx);
    });
    cx.assert_editor_state(
        &"
            🏀{{{)}}}}ˇ
            ε{{{)}}}}ˇ
            ❤️{{{)}}}}ˇ
        "
        .unindent(),
    );

    // autoclose multi-character pairs
    cx.set_state(
        &"
            ˇ
            ˇ
        "
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("/", window, cx);
        editor.handle_input("*", window, cx);
    });
    cx.assert_editor_state(
        &"
            /*ˇ */
            /*ˇ */
        "
        .unindent(),
    );

    // one cursor autocloses a multi-character pair, one cursor
    // does not autoclose.
    cx.set_state(
        &"
            /ˇ
            ˇ
        "
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| editor.handle_input("*", window, cx));
    cx.assert_editor_state(
        &"
            /*ˇ */
            *ˇ
        "
        .unindent(),
    );

    // Don't autoclose if the next character isn't whitespace and isn't
    // listed in the language's "autoclose_before" section.
    cx.set_state("ˇa b");
    cx.update_editor(|editor, window, cx| editor.handle_input("{", window, cx));
    cx.assert_editor_state("{ˇa b");

    // Don't autoclose if `close` is false for the bracket pair
    cx.set_state("ˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("[", window, cx));
    cx.assert_editor_state("[ˇ");

    // Surround with brackets if text is selected
    cx.set_state("«aˇ» b");
    cx.update_editor(|editor, window, cx| editor.handle_input("{", window, cx));
    cx.assert_editor_state("{«aˇ»} b");

    // Autoclose when not immediately after a word character
    cx.set_state("a ˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("\"", window, cx));
    cx.assert_editor_state("a \"ˇ\"");

    // Autoclose pair where the start and end characters are the same
    cx.update_editor(|editor, window, cx| editor.handle_input("\"", window, cx));
    cx.assert_editor_state("a \"\"ˇ");

    // Don't autoclose when immediately after a word character
    cx.set_state("aˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("\"", window, cx));
    cx.assert_editor_state("a\"ˇ");

    // Do autoclose when after a non-word character
    cx.set_state("{ˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("\"", window, cx));
    cx.assert_editor_state("{\"ˇ\"");

    // Non identical pairs autoclose regardless of preceding character
    cx.set_state("aˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("{", window, cx));
    cx.assert_editor_state("a{ˇ}");

    // Don't autoclose pair if autoclose is disabled
    cx.set_state("ˇ");
    cx.update_editor(|editor, window, cx| editor.handle_input("<", window, cx));
    cx.assert_editor_state("<ˇ");

    // Surround with brackets if text is selected and auto_surround is enabled, even if autoclose is disabled
    cx.set_state("«aˇ» b");
    cx.update_editor(|editor, window, cx| editor.handle_input("<", window, cx));
    cx.assert_editor_state("<«aˇ»> b");
}

#[gpui::test]
async fn test_always_treat_brackets_as_autoclosed_skip_over(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.always_treat_brackets_as_autoclosed = Some(true);
    });

    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(Language::new(
        LanguageConfig {
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: false,
                        surround: false,
                        newline: true,
                    },
                ],
                ..Default::default()
            },
            autoclose_before: "})]".to_string(),
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(
        &"
            ˇ
            ˇ
            ˇ
        "
        .unindent(),
    );

    // ensure only matching closing brackets are skipped over
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("}", window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.handle_input(")", window, cx);
        editor.move_left(&MoveLeft, window, cx);
    });
    cx.assert_editor_state(
        &"
            ˇ)}
            ˇ)}
            ˇ)}
        "
        .unindent(),
    );

    // skip-over closing brackets at multiple cursors
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(")", window, cx);
        editor.handle_input("}", window, cx);
    });
    cx.assert_editor_state(
        &"
            )}ˇ
            )}ˇ
            )}ˇ
        "
        .unindent(),
    );

    // ignore non-close brackets
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("]", window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.handle_input("]", window, cx);
    });
    cx.assert_editor_state(
        &"
            )}]ˇ]
            )}]ˇ]
            )}]ˇ]
        "
        .unindent(),
    );
}
