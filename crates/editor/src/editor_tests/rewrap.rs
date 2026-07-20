use super::*;

#[gpui::test]
async fn test_rewrap_block_comments(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.languages.0.extend([(
            "Rust".into(),
            LanguageSettingsContent {
                allow_rewrap: Some(language_settings::RewrapBehavior::InComments),
                preferred_line_length: Some(40),
                ..Default::default()
            },
        )])
    });

    let mut cx = EditorTestContext::new(cx).await;

    let rust_lang = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Rust".into(),
                line_comments: vec!["// ".into()],
                block_comment: Some(BlockCommentConfig {
                    start: "/*".into(),
                    end: "*/".into(),
                    prefix: "* ".into(),
                    tab_size: 1,
                }),
                documentation_comment: Some(BlockCommentConfig {
                    start: "/**".into(),
                    end: "*/".into(),
                    prefix: "* ".into(),
                    tab_size: 1,
                }),

                ..LanguageConfig::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_override_query("[(line_comment) (block_comment)] @comment.inclusive")
        .unwrap(),
    );

    // regular block comment
    assert_rewrap(
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
            /*ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
        "},
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // indent is respected
    assert_rewrap(
        indoc! {"
            {}
                /*ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
        "},
        indoc! {"
            {}
                /*
                 *ˇ Lorem ipsum dolor sit amet,
                 * consectetur adipiscing elit.
                 */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // short block comments with inline delimiters
    assert_rewrap(
        indoc! {"
            /*ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
            /*ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
        "},
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // multiline block comment with inline start/end delimiters
    assert_rewrap(
        indoc! {"
            /*ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit. */
        "},
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // block comment rewrap still respects paragraph bounds
    assert_rewrap(
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             *
             * Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
        "},
        indoc! {"
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             *
             * Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // documentation comments
    assert_rewrap(
        indoc! {"
            /**ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
            /**
             *ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
        "},
        indoc! {"
            /**
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /**
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // different, adjacent comments
    assert_rewrap(
        indoc! {"
            /**
             *ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
            /*ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
            //ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
        "},
        indoc! {"
            /**
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            //ˇ Lorem ipsum dolor sit amet,
            // consectetur adipiscing elit.
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // selection w/ single short block comment
    assert_rewrap(
        indoc! {"
            «/* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */ˇ»
        "},
        indoc! {"
            «/*
             * Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */ˇ»
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // rewrapping a single comment w/ abutting comments
    assert_rewrap(
        indoc! {"
            /* ˇLorem ipsum dolor sit amet, consectetur adipiscing elit. */
            /* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
        "},
        indoc! {"
            /*
             * ˇLorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // selection w/ non-abutting short block comments
    assert_rewrap(
        indoc! {"
            «/* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */

            /* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */ˇ»
        "},
        indoc! {"
            «/*
             * Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */

            /*
             * Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */ˇ»
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // selection of multiline block comments
    assert_rewrap(
        indoc! {"
            «/* Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit. */ˇ»
        "},
        indoc! {"
            «/*
             * Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */ˇ»
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // partial selection of multiline block comments
    assert_rewrap(
        indoc! {"
            «/* Lorem ipsum dolor sit amet,ˇ»
             * consectetur adipiscing elit. */
            /* Lorem ipsum dolor sit amet,
             «* consectetur adipiscing elit. */ˇ»
        "},
        indoc! {"
            «/*
             * Lorem ipsum dolor sit amet,ˇ»
             * consectetur adipiscing elit. */
            /* Lorem ipsum dolor sit amet,
             «* consectetur adipiscing elit.
             */ˇ»
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // selection w/ abutting short block comments
    // TODO: should not be combined; should rewrap as 2 comments
    assert_rewrap(
        indoc! {"
            «/* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
            /* Lorem ipsum dolor sit amet, consectetur adipiscing elit. */ˇ»
        "},
        // desired behavior:
        // indoc! {"
        //     «/*
        //      * Lorem ipsum dolor sit amet,
        //      * consectetur adipiscing elit.
        //      */
        //     /*
        //      * Lorem ipsum dolor sit amet,
        //      * consectetur adipiscing elit.
        //      */ˇ»
        // "},
        // actual behaviour:
        indoc! {"
            «/*
             * Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit. Lorem
             * ipsum dolor sit amet, consectetur
             * adipiscing elit.
             */ˇ»
        "},
        rust_lang.clone(),
        &mut cx,
    );

    // TODO: same as above, but with delimiters on separate line
    // assert_rewrap(
    //     indoc! {"
    //         «/* Lorem ipsum dolor sit amet, consectetur adipiscing elit.
    //          */
    //         /*
    //          * Lorem ipsum dolor sit amet, consectetur adipiscing elit. */ˇ»
    //     "},
    //     // desired:
    //     // indoc! {"
    //     //     «/*
    //     //      * Lorem ipsum dolor sit amet,
    //     //      * consectetur adipiscing elit.
    //     //      */
    //     //     /*
    //     //      * Lorem ipsum dolor sit amet,
    //     //      * consectetur adipiscing elit.
    //     //      */ˇ»
    //     // "},
    //     // actual: (but with trailing w/s on the empty lines)
    //     indoc! {"
    //         «/*
    //          * Lorem ipsum dolor sit amet,
    //          * consectetur adipiscing elit.
    //          *
    //          */
    //         /*
    //          *
    //          * Lorem ipsum dolor sit amet,
    //          * consectetur adipiscing elit.
    //          */ˇ»
    //     "},
    //     rust_lang.clone(),
    //     &mut cx,
    // );

    // TODO these are unhandled edge cases; not correct, just documenting known issues
    assert_rewrap(
        indoc! {"
            /*
             //ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit.
             */
            /*
             //ˇ Lorem ipsum dolor sit amet, consectetur adipiscing elit. */
            /*ˇ Lorem ipsum dolor sit amet */ /* consectetur adipiscing elit. */
        "},
        // desired:
        // indoc! {"
        //     /*
        //      *ˇ Lorem ipsum dolor sit amet,
        //      * consectetur adipiscing elit.
        //      */
        //     /*
        //      *ˇ Lorem ipsum dolor sit amet,
        //      * consectetur adipiscing elit.
        //      */
        //     /*
        //      *ˇ Lorem ipsum dolor sit amet
        //      */ /* consectetur adipiscing elit. */
        // "},
        // actual:
        indoc! {"
            /*
             //ˇ Lorem ipsum dolor sit amet,
             // consectetur adipiscing elit.
             */
            /*
             * //ˇ Lorem ipsum dolor sit amet,
             * consectetur adipiscing elit.
             */
            /*
             *ˇ Lorem ipsum dolor sit amet */ /*
             * consectetur adipiscing elit.
             */
        "},
        rust_lang,
        &mut cx,
    );

    #[track_caller]
    fn assert_rewrap(
        unwrapped_text: &str,
        wrapped_text: &str,
        language: Arc<Language>,
        cx: &mut EditorTestContext,
    ) {
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
        cx.set_state(unwrapped_text);
        cx.update_editor(|e, _, cx| e.rewrap(RewrapOptions::default(), cx));
        cx.assert_editor_state(wrapped_text);
    }
}
