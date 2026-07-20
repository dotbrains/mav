use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_project_group_keys_remain_distinct_for_sibling_repo_subdirectories(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "packages": {
                    "a": { "file.txt": "a" },
                    "b": { "file.txt": "b" },
                },
            },
        }),
    )
    .await;

    let project_a =
        Project::test(fs.clone(), [path!("/root/my-repo/packages/a").as_ref()], cx).await;
    let project_b = Project::test(fs, [path!("/root/my-repo/packages/b").as_ref()], cx).await;

    project_a
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    project_b
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let key_a = project_a.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));
    let key_b = project_b.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));

    assert_ne!(key_a, key_b);
    assert_eq!(
        key_a
            .path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/packages/a"))]
    );
    assert_eq!(
        key_b
            .path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/packages/b"))]
    );
}

#[gpui::test]
async fn test_repository_subfolder_git_status(
    executor: gpui::BackgroundExecutor,
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            "my-repo": {
                ".git": {},
                "a.txt": "a",
                "sub-folder-1": {
                    "sub-folder-2": {
                        "c.txt": "cc",
                        "d": {
                            "e.txt": "eee"
                        }
                    },
                }
            },
        }),
    )
    .await;

    const C_TXT: &str = "sub-folder-1/sub-folder-2/c.txt";
    const E_TXT: &str = "sub-folder-1/sub-folder-2/d/e.txt";

    fs.set_status_for_repo(
        path!("/root/my-repo/.git").as_ref(),
        &[(E_TXT, FileStatus::Untracked)],
    );

    let project = Project::test(
        fs.clone(),
        [path!("/root/my-repo/sub-folder-1/sub-folder-2").as_ref()],
        cx,
    )
    .await;

    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    let worktree_paths = project.read_with(cx, |project, cx| project.worktree_paths(cx));
    assert_eq!(
        worktree_paths
            .main_worktree_path_list()
            .ordered_paths()
            .map(|path| path.as_path())
            .collect::<Vec<_>>(),
        vec![Path::new(path!("/root/my-repo/sub-folder-1/sub-folder-2"))]
    );

    // Ensure that the git status is loaded correctly
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository.work_directory_abs_path,
            Path::new(path!("/root/my-repo")).into()
        );

        assert_eq!(repository.status_for_path(&repo_path(C_TXT)), None);
        assert_eq!(
            repository
                .status_for_path(&repo_path(E_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked
        );
    });

    fs.set_status_for_repo(path!("/root/my-repo/.git").as_ref(), &[]);
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(repository.status_for_path(&repo_path(C_TXT)), None);
        assert_eq!(repository.status_for_path(&repo_path(E_TXT)), None);
    });
}

// TODO: this test is flaky (especially on Windows but at least sometimes on all platforms).
#[cfg(any())]
#[gpui::test]
async fn test_conflicted_cherry_pick(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
        },
    }));
    let root_path = root.path();

    let work_dir = &root_path.join("project");
    let repo = git_init(work_dir);
    git_add("a.txt", &repo);
    git_commit("init", &repo);

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [root_path], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    git_branch("other-branch", &repo);
    git_checkout("other-branch", &repo);
    std::fs::write(root_path.join("project/a.txt"), "A").unwrap();
    git_add("a.txt", &repo);
    git_commit("capitalize", &repo);
    let commit = git_rev_parse("HEAD", &repo);
    git_checkout("main", &repo);
    std::fs::write(root_path.join("project/a.txt"), "b").unwrap();
    git_add("a.txt", &repo);
    git_commit("improve letter", &repo);
    git_cherry_pick_expect_conflict(&commit, &repo);
    std::fs::read_to_string(root_path.join("project/.git/CHERRY_PICK_HEAD"))
        .expect("No CHERRY_PICK_HEAD");
    pretty_assertions::assert_eq!(
        git_status(&repo),
        collections::HashMap::from_iter([("a.txt".to_owned(), GIT_STATUS_CONFLICTED.to_owned())])
    );
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();
    let conflicts = repository.update(cx, |repository, _| {
        repository
            .merge_conflicts
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(conflicts, [RepoPath::from("a.txt")]);

    git_add("a.txt", &repo);
    // Attempt to manually simulate what `git cherry-pick --continue` would do.
    git_commit("whatevs", &repo);
    std::fs::remove_file(root.path().join("project/.git/CHERRY_PICK_HEAD"))
        .expect("Failed to remove CHERRY_PICK_HEAD");
    pretty_assertions::assert_eq!(git_status(&repo), collections::HashMap::default());
    tree.flush_fs_events(cx).await;
    let conflicts = repository.update(cx, |repository, _| {
        repository
            .merge_conflicts
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    });
    pretty_assertions::assert_eq!(conflicts, []);
}

#[gpui::test]
async fn test_update_gitignore(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            ".gitignore": "*.txt\n",
            "a.xml": "<a></a>",
            "b.txt": "Some text"
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/root/.git").as_ref(),
        &[
            (".gitignore", "*.txt\n".into()),
            ("a.xml", "<a></a>".into()),
        ],
    );

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    let tree = project.read_with(cx, |project, cx| project.worktrees(cx).next().unwrap());
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });

    // One file is unmodified, the other is ignored.
    cx.read(|cx| {
        assert_entry_git_state(tree.read(cx), repository.read(cx), "a.xml", None, false);
        assert_entry_git_state(tree.read(cx), repository.read(cx), "b.txt", None, true);
    });

    // Change the gitignore, and stage the newly non-ignored file.
    fs.atomic_write(path!("/root/.gitignore").into(), "*.xml\n".into())
        .await
        .unwrap();
    fs.set_index_for_repo(
        Path::new(path!("/root/.git")),
        &[
            (".gitignore", "*.txt\n".into()),
            ("a.xml", "<a></a>".into()),
            ("b.txt", "Some text".into()),
        ],
    );

    cx.executor().run_until_parked();
    cx.read(|cx| {
        assert_entry_git_state(tree.read(cx), repository.read(cx), "a.xml", None, true);
        assert_entry_git_state(
            tree.read(cx),
            repository.read(cx),
            "b.txt",
            Some(StatusCode::Added),
            false,
        );
    });
}

// NOTE:
// This test always fails on Windows, because on Windows, unlike on Unix, you can't rename
// a directory which some program has already open.
// This is a limitation of the Windows.
// See: https://stackoverflow.com/questions/41365318/access-is-denied-when-renaming-folder
// See: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
