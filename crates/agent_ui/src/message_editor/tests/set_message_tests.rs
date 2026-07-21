use super::*;

#[gpui::test]
async fn test_set_message_plain_text(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, cx) = setup_message_editor(cx).await;

    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "hello world".to_string(),
            ))],
            window,
            cx,
        );
    });

    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert_eq!(text, "hello world");
    assert!(!message_editor.update(cx, |editor, cx| editor.is_empty(cx)));
}

#[gpui::test]
async fn test_set_message_normalizes_crlf_before_mention(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, cx) = setup_message_editor(cx).await;

    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![
                acp::ContentBlock::Text(acp::TextContent::new("before\r\n".to_string())),
                acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
                    "file.txt",
                    "file:///project/file.txt",
                )),
            ],
            window,
            cx,
        );
    });

    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert_eq!(text, "before\n[@file.txt](file:///project/file.txt)");

    let mention_uris =
        message_editor.update(cx, |editor, cx| editor.mention_set.read(cx).mentions());
    assert_eq!(mention_uris.len(), 1);
}

#[gpui::test]
async fn test_set_message_replaces_existing_content(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, cx) = setup_message_editor(cx).await;

    // Set initial content.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "old content".to_string(),
            ))],
            window,
            cx,
        );
    });

    // Replace with new content.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "new content".to_string(),
            ))],
            window,
            cx,
        );
    });

    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert_eq!(
        text, "new content",
        "set_message should replace old content"
    );
}

#[gpui::test]
async fn test_append_message_to_empty_editor(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, cx) = setup_message_editor(cx).await;

    message_editor.update_in(cx, |editor, window, cx| {
        editor.append_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "appended".to_string(),
            ))],
            Some("\n\n"),
            window,
            cx,
        );
    });

    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert_eq!(
        text, "appended",
        "No separator should be inserted when the editor is empty"
    );
}

#[gpui::test]
async fn test_append_message_to_non_empty_editor(cx: &mut TestAppContext) {
    init_test(cx);
    let (message_editor, cx) = setup_message_editor(cx).await;

    // Seed initial content.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "initial".to_string(),
            ))],
            window,
            cx,
        );
    });

    // Append with separator.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.append_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "appended".to_string(),
            ))],
            Some("\n\n"),
            window,
            cx,
        );
    });

    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert_eq!(
        text, "initial\n\nappended",
        "Separator should appear between existing and appended content"
    );
}

#[gpui::test]
async fn test_append_message_preserves_mention_offset(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file.txt": "content"}))
        .await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                None,
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: None,
                },
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    // Seed plain-text prefix so the editor is non-empty before appending.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_message(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "prefix text".to_string(),
            ))],
            window,
            cx,
        );
    });

    // Append a message that contains a ResourceLink mention.
    message_editor.update_in(cx, |editor, window, cx| {
        editor.append_message(
            vec![acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
                "file.txt",
                "file:///project/file.txt",
            ))],
            Some("\n\n"),
            window,
            cx,
        );
    });

    cx.run_until_parked();

    // The mention should be registered in the mention_set so that contents()
    // will emit it as a structured block rather than plain text.
    let mention_uris =
        message_editor.update(cx, |editor, cx| editor.mention_set.read(cx).mentions());
    assert_eq!(
        mention_uris.len(),
        1,
        "Expected exactly one mention in the mention_set after append, got: {mention_uris:?}"
    );

    // The editor text should start with the prefix, then the separator, then
    // the mention placeholder — confirming the offset was computed correctly.
    let text = message_editor.update(cx, |editor, cx| editor.text(cx));
    assert!(
        text.starts_with("prefix text\n\n"),
        "Expected text to start with 'prefix text\\n\\n', got: {text:?}"
    );
}
