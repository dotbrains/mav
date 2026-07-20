use super::*;

#[gpui::test]
async fn test_completion_can_run_commands(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let command_calls = Arc::new(AtomicUsize::new(0));
    let registered_command = "_the/command";

    let closure_command_calls = command_calls.clone();
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    ..lsp::CompletionOptions::default()
                }),
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: vec![registered_command.to_owned()],
                    ..lsp::ExecuteCommandOptions::default()
                }),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                fake_server.set_request_handler::<lsp::request::Completion, _, _>(
                    move |params, _| async move {
                        Ok(Some(lsp::CompletionResponse::Array(vec![
                            lsp::CompletionItem {
                                label: "registered_command".to_owned(),
                                text_edit: gen_text_edit(&params, ""),
                                command: Some(lsp::Command {
                                    title: registered_command.to_owned(),
                                    command: "_the/command".to_owned(),
                                    arguments: Some(vec![serde_json::Value::Bool(true)]),
                                }),
                                ..lsp::CompletionItem::default()
                            },
                            lsp::CompletionItem {
                                label: "unregistered_command".to_owned(),
                                text_edit: gen_text_edit(&params, ""),
                                command: Some(lsp::Command {
                                    title: "????????????".to_owned(),
                                    command: "????????????".to_owned(),
                                    arguments: Some(vec![serde_json::Value::Null]),
                                }),
                                ..lsp::CompletionItem::default()
                            },
                        ])))
                    },
                );
                fake_server.set_request_handler::<lsp::request::ExecuteCommand, _, _>({
                    let command_calls = closure_command_calls.clone();
                    move |params, _| {
                        assert_eq!(params.command, registered_command);
                        let command_calls = command_calls.clone();
                        async move {
                            command_calls.fetch_add(1, atomic::Ordering::Release);
                            Ok(Some(json!(null)))
                        }
                    }
                });
            })),
            ..FakeLspAdapter::default()
        },
    );
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/a/main.rs")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let _fake_server = fake_servers.next().await.unwrap();
    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        cx.focus_self(window);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input(".", window, cx);
    });
    cx.run_until_parked();
    editor.update(cx, |editor, _| {
        assert!(editor.context_menu_visible());
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            let completion_labels = menu
                .completions
                .borrow()
                .iter()
                .map(|c| c.label.text.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                completion_labels,
                &["registered_command", "unregistered_command",],
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor
                .confirm_completion(&ConfirmCompletion::default(), window, cx)
                .unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        command_calls.load(atomic::Ordering::Acquire),
        1,
        "For completion with a registered command, Mav should send a command execution request",
    );

    editor.update_in(cx, |editor, window, cx| {
        cx.focus_self(window);
        editor.handle_input(".", window, cx);
    });
    cx.run_until_parked();
    editor.update(cx, |editor, _| {
        assert!(editor.context_menu_visible());
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            let completion_labels = menu
                .completions
                .borrow()
                .iter()
                .map(|c| c.label.text.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                completion_labels,
                &["registered_command", "unregistered_command",],
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });
    editor
        .update_in(cx, |editor, window, cx| {
            editor.context_menu_next(&Default::default(), window, cx);
            editor
                .confirm_completion(&ConfirmCompletion::default(), window, cx)
                .unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        command_calls.load(atomic::Ordering::Acquire),
        1,
        "For completion with an unregistered command, Mav should not send a command execution request",
    );
}

#[gpui::test]
async fn test_completion_reuse(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    let counter = Arc::new(AtomicUsize::new(0));
    cx.set_state("objˇ");
    cx.simulate_keystroke(".");

    // Initial completion request returns complete results
    let is_incomplete = false;
    handle_completion_request(
        "obj.|<>",
        vec!["a", "ab", "abc"],
        is_incomplete,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.ˇ");
    check_displayed_completions(vec!["a", "ab", "abc"], &mut cx);

    // Type "a" - filters existing completions
    cx.simulate_keystroke("a");
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.aˇ");
    check_displayed_completions(vec!["a", "ab", "abc"], &mut cx);

    // Type "b" - filters existing completions
    cx.simulate_keystroke("b");
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.abˇ");
    check_displayed_completions(vec!["ab", "abc"], &mut cx);

    // Type "c" - filters existing completions
    cx.simulate_keystroke("c");
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.abcˇ");
    check_displayed_completions(vec!["abc"], &mut cx);

    // Backspace to delete "c" - filters existing completions
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.abˇ");
    check_displayed_completions(vec!["ab", "abc"], &mut cx);

    // Moving cursor to the left dismisses menu.
    cx.update_editor(|editor, window, cx| {
        editor.move_left(&MoveLeft, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);
    cx.assert_editor_state("obj.aˇb");
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.context_menu_visible(), false);
    });

    // Type "b" - new request
    cx.simulate_keystroke("b");
    let is_incomplete = false;
    handle_completion_request(
        "obj.<ab|>a",
        vec!["ab", "abc"],
        is_incomplete,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 2);
    cx.assert_editor_state("obj.abˇb");
    check_displayed_completions(vec!["ab", "abc"], &mut cx);

    // Backspace to delete "b" - since query was "ab" and is now "a", new request is made.
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
    });
    let is_incomplete = false;
    handle_completion_request(
        "obj.<a|>b",
        vec!["a", "ab", "abc"],
        is_incomplete,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 3);
    cx.assert_editor_state("obj.aˇb");
    check_displayed_completions(vec!["a", "ab", "abc"], &mut cx);

    // Backspace to delete "a" - dismisses menu.
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(counter.load(atomic::Ordering::Acquire), 3);
    cx.assert_editor_state("obj.ˇb");
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.context_menu_visible(), false);
    });
}

fn gen_text_edit(params: &CompletionParams, text: &str) -> Option<lsp::CompletionTextEdit> {
    let position = || lsp::Position {
        line: params.text_document_position.position.line,
        character: params.text_document_position.position.character,
    };
    Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
        range: lsp::Range {
            start: position(),
            end: position(),
        },
        new_text: text.to_string(),
    }))
}
