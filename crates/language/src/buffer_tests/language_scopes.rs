use super::*;

#[gpui::test]
fn test_language_scope_at_with_javascript(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let language = Language::new(
            LanguageConfig {
                name: "JavaScript".into(),
                line_comments: vec!["// ".into()],
                block_comment: Some(BlockCommentConfig {
                    start: "/*".into(),
                    end: "*/".into(),
                    prefix: "* ".into(),
                    tab_size: 1,
                }),
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".into(),
                            end: "}".into(),
                            close: true,
                            surround: true,
                            newline: false,
                        },
                        BracketPair {
                            start: "'".into(),
                            end: "'".into(),
                            close: true,
                            surround: true,
                            newline: false,
                        },
                    ],
                    disabled_scopes_by_bracket_ix: vec![
                        Vec::new(),                              //
                        vec!["string".into(), "comment".into()], // single quotes disabled
                    ],
                },
                overrides: [(
                    "element".into(),
                    LanguageConfigOverride {
                        line_comments: Override::Remove { remove: true },
                        block_comment: Override::Set(BlockCommentConfig {
                            start: "{/*".into(),
                            prefix: "".into(),
                            end: "*/}".into(),
                            tab_size: 0,
                        }),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        )
        .with_override_query(
            r#"
                (jsx_element) @element
                (string) @string
                (comment) @comment.inclusive
                [
                    (jsx_opening_element)
                    (jsx_closing_element)
                    (jsx_expression)
                ] @default
            "#,
        )
        .unwrap();

        let text = r#"
            a["b"] = <C d="e">
                <F></F>
                { g() }
            </C>; // a comment
        "#
        .unindent();

        let buffer = Buffer::local(&text, cx).with_language(Arc::new(language), cx);
        let snapshot = buffer.snapshot();

        let config = snapshot.language_scope_at(0).unwrap();
        assert_eq!(config.line_comment_prefixes(), &[Arc::from("// ")]);
        assert_eq!(
            config.block_comment(),
            Some(&BlockCommentConfig {
                start: "/*".into(),
                prefix: "* ".into(),
                end: "*/".into(),
                tab_size: 1,
            })
        );

        // Both bracket pairs are enabled
        assert_eq!(
            config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, true]
        );

        let comment_config = snapshot
            .language_scope_at(text.find("comment").unwrap() + "comment".len())
            .unwrap();
        assert_eq!(
            comment_config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, false]
        );

        let string_config = snapshot
            .language_scope_at(text.find("b\"").unwrap())
            .unwrap();
        assert_eq!(string_config.line_comment_prefixes(), &[Arc::from("// ")]);
        assert_eq!(
            string_config.block_comment(),
            Some(&BlockCommentConfig {
                start: "/*".into(),
                prefix: "* ".into(),
                end: "*/".into(),
                tab_size: 1,
            })
        );
        // Second bracket pair is disabled
        assert_eq!(
            string_config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, false]
        );

        // In between JSX tags: use the `element` override.
        let element_config = snapshot
            .language_scope_at(text.find("<F>").unwrap())
            .unwrap();
        // TODO nested blocks after newlines are captured with all whitespaces
        // https://github.com/tree-sitter/tree-sitter-typescript/issues/306
        // assert_eq!(element_config.line_comment_prefixes(), &[]);
        // assert_eq!(
        //     element_config.block_comment_delimiters(),
        //     Some((&"{/*".into(), &"*/}".into()))
        // );
        assert_eq!(
            element_config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, true]
        );

        // Within a JSX tag: use the default config.
        let tag_config = snapshot
            .language_scope_at(text.find(" d=").unwrap() + 1)
            .unwrap();
        assert_eq!(tag_config.line_comment_prefixes(), &[Arc::from("// ")]);
        assert_eq!(
            tag_config.block_comment(),
            Some(&BlockCommentConfig {
                start: "/*".into(),
                prefix: "* ".into(),
                end: "*/".into(),
                tab_size: 1,
            })
        );
        assert_eq!(
            tag_config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, true]
        );

        // In a JSX expression: use the default config.
        let expression_in_element_config = snapshot
            .language_scope_at(text.find('{').unwrap() + 1)
            .unwrap();
        assert_eq!(
            expression_in_element_config.line_comment_prefixes(),
            &[Arc::from("// ")]
        );
        assert_eq!(
            expression_in_element_config.block_comment(),
            Some(&BlockCommentConfig {
                start: "/*".into(),
                prefix: "* ".into(),
                end: "*/".into(),
                tab_size: 1,
            })
        );
        assert_eq!(
            expression_in_element_config
                .brackets()
                .map(|e| e.1)
                .collect::<Vec<_>>(),
            &[true, true]
        );

        buffer
    });
}

#[gpui::test]
fn test_language_scope_at_with_rust(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let language = Language::new(
            LanguageConfig {
                name: "Rust".into(),
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".into(),
                            end: "}".into(),
                            close: true,
                            surround: true,
                            newline: false,
                        },
                        BracketPair {
                            start: "'".into(),
                            end: "'".into(),
                            close: true,
                            surround: true,
                            newline: false,
                        },
                    ],
                    disabled_scopes_by_bracket_ix: vec![
                        Vec::new(), //
                        vec!["string".into()],
                    ],
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_override_query(
            r#"
                (string_literal) @string
            "#,
        )
        .unwrap();

        let text = r#"
            const S: &'static str = "hello";
        "#
        .unindent();

        let buffer = Buffer::local(text.clone(), cx).with_language(Arc::new(language), cx);
        let snapshot = buffer.snapshot();

        // By default, all brackets are enabled
        let config = snapshot.language_scope_at(0).unwrap();
        assert_eq!(
            config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, true]
        );

        // Within a string, the quotation brackets are disabled.
        let string_config = snapshot
            .language_scope_at(text.find("ello").unwrap())
            .unwrap();
        assert_eq!(
            string_config.brackets().map(|e| e.1).collect::<Vec<_>>(),
            &[true, false]
        );

        buffer
    });
}
