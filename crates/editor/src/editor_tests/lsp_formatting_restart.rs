use super::*;

#[gpui::test]
async fn test_on_type_formatting_not_triggered(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
            "other.rs": "// Test file",
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "{".to_string(),
                    end: "}".to_string(),
                    close: true,
                    surround: true,
                    newline: true,
                }],
                disabled_scopes_by_bracket_ix: Vec::new(),
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                    first_trigger_character: "{".to_string(),
                    more_trigger_character: None,
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let cx = &mut VisualTestContext::from_window(*window, cx);

    let worktree_id = workspace.update_in(cx, |workspace, _, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor_handle = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();

    fake_server.set_request_handler::<lsp::request::OnTypeFormatting, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 21),
            );

            Ok(Some(vec![lsp::TextEdit {
                new_text: "]".to_string(),
                range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            }]))
        },
    );

    editor_handle.update_in(cx, |editor, window, cx| {
        window.focus(&editor.focus_handle(cx), cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 21)..Point::new(0, 20)])
        });
        editor.handle_input("{", window, cx);
    });

    cx.executor().run_until_parked();

    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "fn main() { let a = {5}; }",
            "No extra braces from on type formatting should appear in the buffer"
        )
    });
}

#[gpui::test(iterations = 20, seeds(31))]
async fn test_on_type_formatting_is_applied_after_autoindent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                first_trigger_character: ".".to_string(),
                more_trigger_character: None,
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.update_buffer(|buffer, _| {
        // This causes autoindent to be async.
        buffer.set_sync_parse_timeout(None)
    });

    cx.set_state("fn c() {\n    d()ˇ\n}\n");
    cx.simulate_keystroke("\n");
    cx.run_until_parked();

    let buffer_cloned = cx.multibuffer(|multi_buffer, _| multi_buffer.as_singleton().unwrap());
    let mut request =
        cx.set_request_handler::<lsp::request::OnTypeFormatting, _, _>(move |_, _, mut cx| {
            let buffer_cloned = buffer_cloned.clone();
            async move {
                buffer_cloned.update(&mut cx, |buffer, _| {
                    assert_eq!(
                        buffer.text(),
                        "fn c() {\n    d()\n        .\n}\n",
                        "OnTypeFormatting should triggered after autoindent applied"
                    )
                });

                Ok(Some(vec![]))
            }
        });

    cx.simulate_keystroke(".");
    cx.run_until_parked();

    cx.assert_editor_state("fn c() {\n    d()\n        .ˇ\n}\n");
    assert!(request.next().await.is_some());
    request.close();
    assert!(request.next().await.is_none());
}

#[gpui::test]
async fn test_language_server_restart_due_to_settings_change(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let server_restarts = Arc::new(AtomicUsize::new(0));
    let closure_restarts = Arc::clone(&server_restarts);
    let language_server_name = "test language server";
    let language_name: LanguageName = "Rust".into();

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: language_name.clone(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: language_server_name,
            initialization_options: Some(json!({
                "testOptionValue": true
            })),
            initializer: Some(Box::new(move |fake_server| {
                let task_restarts = Arc::clone(&closure_restarts);
                fake_server.set_request_handler::<lsp::request::Shutdown, _, _>(move |_, _| {
                    task_restarts.fetch_add(1, atomic::Ordering::Release);
                    futures::future::ready(Ok(()))
                });
            })),
            ..Default::default()
        },
    );

    let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let _fake_server = fake_servers.next().await.unwrap();
    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            language_name.clone().0.to_string(),
            LanguageSettingsContent {
                tab_size: NonZeroU32::new(8),
                ..Default::default()
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        0,
        "Should not restart LSP server on an unrelated change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            "Some other server name".into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "some other init value": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        0,
        "Should not restart LSP server on an unrelated LSP settings change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "anotherInitValue": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        1,
        "Should restart LSP server on a related LSP settings change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "anotherInitValue": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        1,
        "Should not restart LSP server on a related LSP settings change that is the same"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: None,
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        2,
        "Should restart LSP server on another related LSP settings change"
    );
}
