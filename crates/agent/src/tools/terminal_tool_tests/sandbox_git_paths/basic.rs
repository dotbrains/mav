    #[gpui::test]
    async fn test_sandbox_paths_protect_git_paths_until_git_access_is_allowed(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/main_repo",
            serde_json::json!({
                ".git": {},
                "file.txt": "content",
            }),
        )
        .await;
        fs.add_linked_worktree_for_repo(
            Path::new("/main_repo/.git"),
            false,
            git::repository::Worktree {
                path: PathBuf::from("/linked_worktree"),
                ref_name: Some("refs/heads/feature".into()),
                sha: "abc123".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;
        fs.write(Path::new("/linked_worktree/file.txt"), b"content")
            .await
            .expect("linked worktree file should be written");

        let project = project::Project::test(fs.clone(), [Path::new("/linked_worktree")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_without_git_access = sandbox_git_paths(candidates, fs.as_ref(), false).await;

        assert!(
            paths_without_git_access
                .writable_paths
                .contains(&PathBuf::from("/linked_worktree"))
        );
        assert!(
            paths_without_git_access
                .git_dirs
                .contains(&PathBuf::from("/linked_worktree/.git"))
        );
        assert!(
            !paths_without_git_access
                .git_dirs
                .contains(&PathBuf::from("/linked_worktree/.gitignore"))
        );
        assert!(
            paths_without_git_access
                .git_dirs
                .contains(&PathBuf::from("/main_repo/.git"))
        );
        assert!(
            paths_without_git_access
                .git_dirs
                .contains(&PathBuf::from("/main_repo/.git/worktrees/feature"))
        );

        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/linked_worktree"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/linked_worktree/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/main_repo/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/main_repo/.git/worktrees/feature"))
    );
}
    #[gpui::test]
    async fn test_sandbox_paths_grant_git_access_when_non_git_folder_is_present(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/repo",
            serde_json::json!({
                ".git": {},
                "file.txt": "content",
            }),
        )
        .await;
        // A plain folder opened alongside the repo. Its `<root>/.git` placeholder
        // never corresponds to a repository, so it must not block the grant for
        // the real repo.
        fs.insert_tree("/notes", serde_json::json!({ "todo.txt": "hi" }))
            .await;

        let project =
            project::Project::test(fs.clone(), [Path::new("/repo"), Path::new("/notes")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/repo/.git"))
        );
        assert!(
            paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/notes"))
        );
    }

    #[gpui::test]
    async fn test_sandbox_paths_allow_submodule_gitdir_without_commondir(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/super",
            serde_json::json!({
                ".git": {
                    "modules": {
                        "sub": {
                            "HEAD": "ref: refs/heads/main",
                            "config": "[core]\n\trepositoryformatversion = 0\n\tworktree = ../../../sub\n"
                        }
                    }
                },
                "sub": {
                    ".git": "gitdir: ../.git/modules/sub",
                    "file.txt": "content"
                }
            }),
        )
        .await;

        let project = project::Project::test(fs.clone(), [Path::new("/super/sub")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/super/sub"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/super/sub/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/super/.git/modules/sub"))
        );
    }

    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_submodule_gitdir_without_back_reference(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/super",
            serde_json::json!({
                ".git": {
                    "modules": {
                        "sub": {
                            "HEAD": "ref: refs/heads/main",
                            "config": "[core]\n\trepositoryformatversion = 0\n"
                        }
                    }
                },
                "sub": {
                    ".git": "gitdir: ../.git/modules/sub",
                    "file.txt": "content"
                }
            }),
        )
        .await;

        let project = project::Project::test(fs.clone(), [Path::new("/super/sub")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/super/sub/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/super/.git/modules/sub"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/super/.git/modules/sub"))
        );
    }

    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_submodule_gitfile_to_unrelated_gitdir(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            serde_json::json!({
                "sub": {
                    ".git": "gitdir: /other_repo/.git",
                    "file.txt": "content"
                }
            }),
        )
        .await;
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                ".git": {
                    "HEAD": "ref: refs/heads/main",
                    "config": "[core]\n\trepositoryformatversion = 0\n"
                }
            }),
        )
        .await;

        let project = project::Project::test(fs.clone(), [Path::new("/project/sub")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/project/sub/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/other_repo/.git"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git"))
        );
    }
