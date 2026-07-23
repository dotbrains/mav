use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_remote_git_commit_data_batches(
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

    let commit_shas = [
        "0123456789abcdef0123456789abcdef01234567"
            .parse::<Oid>()
            .unwrap(),
        "1111111111111111111111111111111111111111"
            .parse::<Oid>()
            .unwrap(),
        "2222222222222222222222222222222222222222"
            .parse::<Oid>()
            .unwrap(),
        "3333333333333333333333333333333333333333"
            .parse::<Oid>()
            .unwrap(),
    ];

    client_a.fs().set_commit_data(
        Path::new(path!("/project/.git")),
        commit_shas.iter().enumerate().map(|(index, sha)| {
            (
                CommitData {
                    sha: *sha,
                    parents: Default::default(),
                    author_name: SharedString::from(format!("Author {index}")),
                    author_email: SharedString::from(format!("author{index}@example.com")),
                    commit_timestamp: 1_700_000_000 + index as i64,
                    subject: SharedString::from(format!("Subject {index}")),
                    message: SharedString::from(format!("Subject {index}\n\nBody {index}")),
                },
                false,
            )
        }),
    );

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;
    executor.run_until_parked();

    let repo_a = cx_a.update(|cx| project_a.read(cx).active_repository(cx).unwrap());

    let primed_before = load_commit_data_batch(&repo_a, &commit_shas[..2], &executor, cx_a).await;
    assert_eq!(
        primed_before.len(),
        2,
        "host should prime two commits before sharing"
    );

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    let remote_batch_one =
        load_commit_data_batch(&repo_b, &commit_shas[..3], &executor, cx_b).await;
    assert_eq!(remote_batch_one.len(), 3);
    for (index, sha) in commit_shas[..3].iter().enumerate() {
        let commit_data = remote_batch_one.get(sha).unwrap();
        assert_eq!(commit_data.sha, *sha);
        assert_eq!(commit_data.subject.as_ref(), format!("Subject {index}"));
        assert_eq!(
            commit_data.message.as_ref(),
            format!("Subject {index}\n\nBody {index}")
        );
    }

    let primed_after = load_commit_data_batch(&repo_a, &commit_shas[2..], &executor, cx_a).await;
    assert_eq!(
        primed_after.len(),
        2,
        "host should prime remaining commits after remote fetches"
    );

    let remote_batch_two =
        load_commit_data_batch(&repo_b, &commit_shas[1..], &executor, cx_b).await;
    assert_eq!(remote_batch_two.len(), 3);

    assert_remote_cache_matches_local_cache(&repo_a, &repo_b, cx_a, cx_b);
}
