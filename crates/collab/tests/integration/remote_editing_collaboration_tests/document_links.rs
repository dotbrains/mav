use super::*;

#[gpui::test]
async fn test_ssh_document_links_resolve(
    cx_a: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        project::trusted_worktrees::init(HashMap::default(), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        project::trusted_worktrees::init(HashMap::default(), cx);
    });

    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;

    let document_link_count = Arc::new(AtomicUsize::new(0));
    let resolve_count = Arc::new(AtomicUsize::new(0));

    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(
            path!("/code"),
            json!({
                "main.rs": "// see LICENSE for details\nfn main() {}",
                "other.rs": "fn other() {}\n",
            }),
        )
        .await;

    server_cx.update(HeadlessProject::init);
    let remote_http_client = Arc::new(BlockedHttpClient);
    let node = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    languages.add(rust_lang());

    let capabilities = lsp::ServerCapabilities {
        document_link_provider: Some(lsp::DocumentLinkOptions {
            resolve_provider: Some(true),
            work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
        }),
        ..lsp::ServerCapabilities::default()
    };
    let other_path_for_remote = path!("/code/other.rs");
    let mut fake_language_servers = languages.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            initializer: Some(Box::new({
                let document_link_count = document_link_count.clone();
                let resolve_count = resolve_count.clone();
                move |fake_server| {
                    let document_link_count = document_link_count.clone();
                    fake_server.set_request_handler::<lsp::request::DocumentLinkRequest, _, _>({
                        move |_params, _| {
                            let document_link_count = document_link_count.clone();
                            async move {
                                document_link_count.fetch_add(1, Ordering::Release);
                                Ok(Some(vec![lsp::DocumentLink {
                                    range: lsp::Range {
                                        start: lsp::Position {
                                            line: 0,
                                            character: 7,
                                        },
                                        end: lsp::Position {
                                            line: 0,
                                            character: 14,
                                        },
                                    },
                                    target: None,
                                    tooltip: None,
                                    data: Some(serde_json::json!({"id": 7})),
                                }]))
                            }
                        }
                    });
                    let resolve_count = resolve_count.clone();
                    fake_server.set_request_handler::<lsp::request::DocumentLinkResolve, _, _>({
                        move |link, _| {
                            let resolve_count = resolve_count.clone();
                            async move {
                                resolve_count.fetch_add(1, Ordering::Release);
                                Ok(lsp::DocumentLink {
                                    range: link.range,
                                    target: Some(
                                        lsp::Uri::from_file_path(other_path_for_remote).unwrap(),
                                    ),
                                    tooltip: Some("Open other.rs".into()),
                                    data: None,
                                })
                            }
                        }
                    });
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
    let (project_a, worktree_id) = client_a
        .build_ssh_project(path!("/code"), client_ssh.clone(), true, cx_a)
        .await;

    cx_a.run_until_parked();
    let trusted_worktrees =
        cx_a.update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global"));
    let worktree_store = project_a.read_with(cx_a, |project, _| project.worktree_store());
    trusted_worktrees.update(cx_a, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    cx_a.run_until_parked();

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.lsp_document_links = Some(true);
            });
        });
    });

    project_a.update(cx_a, |project, _| {
        project.languages().add(rust_lang());
        project.languages().register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                capabilities,
                ..FakeLspAdapter::default()
            },
        );
    });

    let (buffer, _registration) = project_a
        .update(cx_a, |project, cx| {
            project.open_buffer_with_lsp((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_id = buffer.read_with(cx_a, |buffer, _| buffer.remote_id());
    cx_a.run_until_parked();
    let _fake_language_server = fake_language_servers.next().await.unwrap();
    cx_a.run_until_parked();

    let (editor, cx_a) = cx_a.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)),
            Some(project_a.clone()),
            window,
            cx,
        )
    });
    cx_a.executor()
        .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT + Duration::from_millis(100));
    cx_a.run_until_parked();

    let fetched = project_a.read_with(cx_a, |project, cx| {
        project
            .lsp_store()
            .read(cx)
            .document_links_for_buffer(buffer_id)
            .unwrap_or_default()
    });
    assert_eq!(
        fetched.values().map(|links| links.len()).sum::<usize>(),
        1,
        "Editor should auto-pull a single document link via SSH"
    );
    assert!(
        document_link_count.load(Ordering::Acquire) >= 1,
        "Remote LSP should have served the fetch request"
    );

    let unresolved = fetched
        .values()
        .flat_map(|per_server| per_server.values())
        .next()
        .expect("local cache should mirror the remote document link");
    assert!(
        !unresolved.resolved,
        "freshly fetched links must come back unresolved"
    );

    let anchor = buffer.read_with(cx_a, |buffer, _| buffer.anchor_after(10));
    let resolved = editor
        .update(cx_a, |editor, cx| {
            editor.document_links_at(buffer.clone(), anchor, cx)
        })
        .expect("editor should expose the cached document link at the cursor")
        .await;
    cx_a.run_until_parked();

    assert_eq!(
        resolved.len(),
        1,
        "Editor should surface exactly one resolved link at the cursor"
    );
    assert!(
        resolve_count.load(Ordering::Acquire) >= 1,
        "Local resolve should be forwarded over SSH and run on the remote LSP"
    );

    let other_uri = lsp::Uri::from_file_path(path!("/code/other.rs"))
        .unwrap()
        .to_string();
    let links = project_a.read_with(cx_a, |project, cx| {
        project
            .lsp_store()
            .read(cx)
            .document_links_for_buffer(buffer_id)
            .unwrap_or_default()
    });
    assert_eq!(
        1,
        links.values().map(|m| m.len()).sum::<usize>(),
        "Local cache should mirror the single document link"
    );
    let link = links
        .values()
        .flat_map(|per_server| per_server.values())
        .next()
        .expect("local cache should contain the mirrored link");
    assert_eq!(
        link.target.as_deref(),
        Some(other_uri.as_str()),
        "Local should see the file:// target resolved on the remote"
    );
    assert_eq!(link.tooltip.as_deref(), Some("Open other.rs"));

    let executor = cx_a.executor();
    client_ssh.update(cx_a, |a, _| {
        a.shutdown_processes(Some(proto::ShutdownRemoteServer {}), executor)
    });
}
