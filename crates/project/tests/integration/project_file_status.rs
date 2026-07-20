use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_rename_work_directory(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let root = TempTree::new(json!({
        "projects": {
            "project1": {
                "a": "",
                "b": "",
            }
        },

    }));
    let root_path = root.path();

    let repo = git_init(&root_path.join("projects/project1"));
    git_add("a", &repo);
    git_commit("init", &repo);
    std::fs::write(root_path.join("projects/project1/a"), "aa").unwrap();

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

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("projects/project1").as_path()
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path("a"))
                .map(|entry| entry.status),
            Some(StatusCode::Modified.worktree()),
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path("b"))
                .map(|entry| entry.status),
            Some(FileStatus::Untracked),
        );
    });

    std::fs::rename(
        root_path.join("projects/project1"),
        root_path.join("projects/project2"),
    )
    .unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("projects/project2").as_path()
        );
        assert_eq!(
            repository.status_for_path(&repo_path("a")).unwrap().status,
            StatusCode::Modified.worktree(),
        );
        assert_eq!(
            repository.status_for_path(&repo_path("b")).unwrap().status,
            FileStatus::Untracked,
        );
    });
}

// NOTE: This test always fails on Windows, because on Windows, unlike on Unix,
// you can't rename a directory which some program has already open. This is a
// limitation of the Windows. See:
// See: https://stackoverflow.com/questions/41365318/access-is-denied-when-renaming-folder
// See: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_file_status(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    const IGNORE_RULE: &str = "**/target";

    let root = TempTree::new(json!({
        "project": {
            "a.txt": "a",
            "b.txt": "bb",
            "c": {
                "d": {
                    "e.txt": "eee"
                }
            },
            "f.txt": "ffff",
            "target": {
                "build_file": "???"
            },
            ".gitignore": IGNORE_RULE
        },

    }));
    let root_path = root.path();

    const A_TXT: &str = "a.txt";
    const B_TXT: &str = "b.txt";
    const E_TXT: &str = "c/d/e.txt";
    const F_TXT: &str = "f.txt";
    const DOTGITIGNORE: &str = ".gitignore";
    const BUILD_FILE: &str = "target/build_file";

    // Set up git repository before creating the worktree.
    let work_dir = root.path().join("project");
    let repo = git_init(work_dir.as_path());
    git_add(A_TXT, &repo);
    git_add(E_TXT, &repo);
    git_add(DOTGITIGNORE, &repo);
    git_commit("Initial commit", &repo);

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

    // Check that the right git state is observed on startup
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository.work_directory_abs_path.as_ref(),
            root_path.join("project").as_path()
        );

        assert_eq!(
            repository
                .status_for_path(&repo_path(B_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path(F_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });

    // Modify a file in the working copy.
    std::fs::write(work_dir.join(A_TXT), "aa").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // The worktree detects that the file's git status has changed.
    repository.read_with(cx, |repository, _| {
        assert_eq!(
            repository
                .status_for_path(&repo_path(A_TXT))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    // Create a commit in the git repository.
    git_add(A_TXT, &repo);
    git_add(B_TXT, &repo);
    git_commit("Committing modified and added", &repo);
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // The worktree detects that the files' git status have changed.
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&repo_path(F_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(repository.status_for_path(&repo_path(B_TXT)), None);
        assert_eq!(repository.status_for_path(&repo_path(A_TXT)), None);
    });

    // Modify files in the working copy and perform git operations on other files.
    git_reset(0, &repo);
    git_remove_index(Path::new(B_TXT), &repo);
    git_stash(&repo);
    std::fs::write(work_dir.join(E_TXT), "eeee").unwrap();
    std::fs::write(work_dir.join(BUILD_FILE), "this should be ignored").unwrap();
    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    // Check that more complex repo changes are tracked
    repository.read_with(cx, |repository, _cx| {
        assert_eq!(repository.status_for_path(&repo_path(A_TXT)), None);
        assert_eq!(
            repository
                .status_for_path(&repo_path(B_TXT))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
        assert_eq!(
            repository
                .status_for_path(&repo_path(E_TXT))
                .unwrap()
                .status,
            StatusCode::Modified.worktree(),
        );
    });

    std::fs::remove_file(work_dir.join(B_TXT)).unwrap();
    std::fs::remove_dir_all(work_dir.join("c")).unwrap();
    std::fs::write(
        work_dir.join(DOTGITIGNORE),
        [IGNORE_RULE, "f.txt"].join("\n"),
    )
    .unwrap();

    git_add(Path::new(DOTGITIGNORE), &repo);
    git_commit("Committing modified git ignore", &repo);

    tree.flush_fs_events(cx).await;
    cx.executor().run_until_parked();

    let mut renamed_dir_name = "first_directory/second_directory";
    const RENAMED_FILE: &str = "rf.txt";

    std::fs::create_dir_all(work_dir.join(renamed_dir_name)).unwrap();
    std::fs::write(
        work_dir.join(renamed_dir_name).join(RENAMED_FILE),
        "new-contents",
    )
    .unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&RepoPath::from_rel_path(
                    &rel_path(renamed_dir_name).join(rel_path(RENAMED_FILE))
                ))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });

    renamed_dir_name = "new_first_directory/second_directory";

    std::fs::rename(
        work_dir.join("first_directory"),
        work_dir.join("new_first_directory"),
    )
    .unwrap();

    tree.flush_fs_events(cx).await;
    project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.executor().run_until_parked();

    repository.read_with(cx, |repository, _cx| {
        assert_eq!(
            repository
                .status_for_path(&RepoPath::from_rel_path(
                    &rel_path(renamed_dir_name).join(rel_path(RENAMED_FILE))
                ))
                .unwrap()
                .status,
            FileStatus::Untracked,
        );
    });
}
