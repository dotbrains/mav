use super::*;

#[gpui::test]
async fn test_remote_lsp(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
                "src": {
                    "lib.rs": "fn one() -> usize { 1 }"
                }
            },
        }),
    )
    .await;

    let (project, headless) = init_test(&fs, cx, server_cx).await;

    fs.insert_tree(
        path!("/code/project1/.mav"),
        json!({
            "settings.json": r#"
          {
            "languages": {"Rust":{"language_servers":["rust-analyzer", "fake-analyzer"]}},
            "lsp": {
              "rust-analyzer": {
                "binary": {
                  "path": "~/.cargo/bin/rust-analyzer"
                }
              },
              "fake-analyzer": {
               "binary": {
                "path": "~/.cargo/bin/rust-analyzer"
               }
              }
            }
          }"#
        }),
    )
    .await;

    cx.update_entity(&project, |project, _| {
        project.languages().register_test_language(LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".into()],
                ..Default::default()
            },
            ..Default::default()
        });
        project.languages().register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                name: "rust-analyzer",
                capabilities: lsp::ServerCapabilities {
                    completion_provider: Some(lsp::CompletionOptions::default()),
                    rename_provider: Some(lsp::OneOf::Left(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        );
        project.languages().register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                name: "fake-analyzer",
                capabilities: lsp::ServerCapabilities {
                    completion_provider: Some(lsp::CompletionOptions::default()),
                    rename_provider: Some(lsp::OneOf::Left(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        )
    });

    let mut fake_lsp = server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("rust-analyzer".into()),
            lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions::default()),
                rename_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            None,
        )
    });

    let mut fake_second_lsp = server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                name: "fake-analyzer",
                capabilities: lsp::ServerCapabilities {
                    completion_provider: Some(lsp::CompletionOptions::default()),
                    rename_provider: Some(lsp::OneOf::Left(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        );
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("fake-analyzer".into()),
            lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions::default()),
                rename_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            None,
        )
    });

    cx.run_until_parked();

    let worktree_id = project
        .update(cx, |project, cx| {
            project.languages().add(rust_lang());
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap()
        .0
        .read_with(cx, |worktree, _| worktree.id());

    // Wait for the settings to synchronize
    cx.run_until_parked();

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_buffer_with_lsp((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let fake_lsp = fake_lsp.next().await.unwrap();
    let fake_second_lsp = fake_second_lsp.next().await.unwrap();

    cx.read(|cx| {
        assert_eq!(
            LanguageSettings::for_buffer(buffer.read(cx), cx).language_servers,
            ["rust-analyzer".to_string(), "fake-analyzer".to_string()]
        )
    });

    let buffer_id = cx.read(|cx| {
        let buffer = buffer.read(cx);
        assert_eq!(buffer.language().unwrap().name(), "Rust");
        buffer.remote_id()
    });

    server_cx.read(|cx| {
        let buffer = headless
            .read(cx)
            .buffer_store
            .read(cx)
            .get(buffer_id)
            .unwrap();

        assert_eq!(buffer.read(cx).language().unwrap().name(), "Rust");
    });

    server_cx.read(|cx| {
        let lsp_store = headless.read(cx).lsp_store.read(cx);
        assert_eq!(lsp_store.as_local().unwrap().language_servers.len(), 2);
    });

    fake_lsp.set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
        Ok(Some(CompletionResponse::Array(vec![lsp::CompletionItem {
            label: "boop".to_string(),
            ..Default::default()
        }])))
    });

    fake_second_lsp.set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
        Ok(Some(CompletionResponse::Array(vec![lsp::CompletionItem {
            label: "beep".to_string(),
            ..Default::default()
        }])))
    });

    let result = project
        .update(cx, |project, cx| {
            project.completions(
                &buffer,
                0,
                CompletionContext {
                    trigger_kind: CompletionTriggerKind::INVOKED,
                    trigger_character: None,
                },
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        result
            .into_iter()
            .flat_map(|response| response.completions)
            .map(|c| c.label.text)
            .collect::<Vec<_>>(),
        vec!["boop".to_string(), "beep".to_string()]
    );

    fake_lsp.set_request_handler::<lsp::request::Rename, _, _>(|_, _| async move {
        Ok(Some(lsp::WorkspaceEdit {
            changes: Some(
                [(
                    lsp::Uri::from_file_path(path!("/code/project1/src/lib.rs")).unwrap(),
                    vec![lsp::TextEdit::new(
                        lsp::Range::new(lsp::Position::new(0, 3), lsp::Position::new(0, 6)),
                        "two".to_string(),
                    )],
                )]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        }))
    });

    project
        .update(cx, |project, cx| {
            project.perform_rename(buffer.clone(), 3, "two".to_string(), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();
    buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), "fn two() -> usize { 1 }")
    })
}

#[gpui::test]
async fn test_remote_code_action_resolve(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        editor::init(cx);
    });

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "src": {
                    "lib.rs": "fn one() -> usize { 1 }"
                }
            },
        }),
    )
    .await;

    let (project, headless) = init_test(&fs, cx, server_cx).await;

    let capabilities = lsp::ServerCapabilities {
        code_action_provider: Some(lsp::CodeActionProviderCapability::Options(
            lsp::CodeActionOptions {
                resolve_provider: Some(true),
                ..lsp::CodeActionOptions::default()
            },
        )),
        ..lsp::ServerCapabilities::default()
    };

    cx.update_entity(&project, |project, _| {
        project.languages().register_test_language(LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".into()],
                ..Default::default()
            },
            ..Default::default()
        });
        project.languages().register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                name: "rust-analyzer",
                capabilities: capabilities.clone(),
                ..FakeLspAdapter::default()
            },
        );
    });

    server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("rust-analyzer".into()),
            capabilities,
            Some(Box::new(|fake_lsp| {
                fake_lsp.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
                    |_, _| async move {
                        Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                            lsp::CodeAction {
                                title: "Use two".to_string(),
                                data: Some(serde_json::json!({ "id": "action" })),
                                ..lsp::CodeAction::default()
                            },
                        )]))
                    },
                );
                fake_lsp.set_request_handler::<lsp::request::CodeActionResolveRequest, _, _>(
                    |mut action, _| async move {
                        action.edit = Some(lsp::WorkspaceEdit {
                            changes: Some(
                                [(
                                    lsp::Uri::from_file_path(path!("/code/project1/src/lib.rs"))
                                        .unwrap(),
                                    vec![lsp::TextEdit::new(
                                        lsp::Range::new(
                                            lsp::Position::new(0, 3),
                                            lsp::Position::new(0, 6),
                                        ),
                                        "two".to_string(),
                                    )],
                                )]
                                .into_iter()
                                .collect(),
                            ),
                            ..lsp::WorkspaceEdit::default()
                        });
                        Ok(action)
                    },
                );
            })),
        );
    });

    cx.run_until_parked();

    let worktree_id = project
        .update(cx, |project, cx| {
            project.languages().add(rust_lang());
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap()
        .0
        .read_with(cx, |worktree, _| worktree.id());
    cx.run_until_parked();

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_buffer_with_lsp((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let cx = cx.add_empty_window();
    let workspace = cx.new_window_entity(|window, cx| {
        workspace::Workspace::test_new(project.clone(), window, cx)
    });
    let editor = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer.clone(), Some(project.clone()), window, cx)
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
    });
    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)]);
        });
    });
    cx.executor()
        .advance_clock(editor::CODE_ACTIONS_DEBOUNCE_TIMEOUT * 2);
    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    editor.update(cx, |editor, _| assert!(editor.context_menu_visible()));

    let confirm_action = editor
        .update_in(cx, |editor, window, cx| {
            Editor::confirm_code_action(editor, &ConfirmCodeAction { item_ix: Some(0) }, window, cx)
        })
        .unwrap();
    confirm_action.await.unwrap();

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "fn two() -> usize { 1 }");
    });
}
