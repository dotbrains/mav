use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_linked_worktrees_sync(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    // Set up a git repo with two linked worktrees already present.
    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({ ".git": {}, "file.txt": "content" }),
        )
        .await;

    let fs = client_a.fs();
    fs.add_linked_worktree_for_repo(
        Path::new(path!("/project/.git")),
        true,
        GitWorktree {
            path: PathBuf::from(path!("/worktrees/feature-branch")),
            ref_name: Some("refs/heads/feature-branch".into()),
            sha: "bbb222".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new(path!("/project/.git")),
        true,
        GitWorktree {
            path: PathBuf::from(path!("/worktrees/bugfix-branch")),
            ref_name: Some("refs/heads/bugfix-branch".into()),
            sha: "ccc333".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;

    // Wait for git scanning to complete on the host.
    executor.run_until_parked();

    // Verify the host sees 2 linked worktrees (main worktree is filtered out).
    let host_linked = project_a.read_with(cx_a, |project, cx| {
        let repos = project.repositories(cx);
        assert_eq!(repos.len(), 1, "host should have exactly 1 repository");
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        host_linked.len(),
        2,
        "host should have 2 linked worktrees (main filtered out)"
    );
    assert_eq!(
        host_linked[0].path,
        PathBuf::from(path!("/worktrees/bugfix-branch"))
    );
    assert_eq!(
        host_linked[0].ref_name,
        Some("refs/heads/bugfix-branch".into())
    );
    assert_eq!(host_linked[0].sha.as_ref(), "ccc333");
    assert_eq!(
        host_linked[1].path,
        PathBuf::from(path!("/worktrees/feature-branch"))
    );
    assert_eq!(
        host_linked[1].ref_name,
        Some("refs/heads/feature-branch".into())
    );
    assert_eq!(host_linked[1].sha.as_ref(), "bbb222");

    // Share the project and have client B join.
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    // Verify the guest sees the same linked worktrees as the host.
    let guest_linked = project_b.read_with(cx_b, |project, cx| {
        let repos = project.repositories(cx);
        assert_eq!(repos.len(), 1, "guest should have exactly 1 repository");
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        guest_linked, host_linked,
        "guest's linked_worktrees should match host's after initial sync"
    );

    // Now mutate: add a third linked worktree on the host side.
    client_a
        .fs()
        .add_linked_worktree_for_repo(
            Path::new(path!("/project/.git")),
            true,
            GitWorktree {
                path: PathBuf::from(path!("/worktrees/hotfix-branch")),
                ref_name: Some("refs/heads/hotfix-branch".into()),
                sha: "ddd444".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;

    // Wait for the host to re-scan and propagate the update.
    executor.run_until_parked();

    // Verify host now sees 3 linked worktrees.
    let host_linked_updated = project_a.read_with(cx_a, |project, cx| {
        let repos = project.repositories(cx);
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        host_linked_updated.len(),
        3,
        "host should now have 3 linked worktrees"
    );
    assert_eq!(
        host_linked_updated[2].path,
        PathBuf::from(path!("/worktrees/hotfix-branch"))
    );

    // Verify the guest also received the update.
    let guest_linked_updated = project_b.read_with(cx_b, |project, cx| {
        let repos = project.repositories(cx);
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        guest_linked_updated, host_linked_updated,
        "guest's linked_worktrees should match host's after update"
    );

    // Now mutate: remove one linked worktree from the host side.
    client_a
        .fs()
        .remove_worktree_for_repo(
            Path::new(path!("/project/.git")),
            true,
            "refs/heads/bugfix-branch",
        )
        .await;

    executor.run_until_parked();

    // Verify host now sees 2 linked worktrees (feature-branch and hotfix-branch).
    let (host_linked_after_removal, host_git_paths_after_removal) =
        project_a.read_with(cx_a, |project, cx| {
            let repos = project.repositories(cx);
            let repo = repos.values().next().unwrap();
            let repo = repo.read(cx);
            (
                repo.linked_worktrees().to_vec(),
                (
                    repo.repository_dir_abs_path.to_path_buf(),
                    repo.common_dir_abs_path.to_path_buf(),
                ),
            )
        });
    assert_eq!(
        host_linked_after_removal.len(),
        2,
        "host should have 2 linked worktrees after removal"
    );
    assert!(
        host_linked_after_removal
            .iter()
            .all(|wt| wt.ref_name != Some("refs/heads/bugfix-branch".into())),
        "bugfix-branch should have been removed"
    );

    // Verify the guest also reflects the removal.
    let guest_linked_after_removal = project_b.read_with(cx_b, |project, cx| {
        let repos = project.repositories(cx);
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        guest_linked_after_removal, host_linked_after_removal,
        "guest's linked_worktrees should match host's after removal"
    );

    // Test DB roundtrip: client C joins late, getting state from the database.
    // This verifies that linked_worktrees are persisted and restored correctly.
    let project_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    let late_joiner_linked = project_c.read_with(cx_c, |project, cx| {
        let repos = project.repositories(cx);
        assert_eq!(
            repos.len(),
            1,
            "late joiner should have exactly 1 repository"
        );
        let repo = repos.values().next().unwrap();
        repo.read(cx).linked_worktrees().to_vec()
    });
    assert_eq!(
        late_joiner_linked, host_linked_after_removal,
        "late-joining client's linked_worktrees should match host's (DB roundtrip)"
    );
    let late_joiner_git_paths = project_c.read_with(cx_c, |project, cx| {
        let repos = project.repositories(cx);
        let repo = repos.values().next().unwrap();
        let repo = repo.read(cx);
        (
            repo.repository_dir_abs_path.to_path_buf(),
            repo.common_dir_abs_path.to_path_buf(),
        )
    });
    assert_eq!(
        late_joiner_git_paths, host_git_paths_after_removal,
        "late-joining client's git directory paths should match host's (DB roundtrip)"
    );

    // Test reconnection: disconnect client B (guest) and reconnect.
    // After rejoining, client B should get linked_worktrees back from the DB.
    server.disconnect_client(client_b.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT);
    executor.run_until_parked();

    // Client B reconnects automatically.
    executor.advance_clock(RECEIVE_TIMEOUT);
    executor.run_until_parked();

    // Verify client B still has the correct linked worktrees after reconnection.
    let (guest_linked_after_reconnect, guest_git_paths_after_reconnect) =
        project_b.read_with(cx_b, |project, cx| {
            let repos = project.repositories(cx);
            assert_eq!(
                repos.len(),
                1,
                "guest should still have exactly 1 repository after reconnect"
            );
            let repo = repos.values().next().unwrap();
            let repo = repo.read(cx);
            (
                repo.linked_worktrees().to_vec(),
                (
                    repo.repository_dir_abs_path.to_path_buf(),
                    repo.common_dir_abs_path.to_path_buf(),
                ),
            )
        });
    assert_eq!(
        guest_linked_after_reconnect, host_linked_after_removal,
        "guest's linked_worktrees should survive guest disconnect/reconnect"
    );
    assert_eq!(
        guest_git_paths_after_reconnect, host_git_paths_after_removal,
        "guest's git directory paths should survive guest disconnect/reconnect"
    );
}
