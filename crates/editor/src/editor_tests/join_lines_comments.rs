use super::*;

#[gpui::test]
async fn test_join_lines_strips_comment_prefix(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    {
        let language = Arc::new(Language::new(
            LanguageConfig {
                line_comments: vec!["// ".into(), "/// ".into()],
                documentation_comment: Some(BlockCommentConfig {
                    start: "/*".into(),
                    end: "*/".into(),
                    prefix: "* ".into(),
                    tab_size: 1,
                }),
                ..LanguageConfig::default()
            },
            None,
        ));

        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

        // Strips the comment prefix (with trailing space) from the joined-in line.
        cx.set_state(indoc! {"
            // ˇfoo
            // bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            // fooˇ bar
        "});

        // Strips the longer doc-comment prefix when both `//` and `///` match.
        cx.set_state(indoc! {"
            /// ˇfoo
            /// bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            /// fooˇ bar
        "});

        // Does not strip when the second line is a regular line (no comment prefix).
        cx.set_state(indoc! {"
            // ˇfoo
            bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            // fooˇ bar
        "});

        // No-whitespace join also strips the comment prefix.
        cx.set_state(indoc! {"
            // ˇfoo
            // bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines_impl(false, window, cx));
        cx.assert_editor_state(indoc! {"
            // fooˇbar
        "});

        // Strips even when the joined-in line is just the bare prefix (no trailing space).
        cx.set_state(indoc! {"
            // ˇfoo
            //
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            // fooˇ
        "});

        // Mixed line comment prefix types: the longer matching prefix is stripped.
        cx.set_state(indoc! {"
            // ˇfoo
            /// bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            // fooˇ bar
        "});

        // Strips block comment body prefix (`* `) from the joined-in line.
        cx.set_state(indoc! {"
             * ˇfoo
             * bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
             * fooˇ bar
        "});

        // Strips bare block comment body prefix (`*` without trailing space).
        cx.set_state(indoc! {"
             * ˇfoo
             *
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
             * fooˇ
        "});
    }

    {
        let markdown_language = Arc::new(Language::new(
            LanguageConfig {
                unordered_list: vec!["- ".into(), "* ".into(), "+ ".into()],
                ..LanguageConfig::default()
            },
            None,
        ));

        let mut cx = EditorTestContext::new(cx).await;
        cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

        // Strips the `- ` list marker from the joined-in line.
        cx.set_state(indoc! {"
            - ˇfoo
            - bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            - fooˇ bar
        "});

        // Strips the `* ` list marker from the joined-in line.
        cx.set_state(indoc! {"
            * ˇfoo
            * bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            * fooˇ bar
        "});

        // Strips the `+ ` list marker from the joined-in line.
        cx.set_state(indoc! {"
            + ˇfoo
            + bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines(&JoinLines, window, cx));
        cx.assert_editor_state(indoc! {"
            + fooˇ bar
        "});

        // No-whitespace join also strips the list marker.
        cx.set_state(indoc! {"
            - ˇfoo
            - bar
        "});
        cx.update_editor(|e, window, cx| e.join_lines_impl(false, window, cx));
        cx.assert_editor_state(indoc! {"
            - fooˇbar
        "});
    }
}
