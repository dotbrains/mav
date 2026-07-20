mod trust_tests {
    use collections::HashSet;
    use fs::FakeFs;
    use gpui::TestAppContext;
    use project::trusted_worktrees::*;

    use serde_json::json;
    use settings::SettingsStore;
    use util::path;

    use crate::Project;

    fn init_test(cx: &mut TestAppContext) {
        zlog::init_test();

        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    #[gpui::test]
    async fn test_repository_defaults_to_untrusted_without_trust_system(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "a.txt": "hello",
            }),
        )
        .await;

        // Create project without trust system — repos should default to untrusted.
        let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        repository.read_with(cx, |repo, _| {
            assert!(
                !repo.is_trusted(),
                "repository should default to untrusted when no trust system is initialized"
            );
        });
    }

    #[gpui::test]
    async fn test_multiple_repos_trust_with_single_worktree(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "a.txt": "hello",
                "sub": {
                    ".git": {},
                    "b.txt": "world",
                },
            }),
        )
        .await;

        cx.update(|cx| {
            init(DbTrustedPaths::default(), cx);
        });

        let project =
            Project::test_with_worktree_trust(fs.clone(), [path!("/project").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
        let worktree_id = worktree_store.read_with(cx, |store, cx| {
            store.worktrees().next().unwrap().read(cx).id()
        });

        let repos = project.read_with(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .cloned()
                .collect::<Vec<_>>()
        });
        assert_eq!(repos.len(), 2, "should have two repositories");
        for repo in &repos {
            repo.read_with(cx, |repo, _| {
                assert!(
                    !repo.is_trusted(),
                    "all repos should be untrusted initially"
                );
            });
        }

        let trusted_worktrees = cx
            .update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global should be set"));
        trusted_worktrees.update(cx, |store, cx| {
            store.trust(
                &worktree_store,
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
        cx.executor().run_until_parked();

        for repo in &repos {
            repo.read_with(cx, |repo, _| {
                assert!(
                    repo.is_trusted(),
                    "all repos should be trusted after worktree is trusted"
                );
            });
        }
    }

    #[gpui::test]
    async fn test_repository_trust_restrict_trust_cycle(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "a.txt": "hello",
            }),
        )
        .await;

        cx.update(|cx| {
            project::trusted_worktrees::init(DbTrustedPaths::default(), cx);
        });

        let project =
            Project::test_with_worktree_trust(fs.clone(), [path!("/project").as_ref()], cx).await;
        cx.executor().run_until_parked();

        let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
        let worktree_id = worktree_store.read_with(cx, |store, cx| {
            store.worktrees().next().unwrap().read(cx).id()
        });

        let repository = project.read_with(cx, |project, cx| {
            project.repositories(cx).values().next().unwrap().clone()
        });

        repository.read_with(cx, |repo, _| {
            assert!(!repo.is_trusted(), "repository should start untrusted");
        });

        let trusted_worktrees = cx
            .update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global should be set"));

        trusted_worktrees.update(cx, |store, cx| {
            store.trust(
                &worktree_store,
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
        cx.executor().run_until_parked();

        repository.read_with(cx, |repo, _| {
            assert!(
                repo.is_trusted(),
                "repository should be trusted after worktree is trusted"
            );
        });

        trusted_worktrees.update(cx, |store, cx| {
            store.restrict(
                worktree_store.downgrade(),
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
        cx.executor().run_until_parked();

        repository.read_with(cx, |repo, _| {
            assert!(
                !repo.is_trusted(),
                "repository should be untrusted after worktree is restricted"
            );
        });

        trusted_worktrees.update(cx, |store, cx| {
            store.trust(
                &worktree_store,
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
        cx.executor().run_until_parked();

        repository.read_with(cx, |repo, _| {
            assert!(
                repo.is_trusted(),
                "repository should be trusted again after second trust"
            );
        });
    }
}
