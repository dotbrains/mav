mod git_worktrees {
    use fs::{FakeFs, Fs};
    use gpui::TestAppContext;
    use project::worktrees_directory_for_repo;
    use serde_json::json;
    use settings::SettingsStore;
    use std::path::{Path, PathBuf};
    use util::{path, paths::PathStyle};

    fn init_test(cx: &mut gpui::TestAppContext) {
        zlog::init_test();

        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    #[test]
    fn test_validate_worktree_directory() {
        let work_dir = Path::new("/code/my-project");

        // Valid: sibling
        assert!(worktrees_directory_for_repo(work_dir, "../worktrees", PathStyle::Posix).is_ok());

        // Valid: subdirectory
        assert!(
            worktrees_directory_for_repo(work_dir, ".git/mav-worktrees", PathStyle::Posix).is_ok()
        );
        assert!(worktrees_directory_for_repo(work_dir, "my-worktrees", PathStyle::Posix).is_ok());

        // Invalid: just ".." would resolve back to the working directory itself
        let err = worktrees_directory_for_repo(work_dir, "..", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("must not be \"..\""));

        // Invalid: ".." with trailing separators
        let err = worktrees_directory_for_repo(work_dir, "..\\", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("must not be \"..\""));
        let err = worktrees_directory_for_repo(work_dir, "../", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("must not be \"..\""));

        // Invalid: empty string would resolve to the working directory itself
        let err = worktrees_directory_for_repo(work_dir, "", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));

        // Invalid: absolute path
        let err =
            worktrees_directory_for_repo(work_dir, "/tmp/worktrees", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("relative path"));

        // Invalid: "/" is absolute on Unix
        let err = worktrees_directory_for_repo(work_dir, "/", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("relative path"));

        // Invalid: "///" is absolute
        let err = worktrees_directory_for_repo(work_dir, "///", PathStyle::Posix).unwrap_err();
        assert!(err.to_string().contains("relative path"));

        // Invalid: escapes too far up
        let err =
            worktrees_directory_for_repo(work_dir, "../../other-project/wt", PathStyle::Posix)
                .unwrap_err();
        assert!(err.to_string().contains("outside"));
    }

    #[test]
    fn test_worktree_directory_uses_remote_path_style() {
        let work_dir = Path::new("/home/user/dev/lsp-tests");

        let directory =
            worktrees_directory_for_repo(work_dir, "../worktrees", PathStyle::Posix).unwrap();

        assert_eq!(
            directory,
            PathBuf::from("/home/user/dev/worktrees/lsp-tests")
        );
    }

    #[gpui::test]
    async fn test_git_worktrees_list_and_create(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/root"),
            json!({
                ".git": {},
                "file.txt": "content",
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        let worktrees = cx
            .update(|cx| repository.update(cx, |repository, _| repository.worktrees()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].path, PathBuf::from(path!("/root")));

        let worktrees_directory = PathBuf::from(path!("/root"));
        let worktree_1_directory = worktrees_directory.join("feature-branch");
        cx.update(|cx| {
            repository.update(cx, |repository, _| {
                repository.create_worktree(
                    git::repository::CreateWorktreeTarget::NewBranch {
                        branch_name: "feature-branch".to_string(),
                        base_sha: Some("abc123".to_string()),
                    },
                    worktree_1_directory.clone(),
                )
            })
        })
        .await
        .unwrap()
        .unwrap();

        cx.executor().run_until_parked();

        let worktrees = cx
            .update(|cx| repository.update(cx, |repository, _| repository.worktrees()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, PathBuf::from(path!("/root")));
        assert_eq!(worktrees[1].path, worktree_1_directory);
        assert_eq!(
            worktrees[1].ref_name,
            Some("refs/heads/feature-branch".into())
        );
        assert_eq!(worktrees[1].sha.as_ref(), "abc123");

        let worktree_2_directory = worktrees_directory.join("bugfix-branch");
        cx.update(|cx| {
            repository.update(cx, |repository, _| {
                repository.create_worktree(
                    git::repository::CreateWorktreeTarget::NewBranch {
                        branch_name: "bugfix-branch".to_string(),
                        base_sha: None,
                    },
                    worktree_2_directory.clone(),
                )
            })
        })
        .await
        .unwrap()
        .unwrap();

        cx.executor().run_until_parked();

        // List worktrees — should now have main + two created
        let worktrees = cx
            .update(|cx| repository.update(cx, |repository, _| repository.worktrees()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worktrees.len(), 3);

        let worktree_1 = worktrees
            .iter()
            .find(|worktree| worktree.ref_name == Some("refs/heads/feature-branch".into()))
            .expect("should find feature-branch worktree");
        assert_eq!(worktree_1.path, worktree_1_directory);

        let worktree_2 = worktrees
            .iter()
            .find(|worktree| worktree.ref_name == Some("refs/heads/bugfix-branch".into()))
            .expect("should find bugfix-branch worktree");
        assert_eq!(worktree_2.path, worktree_2_directory);
        assert_eq!(worktree_2.sha.as_ref(), "fake-sha");
    }

    #[gpui::test]
    async fn test_remove_worktree_removes_managed_parent_directories(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/root"),
            json!({
                ".git": {},
                "file.txt": "content",
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        let worktree_path = PathBuf::from(path!("/worktrees/root/feature/nested/root"));
        let worktree_parent = PathBuf::from(path!("/worktrees/root/feature/nested"));
        let worktree_intermediate_parent = PathBuf::from(path!("/worktrees/root/feature"));
        let worktree_base = PathBuf::from(path!("/worktrees/root"));

        cx.update(|cx| {
            repository.update(cx, |repository, _| {
                repository.create_worktree(
                    git::repository::CreateWorktreeTarget::NewBranch {
                        branch_name: "feature/nested".to_string(),
                        base_sha: Some("abc123".to_string()),
                    },
                    worktree_path.clone(),
                )
            })
        })
        .await
        .unwrap()
        .unwrap();

        assert!(Fs::is_dir(fs.as_ref(), &worktree_path).await);
        assert!(Fs::is_dir(fs.as_ref(), &worktree_parent).await);
        assert!(Fs::is_dir(fs.as_ref(), &worktree_intermediate_parent).await);
        assert!(Fs::is_dir(fs.as_ref(), &worktree_base).await);

        cx.update(|cx| {
            repository.update(cx, |repository, _| {
                repository.remove_worktree(worktree_path.clone(), false)
            })
        })
        .await
        .unwrap()
        .unwrap();

        cx.executor().run_until_parked();

        assert!(!Fs::is_dir(fs.as_ref(), &worktree_path).await);
        assert!(!Fs::is_dir(fs.as_ref(), &worktree_parent).await);
        assert!(!Fs::is_dir(fs.as_ref(), &worktree_intermediate_parent).await);
        assert!(Fs::is_dir(fs.as_ref(), &worktree_base).await);
    }

    use crate::Project;
}
