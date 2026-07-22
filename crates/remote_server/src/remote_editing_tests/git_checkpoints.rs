use super::*;

#[gpui::test]
async fn test_remote_git_checkpoints(cx: &mut TestAppContext, server_cx: &mut TestAppContext) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "file.txt": "original content",
            },
        }),
    )
    .await;

    let (project, _headless) = init_test(&fs, cx, server_cx).await;

    let (_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let repository = project.update(cx, |project, cx| project.active_repository(cx).unwrap());

    // 1. Create a checkpoint of the original state
    let checkpoint_1 = repository
        .update(cx, |repository, _| repository.checkpoint())
        .await
        .unwrap()
        .unwrap();

    // 2. Modify a file on the server-side fs
    fs.write(
        Path::new(path!("/code/project1/file.txt")),
        b"modified content",
    )
    .await
    .unwrap();

    // 3. Create a second checkpoint with the modified state
    let checkpoint_2 = repository
        .update(cx, |repository, _| repository.checkpoint())
        .await
        .unwrap()
        .unwrap();

    // 4. compare_checkpoints: same checkpoint with itself => equal
    let equal = repository
        .update(cx, |repository, _| {
            repository.compare_checkpoints(checkpoint_1.clone(), checkpoint_1.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(equal, "a checkpoint compared with itself should be equal");

    // 5. compare_checkpoints: different states => not equal
    let equal = repository
        .update(cx, |repository, _| {
            repository.compare_checkpoints(checkpoint_1.clone(), checkpoint_2.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(
        !equal,
        "checkpoints of different states should not be equal"
    );

    // 6. diff_checkpoints: same checkpoint => empty diff
    let diff = repository
        .update(cx, |repository, _| {
            repository.diff_checkpoints(checkpoint_1.clone(), checkpoint_1.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(
        diff.is_empty(),
        "diff of identical checkpoints should be empty"
    );

    // 7. diff_checkpoints: different checkpoints => non-empty diff mentioning the changed file
    let diff = repository
        .update(cx, |repository, _| {
            repository.diff_checkpoints(checkpoint_1.clone(), checkpoint_2.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(
        !diff.is_empty(),
        "diff of different checkpoints should be non-empty"
    );
    assert!(
        diff.contains("file.txt"),
        "diff should mention the changed file"
    );
    assert!(
        diff.contains("original content"),
        "diff should contain removed content"
    );
    assert!(
        diff.contains("modified content"),
        "diff should contain added content"
    );

    // 8. restore_checkpoint: restore to original state
    repository
        .update(cx, |repository, _| {
            repository.restore_checkpoint(checkpoint_1.clone())
        })
        .await
        .unwrap()
        .unwrap();
    cx.run_until_parked();

    // 9. Create a checkpoint after restore
    let checkpoint_3 = repository
        .update(cx, |repository, _| repository.checkpoint())
        .await
        .unwrap()
        .unwrap();

    // 10. compare_checkpoints: restored state matches original
    let equal = repository
        .update(cx, |repository, _| {
            repository.compare_checkpoints(checkpoint_1.clone(), checkpoint_3.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(equal, "restored state should match original checkpoint");

    // 11. diff_checkpoints: restored state vs original => empty diff
    let diff = repository
        .update(cx, |repository, _| {
            repository.diff_checkpoints(checkpoint_1.clone(), checkpoint_3.clone())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(diff.is_empty(), "diff after restore should be empty");
}
