use super::*;

#[gpui::test]
async fn test_indent_outdent(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4);
    });

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
          «oneˇ» «twoˇ»
        three
         four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
            «oneˇ» «twoˇ»
        three
         four
    "});

    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        «oneˇ» «twoˇ»
        three
         four
    "});

    // select across line ending
    cx.set_state(indoc! {"
        one two
        t«hree
        ˇ» four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
            t«hree
        ˇ» four
    "});

    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        t«hree
        ˇ» four
    "});

    // Ensure that indenting/outdenting works when the cursor is at column 0.
    cx.set_state(indoc! {"
        one two
        ˇthree
            four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
            ˇthree
            four
    "});

    cx.set_state(indoc! {"
        one two
        ˇ    three
            four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        ˇthree
            four
    "});
}

#[gpui::test]
async fn test_indent_yaml_comments_with_multiple_cursors(cx: &mut TestAppContext) {
    // This is a regression test for issue #33761
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let yaml_language = languages::language("yaml", tree_sitter_yaml::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(yaml_language), cx));

    cx.set_state(
        r#"ˇ#     ingress:
ˇ#         api:
ˇ#             enabled: false
ˇ#             pathType: Prefix
ˇ#           console:
ˇ#               enabled: false
ˇ#               pathType: Prefix
"#,
    );

    // Press tab to indent all lines
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));

    cx.assert_editor_state(
        r#"    ˇ#     ingress:
    ˇ#         api:
    ˇ#             enabled: false
    ˇ#             pathType: Prefix
    ˇ#           console:
    ˇ#               enabled: false
    ˇ#               pathType: Prefix
"#,
    );
}

#[gpui::test]
async fn test_indent_yaml_non_comments_with_multiple_cursors(cx: &mut TestAppContext) {
    // This is a test to make sure our fix for issue #33761 didn't break anything
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let yaml_language = languages::language("yaml", tree_sitter_yaml::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(yaml_language), cx));

    cx.set_state(
        r#"ˇingress:
ˇ  api:
ˇ    enabled: false
ˇ    pathType: Prefix
"#,
    );

    // Press tab to indent all lines
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));

    cx.assert_editor_state(
        r#"ˇingress:
    ˇapi:
        ˇenabled: false
        ˇpathType: Prefix
"#,
    );
}

#[gpui::test]
async fn test_indent_outdent_with_hard_tabs(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.hard_tabs = Some(true);
    });

    let mut cx = EditorTestContext::new(cx).await;

    // select two ranges on one line
    cx.set_state(indoc! {"
        «oneˇ» «twoˇ»
        three
        four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        \t«oneˇ» «twoˇ»
        three
        four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        \t\t«oneˇ» «twoˇ»
        three
        four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        \t«oneˇ» «twoˇ»
        three
        four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        «oneˇ» «twoˇ»
        three
        four
    "});

    // select across a line ending
    cx.set_state(indoc! {"
        one two
        t«hree
        ˇ»four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        \tt«hree
        ˇ»four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        \t\tt«hree
        ˇ»four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        \tt«hree
        ˇ»four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        t«hree
        ˇ»four
    "});

    // Ensure that indenting/outdenting works when the cursor is at column 0.
    cx.set_state(indoc! {"
        one two
        ˇthree
        four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        ˇthree
        four
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        \tˇthree
        four
    "});
    cx.update_editor(|e, window, cx| e.backtab(&Backtab, window, cx));
    cx.assert_editor_state(indoc! {"
        one two
        ˇthree
        four
    "});
}

#[gpui::test]
fn test_indent_outdent_with_excerpts(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.languages.0.extend([
            (
                "TOML".into(),
                LanguageSettingsContent {
                    tab_size: NonZeroU32::new(2),
                    ..Default::default()
                },
            ),
            (
                "Rust".into(),
                LanguageSettingsContent {
                    tab_size: NonZeroU32::new(4),
                    ..Default::default()
                },
            ),
        ]);
    });

    let toml_language = Arc::new(Language::new(
        LanguageConfig {
            name: "TOML".into(),
            ..Default::default()
        },
        None,
    ));
    let rust_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            ..Default::default()
        },
        None,
    ));

    let toml_buffer =
        cx.new(|cx| Buffer::local("a = 1\nb = 2\n", cx).with_language(toml_language, cx));
    let rust_buffer =
        cx.new(|cx| Buffer::local("const c: usize = 3;\n", cx).with_language(rust_language, cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            toml_buffer.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            rust_buffer.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);

        assert_eq!(
            editor.text(cx),
            indoc! {"
                a = 1
                b = 2

                const c: usize = 3;
            "}
        );

        select_ranges(
            &mut editor,
            indoc! {"
                «aˇ» = 1
                b = 2

                «const c:ˇ» usize = 3;
            "},
            window,
            cx,
        );

        editor.tab(&Tab, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                  «aˇ» = 1
                b = 2

                    «const c:ˇ» usize = 3;
            "},
            cx,
        );
        editor.backtab(&Backtab, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                «aˇ» = 1
                b = 2

                «const c:ˇ» usize = 3;
            "},
            cx,
        );

        editor
    });
}
