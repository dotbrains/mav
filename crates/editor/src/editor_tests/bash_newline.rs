use super::*;

#[gpui::test]
async fn test_indent_on_newline_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test correct indent after newline on comment
    cx.set_state(indoc! {"
        # COMMENT:ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        # COMMENT:
        ˇ
    "});

    // test correct indent after newline after `then`
    cx.set_state(indoc! {"

        if [ \"$1\" = \"test\" ]; thenˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        if [ \"$1\" = \"test\" ]; then
            ˇ
    "});

    // test correct indent after newline after `else`
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elseˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        else
            ˇ
    "});

    // test correct indent after newline after `elif`
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elifˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elif
            ˇ
    "});

    // test correct indent after newline after `do`
    cx.set_state(indoc! {"
        for file in *.txt; doˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        for file in *.txt; do
            ˇ
    "});

    // test correct indent after newline after case pattern
    cx.set_state(indoc! {"
        case \"$1\" in
            start)ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                ˇ
    "});

    // test correct indent after newline after case pattern
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                ;;
            *)ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                ;;
            *)
                ˇ
    "});

    // test correct indent after newline after function opening brace
    cx.set_state(indoc! {"
        function test() {ˇ}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function test() {
            ˇ
        }
    "});

    // test no extra indent after semicolon on same line
    cx.set_state(indoc! {"
        echo \"test\";ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        echo \"test\";
        ˇ
    "});
}
