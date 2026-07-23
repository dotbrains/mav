use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_remove_root_deletes_directory_and_git_metadata(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature"]);

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/worktrees/project/feature/project"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    record_mav_created_worktree(&fs, Path::new("/worktrees/project/feature/project"), cx).await;

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/project"),
            Path::new("/worktrees/project/feature/project"),
        ],
        cx,
    )
    .await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    cx.run_until_parked();

    // Build the root plan while the worktree is still loaded.
    let root = workspace
        .read_with(cx, |_workspace, cx| {
            build_root_plan(
                Path::new("/worktrees/project/feature/project"),
                None,
                std::slice::from_ref(&workspace),
                cx,
            )
        })
        .expect("should produce a root plan for the linked worktree");

    assert!(
        fs.is_dir(Path::new("/worktrees/project/feature/project"))
            .await
    );

    // Remove the root.
    let task = cx.update(|cx| cx.spawn(async move |cx| remove_root(root, cx).await));
    task.await.expect("remove_root should succeed");

    cx.run_until_parked();

    // The FakeFs directory should be gone.
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature/project"))
            .await,
        "linked worktree directory should be removed from FakeFs"
    );
}

#[gpui::test]
async fn test_remove_root_succeeds_when_directory_already_gone(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature"]);

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/worktrees/project/feature/project"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    record_mav_created_worktree(&fs, Path::new("/worktrees/project/feature/project"), cx).await;

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/project"),
            Path::new("/worktrees/project/feature/project"),
        ],
        cx,
    )
    .await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    cx.run_until_parked();

    let root = workspace
        .read_with(cx, |_workspace, cx| {
            build_root_plan(
                Path::new("/worktrees/project/feature/project"),
                None,
                std::slice::from_ref(&workspace),
                cx,
            )
        })
        .expect("should produce a root plan for the linked worktree");

    // Manually remove the worktree directory from FakeFs before calling
    // remove_root, simulating the directory being deleted externally.
    fs.as_ref()
        .remove_dir(
            Path::new("/worktrees/project/feature/project"),
            fs::RemoveOptions {
                recursive: true,
                ignore_if_not_exists: false,
            },
        )
        .await
        .unwrap();
    assert!(
        !fs.as_ref()
            .is_dir(Path::new("/worktrees/project/feature/project"))
            .await
    );

    // remove_root should still succeed — fs.remove_dir with
    // ignore_if_not_exists handles NotFound, and git worktree remove
    // handles a missing working tree directory.
    let task = cx.update(|cx| cx.spawn(async move |cx| remove_root(root, cx).await));
    task.await
        .expect("remove_root should succeed even when directory is already gone");
}

#[gpui::test]
async fn test_remove_root_refuses_when_worktree_recreated_outside_mav(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature"]);

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/worktrees/project/feature/project"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    // Record a creation time that doesn't match the directory on disk,
    // simulating a worktree that was removed and recreated outside Mav
    // after Mav recorded the original.
    let worktree_path = Path::new("/worktrees/project/feature/project");
    let actual_created_at = fake_worktree_created_at(&fs, worktree_path).await;
    cx.update(|cx| {
        git_ui::created_worktrees::record_created_worktree(
            worktree_path,
            None,
            actual_created_at + Duration::from_secs(1),
            cx,
        )
    })
    .await
    .unwrap();

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/project"),
            Path::new("/worktrees/project/feature/project"),
        ],
        cx,
    )
    .await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    cx.run_until_parked();

    let root = workspace
        .read_with(cx, |_workspace, cx| {
            build_root_plan(
                Path::new("/worktrees/project/feature/project"),
                None,
                std::slice::from_ref(&workspace),
                cx,
            )
        })
        .expect("should produce a root plan while the record exists");

    let task = cx.update(|cx| cx.spawn(async move |cx| remove_root(root, cx).await));
    let error = task
        .await
        .expect_err("remove_root should refuse to delete a recreated worktree");
    assert!(
        error.to_string().contains("not the worktree Mav created"),
        "unexpected error: {error:#}"
    );

    cx.run_until_parked();

    // The directory must be left untouched.
    assert!(
        fs.is_dir(Path::new("/worktrees/project/feature/project"))
            .await,
        "worktree directory should not be deleted on creation time mismatch"
    );

    // The stale record should be forgotten, so subsequent archival
    // attempts skip the worktree entirely.
    workspace.read_with(cx, |_workspace, cx| {
        assert!(
            git_ui::created_worktrees::recorded_created_at(worktree_path, None, cx).is_none(),
            "stale created-worktree record should be removed"
        );
        let plan = build_root_plan(worktree_path, None, std::slice::from_ref(&workspace), cx);
        assert!(
            plan.is_none(),
            "build_root_plan should return None after the stale record is removed"
        );
    });
}

#[gpui::test]
async fn test_remove_root_returns_error_and_rolls_back_on_remove_dir_failure(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".git": {},
            "src": { "main.rs": "fn main() {}" }
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature"]);

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/worktrees/project/feature/project"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    record_mav_created_worktree(&fs, Path::new("/worktrees/project/feature/project"), cx).await;

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/project"),
            Path::new("/worktrees/project/feature/project"),
        ],
        cx,
    )
    .await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    cx.run_until_parked();

    let root = workspace
        .read_with(cx, |_workspace, cx| {
            build_root_plan(
                Path::new("/worktrees/project/feature/project"),
                None,
                std::slice::from_ref(&workspace),
                cx,
            )
        })
        .expect("should produce a root plan for the linked worktree");

    // Make deleting the worktree directory fail, while leaving the
    // worktree itself intact so the created-by-Mav verification passes.
    let worktree_path = Path::new("/worktrees/project/feature/project");
    fs.set_remove_dir_error(worktree_path, "simulated remove_dir failure".to_string());

    let task = cx.update(|cx| cx.spawn(async move |cx| remove_root(root, cx).await));
    let result = task.await;

    assert!(
        result.is_err(),
        "remove_root should return an error when fs.remove_dir fails"
    );
    let error_message = format!("{:#}", result.unwrap_err());
    assert!(
        error_message.contains("failed to delete worktree directory"),
        "error should mention the directory deletion failure, got: {error_message}"
    );

    cx.run_until_parked();

    // After rollback, the worktree should be re-added to the project.
    let has_worktree = project.read_with(cx, |project, cx| {
        project
            .worktrees(cx)
            .any(|wt| wt.read(cx).abs_path().as_ref() == worktree_path)
    });
    assert!(
        has_worktree,
        "rollback should have re-added the worktree to the project"
    );
}
