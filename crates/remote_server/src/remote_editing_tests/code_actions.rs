use super::*;

#[gpui::test]
async fn test_remote_apply_code_action_skips_unadvertised_command(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
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
            "languages": {"Rust":{"language_servers":["rust-analyzer"]}},
            "lsp": {
              "rust-analyzer": {
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
                ..Default::default()
            },
        )
    });

    // Register the fake LSP with an empty execute_command_provider and a handler that panics
    // if it is ever reached: commands not advertised by the server must be rejected by
    // `apply_code_action` before dispatching to the language server.
    let mut fake_lsp = server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("rust-analyzer".into()),
            lsp::ServerCapabilities {
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: Vec::new(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Some(Box::new(|fake| {
                fake.set_request_handler::<lsp::request::ExecuteCommand, _, _>(
                    |params, _| async move {
                        panic!(
                            "Unadvertised command {} must not reach the language server",
                            params.command
                        );
                    },
                );
            })),
        )
    });

    cx.run_until_parked();

    let worktree_id = project
        .update(cx, |project, cx| {
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

    let _fake_lsp = fake_lsp.next().await.unwrap();

    let server_id = server_cx.read(|cx| {
        *headless
            .read(cx)
            .lsp_store
            .read(cx)
            .as_local()
            .unwrap()
            .language_servers
            .keys()
            .next()
            .unwrap()
    });
    let buffer_id = cx.read(|cx| buffer.read(cx).remote_id());

    let action = project::CodeAction {
        server_id,
        range: language::Anchor::min_min_range_for_buffer(buffer_id),
        lsp_action: project::LspAction::Command(lsp::Command {
            title: "\u{25b6}\u{fe0e} Run Tests".into(),
            command: "rust-analyzer.runSingle".into(),
            arguments: Some(vec![json!({"label": "test-mod tests"})]),
        }),
        resolved: true,
    };

    let transaction = project
        .update(cx, |project, cx| {
            project.apply_code_action(buffer.clone(), action, true, cx)
        })
        .await
        .expect("Unadvertised command must not be forwarded to executeCommand");
    assert_eq!(transaction.0.len(), 0);
}

#[gpui::test]
async fn test_remote_restore_unstaged_hunk_clears_diff(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        editor::init(cx);
    });

    use editor::Editor;
    use gpui::VisualContext;

    let base_text = "
        fn one() -> usize {
            1
        }
    "
    .unindent();
    let modified_text = "
        fn one() -> usize {
            100
        }
    "
    .unindent();

    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "src": {
                    "lib.rs": modified_text
                },
            },
        }),
    )
    .await;
    fs.set_index_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", base_text.clone())],
    );
    fs.set_head_for_repo(
        Path::new(path!("/code/project1/.git")),
        &[("src/lib.rs", base_text.clone())],
        "deadbeef",
    );

    let (project, _headless) = init_test(&fs, cx, server_cx).await;
    let worktree_id = {
        let (worktree, _) = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/code/project1"), true, cx)
            })
            .await
            .unwrap();
        cx.update(|cx| worktree.read(cx).id())
    };
    cx.executor().run_until_parked();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();

    let cx = cx.add_empty_window();
    let editor = cx.new_window_entity(|window, cx| {
        Editor::for_buffer(buffer, Some(project.clone()), window, cx)
    });
    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks: Vec<_> = editor
            .diff_hunks_in_ranges(
                &[editor::Anchor::Min..editor::Anchor::Max],
                &snapshot.buffer_snapshot(),
            )
            .collect();
        assert!(!hunks.is_empty(), "should have diff hunks before restore");
    });

    cx.update_window_entity(&editor, |editor, window, cx| {
        editor.select_all(&editor::actions::SelectAll, window, cx);
        editor.git_restore(&git::Restore, window, cx);
    });
    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            snapshot.text(),
            base_text,
            "buffer text should match base after restoring all hunks"
        );

        let hunks: Vec<_> = editor
            .diff_hunks_in_ranges(&[editor::Anchor::Min..editor::Anchor::Max], &snapshot)
            .collect();
        assert!(hunks.is_empty(), "should have no diff hunks after restore");
    });
}

pub async fn init_test(
    server_fs: &Arc<FakeFs>,
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) -> (Entity<Project>, Entity<HeadlessProject>) {
    let server_fs = server_fs.clone();
    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    init_logger();

    let (opts, ssh_server_client, _) = RemoteClient::fake_server(cx, server_cx);
    let http_client = Arc::new(BlockedHttpClient);
    let node_runtime = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(cx.executor()));
    let proxy = Arc::new(ExtensionHostProxy::new());
    server_cx.update(HeadlessProject::init);
    let headless = server_cx.new(|cx| {
        HeadlessProject::new(
            crate::HeadlessAppState {
                session: ssh_server_client,
                fs: server_fs.clone(),
                http_client,
                node_runtime,
                languages,
                extension_host_proxy: proxy,
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let ssh = RemoteClient::connect_mock(opts, cx).await;
    let project = build_project(ssh, cx);
    project
        .update(cx, {
            let headless = headless.clone();
            |_, cx| cx.on_release(|_, _| drop(headless))
        })
        .detach();
    (project, headless)
}
