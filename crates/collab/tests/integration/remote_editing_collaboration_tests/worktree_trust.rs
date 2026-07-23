use super::*;

#[gpui::test]
async fn test_ssh_remote_worktree_trust(cx_a: &mut TestAppContext, server_cx: &mut TestAppContext) {
    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        project::trusted_worktrees::init(HashMap::default(), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        project::trusted_worktrees::init(HashMap::default(), cx);
    });

    let mut server = TestServer::start(cx_a.executor().clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;

    let server_name = "override-rust-analyzer";
    let lsp_inlay_hint_request_count = Arc::new(AtomicUsize::new(0));

    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(
            path!("/projects"),
            json!({
                "project_a": {
                    ".mav": {
                        "settings.json": r#"{"languages":{"Rust":{"language_servers":["override-rust-analyzer"]}}}"#
                    },
                    "main.rs": "fn main() {}"
                },
                "project_b": { "lib.rs": "pub fn lib() {}" }
            }),
        )
        .await;

    server_cx.update(HeadlessProject::init);
    let remote_http_client = Arc::new(BlockedHttpClient);
    let node = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    languages.add(rust_lang());

    let capabilities = lsp::ServerCapabilities {
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    let mut fake_language_servers = languages.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: server_name,
            capabilities: capabilities.clone(),
            initializer: Some(Box::new({
                let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                move |fake_server| {
                    let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |_params, _| {
                            lsp_inlay_hint_request_count.fetch_add(1, Ordering::Release);
                            async move {
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(0, 0),
                                    label: lsp::InlayHintLabel::String("hint".to_string()),
                                    kind: None,
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                }]))
                            }
                        },
                    );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );

    let _headless_project = server_cx.new(|cx| {
        HeadlessProject::new(
            HeadlessAppState {
                session: server_ssh,
                fs: remote_fs.clone(),
                http_client: remote_http_client,
                node_runtime: node,
                languages,
                extension_host_proxy: Arc::new(ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            true,
            cx,
        )
    });

    let client_ssh = RemoteClient::connect_mock(opts, cx_a).await;
    let (project_a, worktree_id_a) = client_a
        .build_ssh_project(path!("/projects/project_a"), client_ssh.clone(), true, cx_a)
        .await;

    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);

        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                let language_settings = &mut settings.project.all_languages.defaults;
                language_settings.inlay_hints = Some(InlayHintSettingsContent {
                    enabled: Some(true),
                    ..InlayHintSettingsContent::default()
                })
            });
        });
    });

    project_a
        .update(cx_a, |project, cx| {
            project.languages().add(rust_lang());
            project.languages().register_fake_lsp_adapter(
                "Rust",
                FakeLspAdapter {
                    name: server_name,
                    capabilities,
                    ..FakeLspAdapter::default()
                },
            );
            project.find_or_create_worktree(path!("/projects/project_b"), true, cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    let worktree_ids = project_a.read_with(cx_a, |project, cx| {
        project
            .worktrees(cx)
            .map(|wt| wt.read(cx).id())
            .collect::<Vec<_>>()
    });
    assert_eq!(worktree_ids.len(), 2);

    let trusted_worktrees =
        cx_a.update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global should exist"));
    let worktree_store = project_a.read_with(cx_a, |project, _| project.worktree_store());

    let can_trust_a = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(!can_trust_a, "project_a should be restricted initially");
    assert!(!can_trust_b, "project_b should be restricted initially");

    let has_restricted = trusted_worktrees.read_with(cx_a, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(has_restricted, "should have restricted worktrees");

    let buffer_before_approval = project_a
        .update(cx_a, |project, cx| {
            project.open_buffer((worktree_id_a, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx_a) = cx_a.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            cx.new(|cx| MultiBuffer::singleton(buffer_before_approval.clone(), cx)),
            Some(project_a.clone()),
            window,
            cx,
        )
    });
    cx_a.run_until_parked();
    let fake_language_server = fake_language_servers.next();

    cx_a.read(|cx| {
        assert_eq!(
            LanguageSettings::for_buffer(buffer_before_approval.read(cx), cx).language_servers,
            ["...".to_string()],
            "remote .mav/settings.json must not sync before trust approval"
        )
    });

    editor.update_in(cx_a, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx_a.run_until_parked();
    cx_a.executor().advance_clock(Duration::from_secs(1));
    assert_eq!(
        lsp_inlay_hint_request_count.load(Ordering::Acquire),
        0,
        "inlay hints must not be queried before trust approval"
    );

    trusted_worktrees.update(cx_a, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_ids[0])]),
            cx,
        );
    });
    cx_a.run_until_parked();

    cx_a.read(|cx| {
        assert_eq!(
            LanguageSettings::for_buffer(buffer_before_approval.read(cx), cx).language_servers,
            ["override-rust-analyzer".to_string()],
            "remote .mav/settings.json should sync after trust approval"
        )
    });
    let _fake_language_server = fake_language_server.await.unwrap();
    editor.update_in(cx_a, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx_a.run_until_parked();
    cx_a.executor().advance_clock(Duration::from_secs(1));
    assert!(
        lsp_inlay_hint_request_count.load(Ordering::Acquire) > 0,
        "inlay hints should be queried after trust approval"
    );

    let can_trust_a = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(can_trust_a, "project_a should be trusted after trust()");
    assert!(!can_trust_b, "project_b should still be restricted");

    trusted_worktrees.update(cx_a, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_ids[1])]),
            cx,
        );
    });

    let can_trust_a = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx_a, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(can_trust_a, "project_a should remain trusted");
    assert!(can_trust_b, "project_b should now be trusted");

    let has_restricted_after = trusted_worktrees.read_with(cx_a, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(
        !has_restricted_after,
        "should have no restricted worktrees after trusting both"
    );
}
