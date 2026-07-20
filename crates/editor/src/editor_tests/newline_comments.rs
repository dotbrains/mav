use super::*;

#[gpui::test]
async fn test_newline_comments(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into()],
            ..LanguageConfig::default()
        },
        None,
    ));
    {
        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(indoc! {"
        // Fooˇ
    "});

        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        // Foo
        // ˇ
    "});
        // Ensure that we add comment prefix when existing line contains space
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(
            indoc! {"
        // Foo
        //s
        // ˇ
    "}
            .replace("s", " ") // s is used as space placeholder to prevent format on save
            .as_str(),
        );
        // Ensure that we add comment prefix when existing line does not contain space
        cx.set_state(indoc! {"
        // Foo
        //ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        // Foo
        //
        // ˇ
    "});
        // Ensure that if cursor is before the comment start, we do not actually insert a comment prefix.
        cx.set_state(indoc! {"
        ˇ// Foo
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"

        ˇ// Foo
    "});
    }
    // Ensure that comment continuations can be disabled.
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        // Fooˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        // Foo
        ˇ
    "});
}

#[gpui::test]
async fn test_newline_comments_with_multiple_delimiters(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into(), "/// ".into()],
            ..LanguageConfig::default()
        },
        None,
    ));
    {
        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(indoc! {"
        //ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        //
        // ˇ
    "});

        cx.set_state(indoc! {"
        ///ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        ///
        /// ˇ
    "});
    }
}

#[gpui::test]
async fn test_newline_comments_with_brackets(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into()],
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "(".to_string(),
                    end: ")".to_string(),
                    close: false,
                    surround: false,
                    newline: true,
                }],
                ..BracketPairConfig::default()
            },
            ..LanguageConfig::default()
        },
        None,
    ));

    {
        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(indoc! {"
        // (ˇ)
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        // (
        // ˇ)
    "})
    }
}

#[gpui::test]
async fn test_newline_comments_repl_separators(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["# ".into()],
            ..LanguageConfig::default()
        },
        None,
    ));

    {
        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(indoc! {"
        # %%ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        # %%
        ˇ
    "});

        cx.set_state(indoc! {"
            # %%%%%ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
            # %%%%%
            ˇ
    "});

        cx.set_state(indoc! {"
            # %ˇ%
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
            # %
            # ˇ%
    "});
    }
}

#[gpui::test]
async fn test_newline_documentation_comments(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                documentation_comment: Some(language::BlockCommentConfig {
                    start: "/**".into(),
                    end: "*/".into(),
                    prefix: "* ".into(),
                    tab_size: 1,
                }),

                ..LanguageConfig::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_override_query("[(line_comment)(block_comment)] @comment.inclusive")
        .unwrap(),
    );

    {
        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(indoc! {"
        /**ˇ
    "});

        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         * ˇ
    "});
        // Ensure that if cursor is before the comment start,
        // we do not actually insert a comment prefix.
        cx.set_state(indoc! {"
        ˇ/**
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"

        ˇ/**
    "});
        // Ensure that if cursor is between it doesn't add comment prefix.
        cx.set_state(indoc! {"
        /*ˇ*
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /*
        ˇ*
    "});
        // Ensure that if suffix exists on same line after cursor it adds new line.
        cx.set_state(indoc! {"
        /**ˇ*/
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         * ˇ
         */
    "});
        // Ensure that if suffix exists on same line after cursor with space it adds new line.
        cx.set_state(indoc! {"
        /**ˇ */
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         * ˇ
         */
    "});
        // Ensure that if suffix exists on same line after cursor with space it adds new line.
        cx.set_state(indoc! {"
        /** ˇ*/
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(
            indoc! {"
        /**s
         * ˇ
         */
    "}
            .replace("s", " ") // s is used as space placeholder to prevent format on save
            .as_str(),
        );
        // Ensure that delimiter space is preserved when newline on already
        // spaced delimiter.
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(
            indoc! {"
        /**s
         *s
         * ˇ
         */
    "}
            .replace("s", " ") // s is used as space placeholder to prevent format on save
            .as_str(),
        );
        // Ensure that delimiter space is preserved when space is not
        // on existing delimiter.
        cx.set_state(indoc! {"
        /**
         *ˇ
         */
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         *
         * ˇ
         */
    "});
        // Ensure that if suffix exists on same line after cursor it
        // doesn't add extra new line if prefix is not on same line.
        cx.set_state(indoc! {"
        /**
        ˇ*/
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**

        ˇ*/
    "});
        // Ensure that it detects suffix after existing prefix.
        cx.set_state(indoc! {"
        /**ˇ/
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
        ˇ/
    "});
        // Ensure that if suffix exists on same line before
        // cursor it does not add comment prefix.
        cx.set_state(indoc! {"
        /** */ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /** */
        ˇ
    "});
        // Ensure that if suffix exists on same line before
        // cursor it does not add comment prefix.
        cx.set_state(indoc! {"
        /**
         *
         */ˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         *
         */
         ˇ
    "});

        // Ensure that inline comment followed by code
        // doesn't add comment prefix on newline
        cx.set_state(indoc! {"
        /** */ textˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /** */ text
        ˇ
    "});

        // Ensure that text after comment end tag
        // doesn't add comment prefix on newline
        cx.set_state(indoc! {"
        /**
         *
         */ˇtext
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        /**
         *
         */
         ˇtext
    "});

        // Ensure if not comment block it doesn't
        // add comment prefix on newline
        cx.set_state(indoc! {"
        * textˇ
    "});
        cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
        cx.assert_editor_state(indoc! {"
        * text
        ˇ
    "});
    }
    // Ensure that comment continuations can be disabled.
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        /**ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        /**
        ˇ
    "});
}

#[gpui::test]
async fn test_newline_comments_with_block_comment(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let lua_language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["--".into()],
            block_comment: Some(language::BlockCommentConfig {
                start: "--[[".into(),
                prefix: "".into(),
                end: "]]".into(),
                tab_size: 0,
            }),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(lua_language), cx));

    // Line with line comment should extend
    cx.set_state(indoc! {"
        --ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        --
        --ˇ
    "});

    // Line with block comment that matches line comment should not extend
    cx.set_state(indoc! {"
        --[[ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        --[[
        ˇ
    "});
}
