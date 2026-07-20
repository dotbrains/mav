use super::*;

#[gpui::test]
async fn test_autoindent_syntax_aware_applies_syntax_indent(cx: &mut TestAppContext) {
    // Companion test to show that SyntaxAware DOES apply tree-sitter indentation
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::SyntaxAware)
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

        // With SyntaxAware, tree-sitter adds indentation for being inside `{}`
        assert_eq!(editor.text(cx), "fn foo() {\n    \n}");
    });
}

#[gpui::test]
async fn test_autoindent_disabled_with_nested_language(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::SyntaxAware);
        settings.languages.0.insert(
            "python".into(),
            LanguageSettingsContent {
                auto_indent: Some(settings::AutoIndentMode::None),
                ..Default::default()
            },
        );
    });

    let mut cx = EditorTestContext::new(cx).await;

    let injected_language = Arc::new(
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
                            close: true,
                            surround: false,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                name: "python".into(),
                ..Default::default()
            },
            Some(tree_sitter_python::LANGUAGE.into()),
        )
        .with_indents_query(
            r#"
                (_ "(" ")" @end) @indent
                (_ "{" "}" @end) @indent
            "#,
        )
        .unwrap(),
    );

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
                            close: true,
                            surround: false,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                name: LanguageName::new_static("rust"),
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
        .unwrap()
        .with_injection_query(
            r#"
            (macro_invocation
                macro: (identifier) @_macro_name
                (token_tree) @injection.content
                (#set! injection.language "python"))
           "#,
        )
        .unwrap(),
    );

    cx.language_registry().add(injected_language);
    cx.language_registry().add(language.clone());

    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(r#"struct A {ˇ}"#);

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc!(
        "struct A {
            ˇ
        }"
    ));

    cx.set_state(r#"select_biased!(ˇ)"#);

    cx.update_editor(|editor, window, cx| {
        editor.newline(&Default::default(), window, cx);
        editor.handle_input("def ", window, cx);
        editor.handle_input("(", window, cx);
        editor.newline(&Default::default(), window, cx);
        editor.handle_input("a", window, cx);
    });

    cx.assert_editor_state(indoc!(
        "select_biased!(
        def (
        aˇ
        )
        )"
    ));
}
