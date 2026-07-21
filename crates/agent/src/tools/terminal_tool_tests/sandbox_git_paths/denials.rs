    #[gpui::test]
    async fn test_sandbox_paths_do_not_follow_gitfile_changed_after_scan(
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
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                ".git": {
                    "worktrees": {
                        "other": {
                            "HEAD": "ref: refs/heads/other",
                            "commondir": "/other_repo/.git",
                            "gitdir": "/other_worktree/.git"
                        }
                    }
                }
            }),
        )
        .await;

        let project = project::Project::test(fs.clone(), [Path::new("/linked_worktree")], cx).await;
        fs.write(
            Path::new("/linked_worktree/.git"),
            b"gitdir: /other_repo/.git/worktrees/other",
        )
        .await
        .expect("mutated gitfile should be written");

        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
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
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git/worktrees/other"))
    );
}
    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_unverified_worktree_gitdir(
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
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                ".git": {
                    "worktrees": {
                        "other": {
                            "HEAD": "ref: refs/heads/other",
                            "commondir": "/other_repo/.git",
                            "gitdir": "/other_worktree/.git"
                        }
                    }
                }
            }),
        )
        .await;
        fs.write(
            Path::new("/linked_worktree/.git"),
            b"gitdir: /other_repo/.git/worktrees/other",
        )
        .await
        .expect("malicious gitfile should be written");

        let project = project::Project::test(fs.clone(), [Path::new("/linked_worktree")], cx).await;
        let candidates =
            cx.update(|cx| SandboxGitPathCandidates::from_project(project.read(cx), cx));
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/linked_worktree"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git/worktrees/other"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/linked_worktree/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/other_repo/.git"))
        );
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/other_repo/.git/worktrees/other"))
        );
    }

    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_symlinked_dot_git(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            serde_json::json!({
                "file.txt": "content",
            }),
        )
        .await;
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                ".git": {}
            }),
        )
        .await;
        fs.insert_symlink(
            Path::new("/project/.git"),
            PathBuf::from("/other_repo/.git"),
        )
        .await;

        let candidates = SandboxGitPathCandidates {
            writable_paths: vec![PathBuf::from("/project")],
            git_paths: vec![
                PathBuf::from("/project/.git"),
                PathBuf::from("/other_repo/.git"),
            ],
            repositories: vec![SandboxGitRepositoryPaths {
                work_directory_abs_path: PathBuf::from("/project"),
                dot_git_abs_path: PathBuf::from("/project/.git"),
                repository_dir_abs_path: PathBuf::from("/other_repo/.git"),
                common_dir_abs_path: PathBuf::from("/other_repo/.git"),
            }],
        };
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/project/.git"))
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

    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_symlinked_dot_git_file(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            serde_json::json!({
                "file.txt": "content",
            }),
        )
        .await;
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                "gitfile": "gitdir: /other_repo/.git",
                ".git": {
                    "HEAD": "ref: refs/heads/main",
                    "config": "[core]\n\trepositoryformatversion = 0\n\tworktree = /project\n"
                }
            }),
        )
        .await;
        fs.insert_symlink(
            Path::new("/project/.git"),
            PathBuf::from("/other_repo/gitfile"),
        )
        .await;

        let candidates = SandboxGitPathCandidates {
            writable_paths: vec![PathBuf::from("/project")],
            git_paths: vec![
                PathBuf::from("/project/.git"),
                PathBuf::from("/other_repo/.git"),
            ],
            repositories: vec![SandboxGitRepositoryPaths {
                work_directory_abs_path: PathBuf::from("/project"),
                dot_git_abs_path: PathBuf::from("/project/.git"),
                repository_dir_abs_path: PathBuf::from("/other_repo/.git"),
                common_dir_abs_path: PathBuf::from("/other_repo/.git"),
            }],
        };
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/project/.git"))
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

    #[gpui::test]
    async fn test_sandbox_paths_do_not_grant_gitfile_to_symlinked_gitdir(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            serde_json::json!({
                ".git": "gitdir: /other_repo/gitdir_link",
                "file.txt": "content",
            }),
        )
        .await;
        fs.insert_tree(
            "/other_repo",
            serde_json::json!({
                ".git": {
                    "HEAD": "ref: refs/heads/main",
                    "config": "[core]\n\trepositoryformatversion = 0\n\tworktree = /project\n"
                }
            }),
        )
        .await;
        fs.insert_symlink(
            Path::new("/other_repo/gitdir_link"),
            PathBuf::from("/other_repo/.git"),
        )
        .await;

        let candidates = SandboxGitPathCandidates {
            writable_paths: vec![PathBuf::from("/project")],
            git_paths: vec![
                PathBuf::from("/project/.git"),
                PathBuf::from("/other_repo/gitdir_link"),
                PathBuf::from("/other_repo/.git"),
            ],
            repositories: vec![SandboxGitRepositoryPaths {
                work_directory_abs_path: PathBuf::from("/project"),
                dot_git_abs_path: PathBuf::from("/project/.git"),
                repository_dir_abs_path: PathBuf::from("/other_repo/gitdir_link"),
                common_dir_abs_path: PathBuf::from("/other_repo/gitdir_link"),
            }],
        };
        let paths_with_git_access = sandbox_git_paths(candidates, fs.as_ref(), true).await;

        assert!(!paths_with_git_access.allow_git_access);
        assert!(
            paths_with_git_access
                .git_dirs
                .contains(&PathBuf::from("/other_repo/gitdir_link"))
        );
        assert!(
            !paths_with_git_access
                .writable_paths
                .contains(&PathBuf::from("/other_repo/.git"))
        );
    }
