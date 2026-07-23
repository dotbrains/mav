use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_branch_list_sync(
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
    client_a.fs().insert_branches(
        Path::new(path!("/project/.git")),
        &["main", "feature-1", "feature-2"],
    );

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;
    executor.run_until_parked();

    let host_snapshot = branch_list_snapshot(&project_a, cx_a);
    assert_eq!(host_snapshot.0.as_deref(), Some("main"));
    assert_eq!(
        host_snapshot.1,
        vec![
            "refs/heads/feature-1".to_string(),
            "refs/heads/feature-2".to_string(),
            "refs/heads/main".to_string(),
        ]
    );

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.create_branch("totally-new-branch".to_string(), None)
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.change_branch("totally-new-branch".to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_snapshot_after_update = branch_list_snapshot(&project_a, cx_a);
    assert_eq!(
        host_snapshot_after_update.0.as_deref(),
        Some("totally-new-branch")
    );
    assert_eq!(
        host_snapshot_after_update.1,
        vec![
            "refs/heads/feature-1".to_string(),
            "refs/heads/feature-2".to_string(),
            "refs/heads/main".to_string(),
            "refs/heads/totally-new-branch".to_string(),
        ]
    );

    let guest_snapshot_after_update = branch_list_snapshot(&project_b, cx_b);
    assert_eq!(guest_snapshot_after_update, host_snapshot_after_update);
}
