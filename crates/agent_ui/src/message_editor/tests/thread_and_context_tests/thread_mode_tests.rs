use super::*;

#[gpui::test]
async fn test_thread_mode_hidden_when_disabled(cx: &mut TestAppContext) {
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

    message_editor.update(cx, |editor, _cx| {
        editor
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().embedded_context(true));
    });

    let supported_modes = {
        let app = cx.app.borrow();
        let _ = &app;
        message_editor
            .read(&app)
            .session_capabilities
            .read()
            .supported_modes(false)
    };

    assert!(
        !supported_modes.contains(&PromptContextType::Thread),
        "Expected thread mode to be hidden when thread mentions are disabled"
    );
}

#[gpui::test]
async fn test_thread_mode_visible_when_enabled(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file": ""})).await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

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

    message_editor.update(cx, |editor, _cx| {
        editor
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().embedded_context(true));
    });

    let supported_modes = {
        let app = cx.app.borrow();
        let _ = &app;
        message_editor
            .read(&app)
            .session_capabilities
            .read()
            .supported_modes(true)
    };

    assert!(
        supported_modes.contains(&PromptContextType::Thread),
        "Expected thread mode to be visible when enabled"
    );
}

#[gpui::test]
async fn test_whitespace_trimming(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({"file.rs": "fn main() {}"}))
        .await;
    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

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

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("  \u{A0}してhello world  ", window, cx);
    });

    let (content, _) = message_editor
        .update(cx, |message_editor, cx| message_editor.contents(false, cx))
        .await
        .unwrap();

    assert_eq!(content, vec!["してhello world".into()]);
}

#[gpui::test]
async fn test_editor_respects_embedded_context_capability(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    let file_content = "fn main() { println!(\"Hello, world!\"); }\n";

    fs.insert_tree(
        "/project",
        json!({
            "src": {
                "main.rs": file_content,
            }
        }),
    )
    .await;

    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

    let (message_editor, editor) = workspace.update_in(cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
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
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });
        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        let editor = message_editor.read(cx).editor().clone();
        (message_editor, editor)
    });

    cx.simulate_input("What is in @file main");

    editor.update_in(cx, |editor, window, cx| {
        assert!(editor.has_visible_completions_menu());
        assert_eq!(editor.text(cx), "What is in @file main");
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    let content = message_editor
        .update(cx, |editor, cx| editor.contents(false, cx))
        .await
        .unwrap()
        .0;

    let main_rs_uri = if cfg!(windows) {
        "file:///C:/project/src/main.rs"
    } else {
        "file:///project/src/main.rs"
    };

    // When embedded context is `false` we should get a resource link
    pretty_assertions::assert_eq!(
        content,
        vec![
            "What is in ".into(),
            acp::ContentBlock::ResourceLink(acp::ResourceLink::new("main.rs", main_rs_uri))
        ]
    );

    message_editor.update(cx, |editor, _cx| {
        editor
            .session_capabilities
            .write()
            .set_prompt_capabilities(acp::PromptCapabilities::new().embedded_context(true))
    });

    let content = message_editor
        .update(cx, |editor, cx| editor.contents(false, cx))
        .await
        .unwrap()
        .0;

    // When embedded context is `true` we should get a resource
    pretty_assertions::assert_eq!(
        content,
        vec![
            "What is in ".into(),
            acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::TextResourceContents(
                    acp::TextResourceContents::new(file_content, main_rs_uri)
                )
            ))
        ]
    );
}
