mod resolve_worktree_tests {
    use fs::FakeFs;
    use gpui::TestAppContext;
    use project::{
        git_store::resolve_git_worktree_to_main_repo, linked_worktree_short_name,
        repo_identity_path,
    };
    use serde_json::json;
    use std::path::{Path, PathBuf};

    #[gpui::test]
    async fn test_resolve_git_worktree_to_main_repo(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        // Set up a main repo with a worktree entry
        fs.insert_tree(
            "/main-repo",
            json!({
                ".git": {
                    "worktrees": {
                        "feature": {
                            "commondir": "../../",
                            "HEAD": "ref: refs/heads/feature"
                        }
                    }
                },
                "src": { "main.rs": "" }
            }),
        )
        .await;
        // Set up a worktree checkout pointing back to the main repo
        fs.insert_tree(
            "/worktree-checkout",
            json!({
                ".git": "gitdir: /main-repo/.git/worktrees/feature",
                "src": { "main.rs": "" }
            }),
        )
        .await;

        let result =
            resolve_git_worktree_to_main_repo(fs.as_ref(), Path::new("/worktree-checkout")).await;
        assert_eq!(result, Some(PathBuf::from("/main-repo")));
    }

    #[gpui::test]
    async fn test_resolve_git_worktree_normal_repo_returns_none(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/repo",
            json!({
                ".git": {},
                "src": { "main.rs": "" }
            }),
        )
        .await;

        let result = resolve_git_worktree_to_main_repo(fs.as_ref(), Path::new("/repo")).await;
        assert_eq!(result, None);
    }

    #[gpui::test]
    async fn test_resolve_git_worktree_bare_repo_identity_path(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/monty/.bare",
            json!({
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            }),
        )
        .await;
        fs.insert_tree(
            "/monty/feature-a",
            json!({
                ".git": "gitdir: /monty/.bare/worktrees/feature-a",
                "src": { "main.rs": "" }
            }),
        )
        .await;

        let result =
            resolve_git_worktree_to_main_repo(fs.as_ref(), Path::new("/monty/feature-a")).await;
        assert_eq!(result, Some(PathBuf::from("/monty")));
    }

    #[gpui::test]
    async fn test_resolve_git_worktree_no_git_returns_none(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/plain",
            json!({
                "src": { "main.rs": "" }
            }),
        )
        .await;

        let result = resolve_git_worktree_to_main_repo(fs.as_ref(), Path::new("/plain")).await;
        assert_eq!(result, None);
    }

    #[gpui::test]
    async fn test_resolve_git_worktree_nonexistent_returns_none(cx: &mut TestAppContext) {
        let fs = FakeFs::new(cx.executor());

        let result =
            resolve_git_worktree_to_main_repo(fs.as_ref(), Path::new("/does-not-exist")).await;
        assert_eq!(result, None);
    }

    #[test]
    fn test_repo_identity_path() {
        let examples = [
            // Normal checkout: `.git` starts with `.`, so parent is the worktree
            ("/home/bob/mav/.git", "/home/bob/mav"),
            // Bare clone named `.bare`: starts with `.`, so parent is the project dir
            ("/repos/project/.bare", "/repos/project"),
            // Bare clone with `.git` extension: does not start with `.`, kept as-is
            ("/repos/mav.git", "/repos/mav.git"),
            // Bare clone with arbitrary plain name: kept as-is
            ("/repos/project", "/repos/project"),
        ];
        for (common_dir, expected) in examples {
            assert_eq!(
                repo_identity_path(Path::new(common_dir)),
                Path::new(expected),
                "identity path for common_dir {common_dir:?} should be {expected:?}"
            );
        }
    }

    #[test]
    fn test_linked_worktree_short_name() {
        let examples = [
            (
                "/home/bob/mav",
                "/home/bob/worktrees/olivetti/mav",
                Some("olivetti".into()),
            ),
            ("/home/bob/mav", "/home/bob/mav2", Some("mav2".into())),
            (
                "/home/bob/mav",
                "/home/bob/worktrees/mav/selectric",
                Some("selectric".into()),
            ),
            ("/home/bob/mav", "/home/bob/mav", None),
        ];
        for (main_worktree_path, linked_worktree_path, expected) in examples {
            let short_name = linked_worktree_short_name(
                Path::new(main_worktree_path),
                Path::new(linked_worktree_path),
            );
            assert_eq!(
                short_name, expected,
                "short name for {linked_worktree_path:?}, linked worktree of {main_worktree_path:?}, should be {expected:?}"
            );
        }
    }
}
