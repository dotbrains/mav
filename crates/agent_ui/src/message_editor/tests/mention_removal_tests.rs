use super::*;

#[gpui::test]
async fn test_at_mention_removal(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file": ""})).await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = None;

    let message_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            MessageEditor::new(
                workspace.downgrade(),
                project.downgrade(),
                thread_store.clone(),
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
    let editor = message_editor.update(cx, |message_editor, _| message_editor.editor.clone());

    cx.run_until_parked();

    let completions = editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello @file ", window, cx);
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        let completion_provider = editor.completion_provider().unwrap();
        completion_provider.completions(
            &buffer,
            text::Anchor::max_for_buffer(buffer.read(cx).remote_id()),
            CompletionContext {
                trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
                trigger_character: Some("@".into()),
            },
            window,
            cx,
        )
    });
    let [_, completion]: [_; 2] = completions
        .await
        .unwrap()
        .into_iter()
        .flat_map(|response| response.completions)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let range = snapshot
            .buffer_anchor_range_to_anchor_range(completion.replace_range)
            .unwrap();
        editor.edit([(range, completion.new_text)], cx);
        (completion.confirm.unwrap())(CompletionIntent::Complete, window, cx);
    });

    cx.run_until_parked();

    // Backspace over the inserted crease (and the following space).
    editor.update_in(cx, |editor, window, cx| {
        editor.backspace(&Default::default(), window, cx);
        editor.backspace(&Default::default(), window, cx);
    });

    let (content, _) = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await
        .unwrap();

    // We don't send a resource link for the deleted crease.
    pretty_assertions::assert_matches!(content.as_slice(), [acp::ContentBlock::Text { .. }]);
}

#[gpui::test]
async fn test_slash_command_validation(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/test",
        json!({
            ".mav": {
                "tasks.json": r#"[{"label": "test", "command": "echo"}]"#
            },
            "src": {
                "main.rs": "fn main() {}",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
    let thread_store = None;
    let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::from_acp_commands(
        acp::PromptCapabilities::default(),
        vec![],
    )));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let workspace_handle = workspace.downgrade();
    let message_editor = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| {
            MessageEditor::new(
                workspace_handle.clone(),
                project.downgrade(),
                thread_store.clone(),
                session_capabilities.clone(),
                "Claude Agent".into(),
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
    let editor = message_editor.update(cx, |message_editor, _| message_editor.editor.clone());

    // Test that slash commands fail when no available_commands are set (empty list means no commands supported)
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("/file test.txt", window, cx);
    });

    let contents_result = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await;

    // Should fail because available_commands is empty (no commands supported)
    assert!(contents_result.is_err());
    let error_message = contents_result.unwrap_err().to_string();
    assert!(error_message.contains("is not a recognized command in Claude Agent"));
    assert!(error_message.contains("Available commands for Claude Agent: none"));

    // Now simulate Claude providing its list of available commands (which doesn't include file)
    session_capabilities
        .write()
        .set_available_commands(vec![acp::AvailableCommand::new("help", "Get help")]);

    // Test that unsupported slash commands trigger an error when we have a list of available commands
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("/file test.txt", window, cx);
    });

    let contents_result = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await;

    assert!(contents_result.is_err());
    let error_message = contents_result.unwrap_err().to_string();
    assert!(error_message.contains("is not a recognized command in Claude Agent"));
    assert!(error_message.contains("/file"));
    assert!(error_message.contains("Available commands for Claude Agent: /help"));

    // Test that supported commands work fine
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("/help", window, cx);
    });

    let contents_result = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await;

    // Should succeed because /help is in available_commands
    assert!(contents_result.is_ok());

    // Test that regular text works fine
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello Claude!", window, cx);
    });

    let (content, _) = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await
        .unwrap();

    assert_eq!(content.len(), 1);
    if let acp::ContentBlock::Text(text) = &content[0] {
        assert_eq!(text.text, "Hello Claude!");
    } else {
        panic!("Expected ContentBlock::Text");
    }

    // Test that @ mentions still work
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Check this @", window, cx);
    });

    // The @ mention functionality should not be affected
    let (content, _) = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await
        .unwrap();

    assert_eq!(content.len(), 1);
    if let acp::ContentBlock::Text(text) = &content[0] {
        assert_eq!(text.text, "Check this @");
    } else {
        panic!("Expected ContentBlock::Text");
    }
}

struct MessageEditorItem(Entity<MessageEditor>);

impl Item for MessageEditorItem {
    type Event = ();

    fn include_in_nav_history() -> bool {
        false
    }

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Test".into()
    }
}

impl EventEmitter<()> for MessageEditorItem {}

impl Focusable for MessageEditorItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }
}

impl Render for MessageEditorItem {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.0.clone().into_any_element()
    }
}
