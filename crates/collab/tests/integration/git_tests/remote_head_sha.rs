use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_remote_git_head_sha(
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
    let local_head_sha = cx_a.update(|cx| {
        project_a
            .read(cx)
            .active_repository(cx)
            .unwrap()
            .update(cx, |repository, _| repository.head_sha())
    });
    let local_head_sha = local_head_sha.await.unwrap().unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    let remote_head_sha = cx_b.update(|cx| {
        project_b
            .read(cx)
            .active_repository(cx)
            .unwrap()
            .update(cx, |repository, _| repository.head_sha())
    });
    let remote_head_sha = remote_head_sha.await.unwrap();

    assert_eq!(remote_head_sha.unwrap(), local_head_sha);
}
