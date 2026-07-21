use super::*;

#[gpui::test]
async fn test_checkpoint_basic(cx: &mut TestAppContext) {
    disable_git_global_config();

    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();

    git_init_repo(repo_dir.path());
    let file_path = repo_dir.path().join("file");
    smol::fs::write(&file_path, "initial").await.unwrap();

    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    repo.stage_paths(vec![repo_path("file")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Initial commit".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    smol::fs::write(&file_path, "modified before checkpoint")
        .await
        .unwrap();
    smol::fs::write(repo_dir.path().join("new_file_before_checkpoint"), "1")
        .await
        .unwrap();
    let checkpoint = repo.checkpoint().await.unwrap();

    // Ensure the user can't see any branches after creating a checkpoint.
    assert_eq!(repo.branches().await.unwrap().branches.len(), 1);

    smol::fs::write(&file_path, "modified after checkpoint")
        .await
        .unwrap();
    repo.stage_paths(vec![repo_path("file")], Arc::new(HashMap::default()))
        .await
        .unwrap();
    repo.commit(
        "Commit after checkpoint".into(),
        None,
        CommitOptions::default(),
        AskPassDelegate::new(&mut cx.to_async(), |_, _, _| {}),
        Arc::new(test_commit_envs()),
    )
    .await
    .unwrap();

    smol::fs::remove_file(repo_dir.path().join("new_file_before_checkpoint"))
        .await
        .unwrap();
    smol::fs::write(repo_dir.path().join("new_file_after_checkpoint"), "2")
        .await
        .unwrap();

    // Ensure checkpoint stays alive even after a Git GC.
    repo.gc().await.unwrap();
    repo.restore_checkpoint(checkpoint.clone()).await.unwrap();

    assert_eq!(
        smol::fs::read_to_string(&file_path).await.unwrap(),
        "modified before checkpoint"
    );
    assert_eq!(
        smol::fs::read_to_string(repo_dir.path().join("new_file_before_checkpoint"))
            .await
            .unwrap(),
        "1"
    );
    // See TODO above
    // assert_eq!(
    //     smol::fs::read_to_string(repo_dir.path().join("new_file_after_checkpoint"))
    //         .await
    //         .ok(),
    //     None
    // );
}

#[gpui::test]
async fn test_checkpoint_empty_repo(cx: &mut TestAppContext) {
    disable_git_global_config();

    cx.executor().allow_parking();

    let repo_dir = tempfile::tempdir().unwrap();
    git_init_repo(repo_dir.path());
    let repo = RealGitRepository::new(
        &repo_dir.path().join(".git"),
        None,
        Some("git".into()),
        cx.executor(),
    )
    .unwrap();

    smol::fs::write(repo_dir.path().join("foo"), "foo")
        .await
        .unwrap();
    let checkpoint_sha = repo.checkpoint().await.unwrap();

    // Ensure the user can't see any branches after creating a checkpoint.
    assert_eq!(repo.branches().await.unwrap().branches.len(), 1);

    smol::fs::write(repo_dir.path().join("foo"), "bar")
        .await
        .unwrap();
    smol::fs::write(repo_dir.path().join("baz"), "qux")
        .await
        .unwrap();
    repo.restore_checkpoint(checkpoint_sha).await.unwrap();
    assert_eq!(
        smol::fs::read_to_string(repo_dir.path().join("foo"))
            .await
            .unwrap(),
        "foo"
    );
    // See TODOs above
    // assert_eq!(
    //     smol::fs::read_to_string(repo_dir.path().join("baz"))
    //         .await
    //         .ok(),
    //     None
    // );
}
