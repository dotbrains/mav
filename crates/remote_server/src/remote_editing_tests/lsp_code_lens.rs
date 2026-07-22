use super::*;

#[gpui::test]
async fn test_remote_code_lens_fetch_after_lsp_starts(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
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
        code_lens_provider: Some(lsp::CodeLensOptions {
            resolve_provider: None,
        }),
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

    // Prime the code lens cache while no LSP server exists on the host.
    // This simulates the race where the editor fetches code lenses during
    // initial paint before the language server has started.
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let initial_actions = lsp_store
        .update(cx, |lsp_store, cx| lsp_store.code_lens_actions(&buffer, cx))
        .await
        .unwrap();
    assert_eq!(
        initial_actions.map(|a| a.len()),
        Some(0),
        "Before any LSP starts, code lenses should be empty"
    );

    // Now register the LSP on the host. This triggers server startup and
    // buffer registration, propagating capabilities to the client.
    server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("rust-analyzer".into()),
            capabilities,
            Some(Box::new(|fake_lsp| {
                fake_lsp.set_request_handler::<lsp::request::CodeLensRequest, _, _>(
                    |_, _| async move {
                        Ok(Some(vec![lsp::CodeLens {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 0),
                                lsp::Position::new(0, 9),
                            ),
                            command: Some(lsp::Command {
                                title: "1 reference".to_string(),
                                command: "lens_cmd".to_string(),
                                arguments: None,
                            }),
                            data: None,
                        }]))
                    },
                );
            })),
        );
    });

    // Trigger re-evaluation of language servers for the already-open buffer.
    server_cx.update_entity(&headless, |headless, cx| {
        headless.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.restart_all_language_servers(cx);
        });
    });
    cx.run_until_parked();

    // A subsequent fetch must bypass the stale (empty) cache now that a
    // new server is available.
    let actions = lsp_store
        .update(cx, |lsp_store, cx| lsp_store.code_lens_actions(&buffer, cx))
        .await
        .unwrap();
    let actions = actions.expect("Should have code lens actions after LSP starts");
    assert_eq!(
        actions.len(),
        1,
        "Should have fetched one code lens from the newly started LSP"
    );
    assert_eq!(
        actions.values().next().unwrap().lsp_action.title(),
        "1 reference",
    );
}

#[gpui::test]
async fn test_remote_code_lens_resolve(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        editor::init(cx);
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.code_lens = Some(settings::CodeLens::Menu);
            });
        });
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
        code_lens_provider: Some(lsp::CodeLensOptions {
            resolve_provider: Some(true),
        }),
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
                fake_lsp.set_request_handler::<lsp::request::CodeLensRequest, _, _>(
                    |_, _| async move {
                        Ok(Some(vec![lsp::CodeLens {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 0),
                                lsp::Position::new(0, 9),
                            ),
                            command: None,
                            data: Some(serde_json::json!({ "id": "lens" })),
                        }]))
                    },
                );
                fake_lsp.set_request_handler::<lsp::request::CodeLensResolve, _, _>(
                    |lens, _| async move {
                        Ok(lsp::CodeLens {
                            command: Some(lsp::Command {
                                title: "1 reference".to_string(),
                                command: "noop".to_string(),
                                arguments: None,
                            }),
                            ..lens
                        })
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

    editor.update(cx, |editor, _| {
        assert!(editor.context_menu_visible());
        let menu = editor.context_menu().borrow();
        let actions_menu = match menu.as_ref() {
            Some(CodeContextMenu::CodeActions(m)) => m,
            _ => panic!("Expected code actions menu to be visible"),
        };
        let item = actions_menu
            .actions
            .get(0)
            .expect("Expected at least one item in menu");
        assert_eq!(item.label(), "1 reference");
    });
}
