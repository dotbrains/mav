use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_root_repo_common_dir_sync(
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

    // Set up a project whose root IS a git repository.
    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({ ".git": {}, "file.txt": "content" }),
        )
        .await;

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;
    executor.run_until_parked();

    // Host should see root_repo_common_dir pointing to .git at the root.
    let host_common_dir = project_a.read_with(cx_a, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        worktree.read(cx).snapshot().root_repo_common_dir().cloned()
    });
    assert_eq!(
        host_common_dir.as_deref(),
        Some(path::Path::new(path!("/project/.git"))),
    );

    // Share the project and have client B join.
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    // Guest should see the same root_repo_common_dir as the host.
    let guest_common_dir = project_b.read_with(cx_b, |project, cx| {
        let worktree = project.worktrees(cx).next().unwrap();
        worktree.read(cx).snapshot().root_repo_common_dir().cloned()
    });
    assert_eq!(
        guest_common_dir, host_common_dir,
        "guest should see the same root_repo_common_dir as host",
    );
}
