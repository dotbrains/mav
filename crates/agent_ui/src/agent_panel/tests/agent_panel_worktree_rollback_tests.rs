use super::*;

#[gpui::test]
async fn test_rollback_all_succeed_returns_ok(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        cx.update_flags(true, vec!["agent-v2".to_string()]);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let path_a = PathBuf::from("/worktrees/branch/project_a");
    let path_b = PathBuf::from("/worktrees/branch/project_b");

    let (sender_a, receiver_a) = futures::channel::oneshot::channel::<Result<()>>();
    let (sender_b, receiver_b) = futures::channel::oneshot::channel::<Result<()>>();
    sender_a.send(Ok(())).unwrap();
    sender_b.send(Ok(())).unwrap();

    let creation_infos = vec![
        (repository.clone(), path_a.clone(), receiver_a),
        (repository.clone(), path_b.clone(), receiver_b),
    ];

    let fs_clone = fs.clone();
    let result = multi_workspace
        .update(cx, |_, window, cx| {
            window.spawn(cx, async move |cx| {
                git_ui::worktree_service::await_and_rollback_on_failure(
                    creation_infos,
                    fs_clone,
                    cx,
                )
                .await
            })
        })
        .unwrap()
        .await;

    let paths = result.expect("all succeed should return Ok");
    assert_eq!(paths, vec![path_a, path_b]);
}

#[gpui::test]
async fn test_rollback_on_failure_attempts_all_worktrees(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        cx.update_flags(true, vec!["agent-v2".to_string()]);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let success_path = PathBuf::from("/worktrees/branch/project");
    cx.update(|cx| {
        repository.update(cx, |repo, _| {
            repo.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "branch".to_string(),
                    base_sha: None,
                },
                success_path.clone(),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();
    cx.executor().run_until_parked();

    assert!(
        fs.is_dir(&success_path).await,
        "worktree directory should exist before rollback"
    );

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let failed_path = PathBuf::from("/worktrees/branch/failed_project");

    let (sender_ok, receiver_ok) = futures::channel::oneshot::channel::<Result<()>>();
    let (sender_err, receiver_err) = futures::channel::oneshot::channel::<Result<()>>();
    sender_ok.send(Ok(())).unwrap();
    sender_err
        .send(Err(anyhow!("branch already exists")))
        .unwrap();

    let creation_infos = vec![
        (repository.clone(), success_path.clone(), receiver_ok),
        (repository.clone(), failed_path.clone(), receiver_err),
    ];

    let fs_clone = fs.clone();
    let result = multi_workspace
        .update(cx, |_, window, cx| {
            window.spawn(cx, async move |cx| {
                git_ui::worktree_service::await_and_rollback_on_failure(
                    creation_infos,
                    fs_clone,
                    cx,
                )
                .await
            })
        })
        .unwrap()
        .await;

    assert!(
        result.is_err(),
        "should return error when any creation fails"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("branch already exists"),
        "error should mention the original failure: {err_msg}"
    );

    cx.executor().run_until_parked();
    assert!(
        !fs.is_dir(&success_path).await,
        "successful worktree directory should be removed by rollback"
    );
}

#[gpui::test]
async fn test_rollback_on_canceled_receiver(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        cx.update_flags(true, vec!["agent-v2".to_string()]);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let path = PathBuf::from("/worktrees/branch/project");

    let (sender, receiver) = futures::channel::oneshot::channel::<Result<()>>();
    drop(sender);

    let creation_infos = vec![(repository.clone(), path.clone(), receiver)];

    let fs_clone = fs.clone();
    let result = multi_workspace
        .update(cx, |_, window, cx| {
            window.spawn(cx, async move |cx| {
                git_ui::worktree_service::await_and_rollback_on_failure(
                    creation_infos,
                    fs_clone,
                    cx,
                )
                .await
            })
        })
        .unwrap()
        .await;

    assert!(
        result.is_err(),
        "should return error when receiver is canceled"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("canceled"),
        "error should mention cancellation: {err_msg}"
    );
}

#[gpui::test]
async fn test_rollback_cleans_up_orphan_directories(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        cx.update_flags(true, vec!["agent-v2".to_string()]);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let orphan_path = PathBuf::from("/worktrees/branch/orphan_project");
    fs.insert_tree(
        "/worktrees/branch/orphan_project",
        json!({ "leftover.txt": "junk" }),
    )
    .await;

    assert!(
        fs.is_dir(&orphan_path).await,
        "orphan dir should exist before rollback"
    );

    let (sender, receiver) = futures::channel::oneshot::channel::<Result<()>>();
    sender.send(Err(anyhow!("hook failed"))).unwrap();

    let creation_infos = vec![(repository.clone(), orphan_path.clone(), receiver)];

    let fs_clone = fs.clone();
    let result = multi_workspace
        .update(cx, |_, window, cx| {
            window.spawn(cx, async move |cx| {
                git_ui::worktree_service::await_and_rollback_on_failure(
                    creation_infos,
                    fs_clone,
                    cx,
                )
                .await
            })
        })
        .unwrap()
        .await;

    cx.executor().run_until_parked();

    assert!(result.is_err());
    assert!(
        !fs.is_dir(&orphan_path).await,
        "orphan worktree directory should be removed by filesystem cleanup"
    );
}
