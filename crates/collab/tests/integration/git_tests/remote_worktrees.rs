use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_remote_git_worktrees(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({ ".git": {}, "file.txt": "content" }),
        )
        .await;

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    // Initially only the main worktree (the repo itself) should be present
    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].path, PathBuf::from(path!("/project")));

    // Client B creates a git worktree via the remote project
    let worktree_directory = PathBuf::from(path!("/project"));
    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _| {
            repository.create_worktree(
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

    // Client B lists worktrees — should see main + the one just created
    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worktrees.len(), 2);
    assert_eq!(worktrees[0].path, PathBuf::from(path!("/project")));
    assert_eq!(worktrees[1].path, worktree_directory.join("feature-branch"));
    assert_eq!(
        worktrees[1].ref_name,
        Some("refs/heads/feature-branch".into())
    );
    assert_eq!(worktrees[1].sha.as_ref(), "abc123");

    // Verify from the host side that the worktree was actually created
    let host_worktrees = {
        let repo_a = cx_a.update(|cx| {
            project_a
                .read(cx)
                .repositories(cx)
                .values()
                .next()
                .unwrap()
                .clone()
        });
        cx_a.update(|cx| repo_a.update(cx, |repository, _| repository.worktrees()))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(host_worktrees.len(), 2);
    assert_eq!(host_worktrees[0].path, PathBuf::from(path!("/project")));
    assert_eq!(
        host_worktrees[1].path,
        worktree_directory.join("feature-branch")
    );

    // Client B creates a second git worktree without an explicit commit
    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _| {
            repository.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "bugfix-branch".to_string(),
                    base_sha: None,
                },
                worktree_directory.join("bugfix-branch"),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    // Client B lists worktrees — should now have main + two created
    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worktrees.len(), 3);

    let feature_worktree = worktrees
        .iter()
        .find(|worktree| worktree.ref_name == Some("refs/heads/feature-branch".into()))
        .expect("should find feature-branch worktree");
    assert_eq!(
        feature_worktree.path,
        worktree_directory.join("feature-branch")
    );

    let bugfix_worktree = worktrees
        .iter()
        .find(|worktree| worktree.ref_name == Some("refs/heads/bugfix-branch".into()))
        .expect("should find bugfix-branch worktree");
    assert_eq!(
        bugfix_worktree.path,
        worktree_directory.join("bugfix-branch")
    );
    assert_eq!(bugfix_worktree.sha.as_ref(), "fake-sha");

    // Client B (guest) attempts to rename a worktree. This should fail
    // because worktree renaming is not forwarded through collab
    let rename_result = cx_b
        .update(|cx| {
            repo_b.update(cx, |repository, _| {
                repository.rename_worktree(
                    worktree_directory.join("feature-branch"),
                    worktree_directory.join("renamed-branch"),
                )
            })
        })
        .await
        .unwrap();
    assert!(
        rename_result.is_err(),
        "Guest should not be able to rename worktrees via collab"
    );

    executor.run_until_parked();

    // Verify worktrees are unchanged — still 3
    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        worktrees.len(),
        3,
        "Worktree count should be unchanged after failed rename"
    );

    // Client B (guest) attempts to remove a worktree. This should fail
    // because worktree removal is not forwarded through collab
    let remove_result = cx_b
        .update(|cx| {
            repo_b.update(cx, |repository, _| {
                repository.remove_worktree(worktree_directory.join("feature-branch"), false)
            })
        })
        .await
        .unwrap();
    assert!(
        remove_result.is_err(),
        "Guest should not be able to remove worktrees via collab"
    );

    executor.run_until_parked();

    // Verify worktrees are unchanged — still 3
    let worktrees = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.worktrees()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        worktrees.len(),
        3,
        "Worktree count should be unchanged after failed removal"
    );
}
