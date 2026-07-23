use super::*;

#[gpui::test(iterations = 10)]
async fn test_sharing_an_ssh_remote_project(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let executor = cx_a.executor();
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

    // Set up project on remote FS
    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(
            path!("/code"),
            json!({
                "project1": {
                    ".mav": {
                        "settings.json": r#"{"languages":{"Rust":{"language_servers":["override-rust-analyzer"]}}}"#
                    },
                    "README.md": "# project 1",
                    "src": {
                        "lib.rs": "fn one() -> usize { 1 }"
                    }
                },
                "project2": {
                    "README.md": "# project 2",
                },
            }),
        )
        .await;

    // User A connects to the remote project via SSH.
    server_cx.update(HeadlessProject::init);
    let remote_http_client = Arc::new(BlockedHttpClient);
    let node = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    languages.add(rust_lang());
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
            false,
            cx,
        )
    });

    let client_ssh = RemoteClient::connect_mock(opts, cx_a).await;
    let (project_a, worktree_id) = client_a
        .build_ssh_project(path!("/code/project1"), client_ssh, false, cx_a)
        .await;

    // While the SSH worktree is being scanned, user A shares the remote project.
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // User B joins the project.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    project_b.update(cx_b, |project, _| project.languages().add(rust_lang()));
    let worktree_b = project_b
        .update(cx_b, |project, cx| project.worktree_for_id(worktree_id, cx))
        .unwrap();

    let worktree_a = project_a
        .update(cx_a, |project, cx| project.worktree_for_id(worktree_id, cx))
        .unwrap();

    executor.run_until_parked();

    worktree_a.update(cx_a, |worktree, _cx| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            vec![
                rel_path(".mav"),
                rel_path(".mav/settings.json"),
                rel_path("README.md"),
                rel_path("src"),
                rel_path("src/lib.rs"),
            ]
        );
    });

    worktree_b.update(cx_b, |worktree, _cx| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            vec![
                rel_path(".mav"),
                rel_path(".mav/settings.json"),
                rel_path("README.md"),
                rel_path("src"),
                rel_path("src/lib.rs"),
            ]
        );
    });

    // User B can open buffers in the remote project.
    let buffer_b = project_b
        .update(cx_b, |project, cx| {
            project.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    buffer_b.update(cx_b, |buffer, cx| {
        assert_eq!(buffer.text(), "fn one() -> usize { 1 }");
        let ix = buffer.text().find('1').unwrap();
        buffer.edit([(ix..ix + 1, "100")], None, cx);
    });

    executor.run_until_parked();

    cx_b.read(|cx| {
        assert_eq!(
            LanguageSettings::for_buffer(buffer_b.read(cx), cx).language_servers,
            ["override-rust-analyzer".to_string()]
        )
    });

    project_b
        .update(cx_b, |project, cx| {
            project.save_buffer_as(
                buffer_b.clone(),
                ProjectPath {
                    worktree_id: worktree_id.to_owned(),
                    path: rel_path("src/renamed.rs").into(),
                },
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        remote_fs
            .load(path!("/code/project1/src/renamed.rs").as_ref())
            .await
            .unwrap(),
        "fn one() -> usize { 100 }"
    );
    cx_b.run_until_parked();
    cx_b.update(|cx| {
        assert_eq!(
            buffer_b.read(cx).file().unwrap().path().as_ref(),
            rel_path("src/renamed.rs")
        );
    });
}
