use super::*;

#[gpui::test]
async fn test_completion_provider_commands(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    let project = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let thread_store = None;
    let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::from_acp_commands(
        acp::PromptCapabilities::default(),
        vec![
            acp::AvailableCommand::new("quick-math", "2 + 2 = 4 - 1 = 3"),
            acp::AvailableCommand::new("say-hello", "Say hello to whoever you want").input(
                acp::AvailableCommandInput::Unstructured(acp::UnstructuredCommandInput::new(
                    "<name>",
                )),
            ),
        ],
    )));

    let editor = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                thread_store.clone(),
                session_capabilities.clone(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
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
        message_editor.read(cx).editor().clone()
    });

    cx.simulate_input("/");

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "/");
        assert!(editor.has_visible_completions_menu());

        assert_eq!(
            current_completion_labels_with_documentation(editor),
            &[
                ("quick-math".into(), "2 + 2 = 4 - 1 = 3".into()),
                ("say-hello".into(), "Say hello to whoever you want".into())
            ]
        );
        editor.set_text("", window, cx);
    });

    cx.simulate_input("/qui");

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "/qui");
        assert!(editor.has_visible_completions_menu());

        assert_eq!(
            current_completion_labels_with_documentation(editor),
            &[("quick-math".into(), "2 + 2 = 4 - 1 = 3".into())]
        );
        editor.set_text("", window, cx);
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        assert!(editor.has_visible_completions_menu());
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.display_text(cx), "/quick-math ");
        assert!(!editor.has_visible_completions_menu());
        editor.set_text("", window, cx);
    });

    cx.simulate_input("/say");

    editor.update_in(&mut cx, |editor, _window, cx| {
        assert_eq!(editor.display_text(cx), "/say");
        assert!(editor.has_visible_completions_menu());

        assert_eq!(
            current_completion_labels_with_documentation(editor),
            &[("say-hello".into(), "Say hello to whoever you want".into())]
        );
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        assert!(editor.has_visible_completions_menu());
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, _window, cx| {
        assert_eq!(editor.text(cx), "/say-hello ");
        assert_eq!(editor.display_text(cx), "/say-hello <name>");
        assert!(!editor.has_visible_completions_menu());
    });

    cx.simulate_input("GPT5");

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "/say-hello GPT5");
        assert_eq!(editor.display_text(cx), "/say-hello GPT5");
        assert!(!editor.has_visible_completions_menu());

        // Delete argument
        for _ in 0..5 {
            editor.backspace(&editor::actions::Backspace, window, cx);
        }
    });

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "/say-hello");
        // Hint is visible because argument was deleted
        assert_eq!(editor.display_text(cx), "/say-hello <name>");

        // Delete last command letter
        editor.backspace(&editor::actions::Backspace, window, cx);
    });

    cx.run_until_parked();

    editor.update_in(&mut cx, |editor, _window, cx| {
        // Hint goes away once command no longer matches an available one
        assert_eq!(editor.text(cx), "/say-hell");
        assert_eq!(editor.display_text(cx), "/say-hell");
        assert!(!editor.has_visible_completions_menu());
    });
}

/// Opening slash-command autocomplete must emit
/// [`MessageEditorEvent::SlashAutocompleteOpened`]. `ThreadView`
/// subscribes to that event to fire the global-skills scan trigger
/// (see `NativeAgent::ensure_skills_scan_started`); without the
/// event the trigger never runs and lazily-discovered skills never
/// appear in autocomplete.
#[gpui::test]
async fn test_slash_autocomplete_emits_opened_event(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    let project = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::from_acp_commands(
        acp::PromptCapabilities::default(),
        vec![acp::AvailableCommand::new("hello", "Say hello")],
    )));

    // Track every event emitted by the message editor across the
    // lifetime of the test. We expect to see Focus (from the focus
    // call below) and SlashAutocompleteOpened (from typing "/").
    let received_events: Arc<parking_lot::Mutex<Vec<MessageEditorEvent>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));

    let editor = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                None,
                session_capabilities.clone(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
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

        let received_events = received_events.clone();
        cx.subscribe(
            &message_editor,
            move |_editor: &mut Workspace, _, event: &MessageEditorEvent, _cx| {
                received_events.lock().push(event.clone());
            },
        )
        .detach();

        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        message_editor.read(cx).editor().clone()
    });

    cx.simulate_input("/");

    editor.update_in(&mut cx, |editor, _window, cx| {
        assert_eq!(editor.text(cx), "/");
        assert!(editor.has_visible_completions_menu());
    });

    let events = received_events.lock();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, MessageEditorEvent::SlashAutocompleteOpened)),
        "expected SlashAutocompleteOpened to have been emitted; saw events: {events:?}",
    );
}

mod context_completion_tests;
