use super::*;

#[gpui::test]
async fn test_ssh_collaboration_formatting_with_prettier(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    cx_a.set_name("a");
    cx_b.set_name("b");
    server_cx.set_name("server");

    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    let buffer_text = "let one = \"two\"";
    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    remote_fs
        .insert_tree(
            path!("/project"),
            serde_json::json!({ "a.ts": buffer_text }),
        )
        .await;

    let test_plugin = "test_plugin";
    let ts_lang = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));
    client_a.language_registry().add(ts_lang.clone());
    client_b.language_registry().add(ts_lang.clone());

    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    let mut fake_language_servers = languages.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    // User A connects to the remote project via SSH.
    server_cx.update(HeadlessProject::init);
    let remote_http_client = Arc::new(BlockedHttpClient);
    let _headless_project = server_cx.new(|cx| {
        HeadlessProject::new(
            HeadlessAppState {
                session: server_ssh,
                fs: remote_fs.clone(),
                http_client: remote_http_client,
                node_runtime: NodeRuntime::unavailable(),
                languages,
                extension_host_proxy: Arc::new(ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let client_ssh = RemoteClient::connect_mock(opts, cx_a).await;
    let (project_a, worktree_id) = client_a
        .build_ssh_project(path!("/project"), client_ssh, false, cx_a)
        .await;

    // While the SSH worktree is being scanned, user A shares the remote project.
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // User B joins the project.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    // Opens the buffer and formats it
    let (buffer_b, _handle) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("a.ts")), cx)
        })
        .await
        .expect("user B opens buffer for formatting");

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |file| {
                file.project.all_languages.defaults.format_on_save =
                    Some(settings::FormatOnSave::On);
                file.project.all_languages.defaults.formatter = Some(FormatterList::default());
                file.project.all_languages.defaults.prettier = Some(PrettierSettingsContent {
                    allowed: Some(true),
                    ..Default::default()
                });
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |file| {
                file.project.all_languages.defaults.format_on_save =
                    Some(settings::FormatOnSave::On);
                file.project.all_languages.defaults.formatter = Some(FormatterList::Single(
                    Formatter::LanguageServer(LanguageServerFormatterSpecifier::Current),
                ));
                file.project.all_languages.defaults.prettier = Some(PrettierSettingsContent {
                    allowed: Some(true),
                    ..Default::default()
                });
            });
        });
    });
    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>(|_, _| async move {
        panic!(
            "Unexpected: prettier should be preferred since it's enabled and language supports it"
        )
    });

    project_b
        .update(cx_b, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_b.clone()]),
                LspFormatTarget::Buffers,
                true,
                FormatTrigger::Save,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        buffer_text.to_string() + "\n" + prettier_format_suffix,
        "Prettier formatting was not applied to client buffer after client's request"
    );

    // User A opens and formats the same buffer too
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.ts")), cx)
        })
        .await
        .expect("user A opens buffer for formatting");

    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |file| {
                file.project.all_languages.defaults.formatter = Some(FormatterList::default());
                file.project.all_languages.defaults.prettier = Some(PrettierSettingsContent {
                    allowed: Some(true),
                    ..Default::default()
                });
            });
        });
    });
    project_a
        .update(cx_a, |project, cx| {
            project.format(
                HashSet::from_iter([buffer_a.clone()]),
                LspFormatTarget::Buffers,
                true,
                FormatTrigger::Manual,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        buffer_b.read_with(cx_b, |buffer, _| buffer.text()),
        buffer_text.to_string() + "\n" + prettier_format_suffix + "\n" + prettier_format_suffix,
        "Prettier formatting was not applied to client buffer after host's request"
    );
}
