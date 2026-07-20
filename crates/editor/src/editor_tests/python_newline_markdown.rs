use super::*;

#[gpui::test]
async fn test_indent_on_newline_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
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

    // test correct indent after newline in brackets
    cx.set_state(indoc! {"
        {ˇ}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        {
            ˇ
        }
    "});

    cx.set_state(indoc! {"
        (ˇ)
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        (
            ˇ
        )
    "});

    // do not indent after empty lists or dictionaries
    cx.set_state(indoc! {"
        a = []ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        a = []
        ˇ
    "});
}

#[gpui::test]
async fn test_python_indent_in_markdown(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language_registry = Arc::new(language::LanguageRegistry::test(cx.executor()));
    let python_lang = languages::language("python", tree_sitter_python::LANGUAGE.into());
    language_registry.add(markdown_lang());
    language_registry.add(python_lang);

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| {
        buffer.set_language_registry(language_registry);
        buffer.set_language(Some(markdown_lang()), cx);
    });

    // Test that `else:` correctly outdents to match `if:` inside the Python code block
    cx.set_state(indoc! {"
        # Heading

        ```python
        def main():
            if condition:
                pass
                ˇ
        ```
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        # Heading

        ```python
        def main():
            if condition:
                pass
            else:ˇ
        ```
    "});
}
