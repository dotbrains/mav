use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_build_root_plan_returns_none_for_main_worktree(cx: &mut TestAppContext) {
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

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    cx.run_until_parked();

    // The main worktree should NOT produce a root plan.
    workspace.read_with(cx, |_workspace, cx| {
        let plan = build_root_plan(
            Path::new("/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_none(),
            "build_root_plan should return None for a main worktree",
        );
    });
}

#[gpui::test]
async fn test_build_root_plan_returns_some_for_linked_worktree(cx: &mut TestAppContext) {
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

    workspace.read_with(cx, |_workspace, cx| {
        // The linked worktree SHOULD produce a root plan.
        let plan = build_root_plan(
            Path::new("/worktrees/project/feature/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_some(),
            "build_root_plan should return Some for a linked worktree",
        );
        let plan = plan.unwrap();
        assert_eq!(
            plan.root_path,
            PathBuf::from("/worktrees/project/feature/project")
        );
        assert_eq!(plan.main_repo_path, PathBuf::from("/project"));

        // The main worktree should still return None.
        let main_plan = build_root_plan(
            Path::new("/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            main_plan.is_none(),
            "build_root_plan should return None for the main worktree \
                 even when a linked worktree exists",
        );
    });
}

#[gpui::test]
async fn test_build_root_plan_returns_none_for_external_linked_worktree(cx: &mut TestAppContext) {
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
            path: PathBuf::from("/external-worktree"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [Path::new("/project"), Path::new("/external-worktree")],
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

    workspace.read_with(cx, |_workspace, cx| {
        let plan = build_root_plan(
            Path::new("/external-worktree"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_none(),
            "build_root_plan should return None for a linked worktree \
                 outside the Mav-managed worktrees directory",
        );
    });
}

#[gpui::test]
async fn test_build_root_plan_returns_none_for_unrecorded_linked_worktree_in_managed_directory(
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
    // Deliberately don't record the worktree in the created-worktrees
    // registry: it represents a worktree the user created manually.

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

    workspace.read_with(cx, |_workspace, cx| {
        let plan = build_root_plan(
            Path::new("/worktrees/project/feature/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_none(),
            "build_root_plan should return None for a linked worktree Mav didn't create, \
                 even when it lives inside the Mav-managed worktrees directory",
        );
    });
}
