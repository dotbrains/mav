use super::*;

#[gpui::test]
async fn test_ssh_collaboration_git_worktrees(
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
    remote_fs
        .insert_tree("/project", json!({ ".git": {}, "file.txt": "content" }))
        .await;

    server_cx.update(HeadlessProject::init);
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    let headless_project = server_cx.new(|cx| {
        HeadlessProject::new(
            HeadlessAppState {
                session: server_ssh,
                fs: remote_fs.clone(),
                http_client: Arc::new(BlockedHttpClient),
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
    let (project_a, _) = client_a
        .build_ssh_project("/project", client_ssh, false, cx_a)
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repo, _| repo.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worktrees.len(), 1);

    let worktree_directory = PathBuf::from("/worktrees");
    cx_b.update(|cx| {
        repo_b.update(cx, |repo, _| {
            repo.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "feature-branch".to_string(),
                    base_sha: Some("abc123".to_string()),
                },
                worktree_directory.join("feature-branch"),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repo, _| repo.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[1].path, worktree_directory.join("feature-branch"));
    assert_eq!(
        worktrees[1].ref_name,
        Some("refs/heads/feature-branch".into())
    );
    assert_eq!(worktrees[1].sha.as_ref(), "abc123");

    let server_worktrees = {
        let server_repo = server_cx.update(|cx| {
            headless_project.update(cx, |headless_project, cx| {
                headless_project
                    .git_store
                    .read(cx)
                    .repositories()
                    .values()
                    .next()
                    .unwrap()
                    .clone()
            })
        });
        server_cx
            .update(|cx| server_repo.update(cx, |repo, _| repo.worktrees()))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(server_worktrees.len(), 2);
    assert_eq!(
        server_worktrees[1].path,
        worktree_directory.join("feature-branch")
    );

    // Host (client A) renames the worktree via SSH
    let repo_a = cx_a.update(|cx| {
        project_a
            .read(cx)
            .repositories(cx)
            .values()
            .next()
            .unwrap()
            .clone()
    });
    cx_a.update(|cx| {
        repo_a.update(cx, |repository, _| {
            repository.rename_worktree(
                PathBuf::from("/worktrees/feature-branch"),
                PathBuf::from("/worktrees/renamed-branch"),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_worktrees = cx_a
        .update(|cx| repo_a.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        host_worktrees.len(),
        2,
        "Host should still have 2 worktrees after rename"
    );
    assert_eq!(
        host_worktrees[1].path,
        PathBuf::from("/worktrees/renamed-branch")
    );

    let server_worktrees = {
        let server_repo = server_cx.update(|cx| {
            headless_project.update(cx, |headless_project, cx| {
                headless_project
                    .git_store
                    .read(cx)
                    .repositories()
                    .values()
                    .next()
                    .unwrap()
                    .clone()
            })
        });
        server_cx
            .update(|cx| server_repo.update(cx, |repo, _| repo.worktrees()))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        server_worktrees.len(),
        2,
        "Server should still have 2 worktrees after rename"
    );
    assert_eq!(
        server_worktrees[1].path,
        PathBuf::from("/worktrees/renamed-branch")
    );

    // Host (client A) removes the renamed worktree via SSH
    cx_a.update(|cx| {
        repo_a.update(cx, |repository, _| {
            repository.remove_worktree(PathBuf::from("/worktrees/renamed-branch"), false)
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_worktrees = cx_a
        .update(|cx| repo_a.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        host_worktrees.len(),
        1,
        "Host should only have the main worktree after removal"
    );

    let server_worktrees = {
        let server_repo = server_cx.update(|cx| {
            headless_project.update(cx, |headless_project, cx| {
                headless_project
                    .git_store
                    .read(cx)
                    .repositories()
                    .values()
                    .next()
                    .unwrap()
                    .clone()
            })
        });
        server_cx
            .update(|cx| server_repo.update(cx, |repo, _| repo.worktrees()))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        server_worktrees.len(),
        1,
        "Server should only have the main worktree after removal"
    );
}
