use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_build_root_plan_with_custom_worktree_directory(cx: &mut TestAppContext) {
    init_test(cx);

    // Override the worktree_directory setting to a non-default location.
    // With main repo at /project and setting "../custom-worktrees", the
    // resolved base is /custom-worktrees/project.
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.git.get_or_insert(Default::default()).worktree_directory =
                    Some("../custom-worktrees".to_string());
            });
        });
    });

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
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature", "feature2"]);

    // Worktree inside the custom managed directory.
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/custom-worktrees/project/feature/project"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    record_mav_created_worktree(
        &fs,
        Path::new("/custom-worktrees/project/feature/project"),
        cx,
    )
    .await;

    // Worktree outside the custom managed directory (at the default
    // `../worktrees` location, which is not what the setting says).
    // It is recorded as Mav-created so that the directory check, not
    // the registry, is what excludes it below.
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        true,
        GitWorktree {
            path: PathBuf::from("/worktrees/project/feature2/project"),
            ref_name: Some("refs/heads/feature2".into()),
            sha: "def456".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    record_mav_created_worktree(&fs, Path::new("/worktrees/project/feature2/project"), cx).await;

    let project = Project::test(
        fs.clone(),
        [
            Path::new("/project"),
            Path::new("/custom-worktrees/project/feature/project"),
            Path::new("/worktrees/project/feature2/project"),
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
        // Worktree inside the custom managed directory SHOULD be archivable.
        let plan = build_root_plan(
            Path::new("/custom-worktrees/project/feature/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_some(),
            "build_root_plan should return Some for a worktree inside \
                 the custom worktree_directory",
        );

        // Worktree at the default location SHOULD NOT be archivable
        // because the setting points elsewhere.
        let plan = build_root_plan(
            Path::new("/worktrees/project/feature2/project"),
            None,
            std::slice::from_ref(&workspace),
            cx,
        );
        assert!(
            plan.is_none(),
            "build_root_plan should return None for a worktree outside \
                 the custom worktree_directory, even if it would match the default",
        );
    });
}
