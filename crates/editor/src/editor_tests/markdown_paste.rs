use super::*;

#[gpui::test]
async fn test_paste_url_from_other_app_creates_markdown_link_over_selected_text(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Hello, «editorˇ».\nMav is «ˇgreat» (see this link: ˇ)");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "Hello, [editor]({url})ˇ.\nMav is [great]({url})ˇ (see this link: {url}ˇ)"
    ));
}

#[gpui::test]
async fn test_markdown_indents(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Test if adding a character with multi cursors preserves nested list indents
    cx.set_state(&indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [ˇ] Item 2
            - [ˇ] Item 2.a
            - [ˇ] Item 2.b
        "
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("x", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [xˇ] Item 2
            - [xˇ] Item 2.a
            - [xˇ] Item 2.b
        "
    });

    // Case 2: Test adding new line after nested list continues the list with unchecked task
    cx.set_state(&indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.bˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.b
            - [ ] ˇ"
    });

    // Case 3: Test adding content to continued list item
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Item 2.c", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.b
            - [ ] Item 2.cˇ"
    });

    // Case 4: Test adding new line after nested ordered list continues with next number
    cx.set_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.bˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.b
            3. ˇ"
    });

    // Case 5: Adding content to continued ordered list item
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Item 2.c", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.b
            3. Item 2.cˇ"
    });

    // Case 6: Test adding new line after nested ordered list preserves indent of previous line
    cx.set_state(indoc! {"
        - Item 1
            - Item 1.a
            - Item 1.a
        ˇ"});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("-", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - Item 1
            - Item 1.a
            - Item 1.a
        -ˇ"});

    // Case 7: Test blockquote newline preserves something
    cx.set_state(indoc! {"
        > Item 1ˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        > Item 1
        ˇ"
    });
}

#[gpui::test]
async fn test_paste_url_from_mav_copy_creates_markdown_link_over_selected_text(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state(&format!(
        "Hello, editor.\nMav is great (see this link: )\n«{url}ˇ»"
    ));

    cx.update_editor(|editor, window, cx| {
        editor.copy(&Copy, window, cx);
    });

    cx.set_state(&format!(
        "Hello, «editorˇ».\nMav is «ˇgreat» (see this link: ˇ)\n{url}"
    ));

    cx.update_editor(|editor, window, cx| {
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "Hello, [editor]({url})ˇ.\nMav is [great]({url})ˇ (see this link: {url}ˇ)\n{url}"
    ));
}

#[gpui::test]
async fn test_paste_url_from_other_app_replaces_existing_url_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Please visit mav's homepage: «https://www.apple.comˇ»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("Please visit mav's homepage: {url}ˇ"));
}

#[gpui::test]
async fn test_paste_plain_text_from_other_app_replaces_selection_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let text = "Awesome";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Hello, «editorˇ».\nMav is «ˇgreat»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("Hello, {text}ˇ.\nMav is {text}ˇ"));
}

#[gpui::test]
async fn test_paste_text_with_scheme_like_prefix_replaces_selection_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    // `url::Url::parse` accepts this as a URL with the scheme `editor`, but it
    // should not be treated as one when pasting.
    let text = "editor: Fix double-click bracket selection for large spans";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("«(feat on git-ui-add-info-exclude-to-context-menus) Fmtˇ»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("{text}ˇ"));
}

#[gpui::test]
async fn test_paste_url_from_other_app_without_creating_markdown_link_in_non_markdown_language(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("// Hello, «editorˇ».\n// Mav is «ˇgreat» (see this link: ˇ)");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "// Hello, {url}ˇ.\n// Mav is {url}ˇ (see this link: {url}ˇ)"
    ));
}

#[gpui::test]
async fn test_paste_url_from_other_app_creates_markdown_link_selectively_in_multi_buffer(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("this will embed -> link", vec![Point::row_range(0..1)]),
                ("this will replace -> link", vec![Point::row_range(0..1)]),
            ],
            cx,
        );
        let mut editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(vec![
                Point::new(0, 19)..Point::new(0, 23),
                Point::new(1, 21)..Point::new(1, 25),
            ])
        });
        let snapshot = multi_buffer.read(cx).snapshot(cx);
        let first_buffer_id = snapshot.all_buffer_ids().next().unwrap();
        let first_buffer = multi_buffer.read(cx).buffer(first_buffer_id).unwrap();
        first_buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(markdown_language.clone()), cx);
        });

        editor
    });
    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "this will embed -> [link]({url})ˇ\nthis will replace -> {url}ˇ"
    ));
}
