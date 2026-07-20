use crate::trusted_worktrees::*;

#[gpui::test]
async fn test_two_directory_worktrees_separate_trust(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/projects"),
        json!({
            "project_a": { "main.rs": "fn main() {}" },
            "project_b": { "lib.rs": "pub fn lib() {}" }
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/projects/project_a").as_ref(),
            path!("/projects/project_b").as_ref(),
        ],
        cx,
    )
    .await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let worktree_ids: Vec<_> = worktree_store.read_with(cx, |store, cx| {
        store
            .worktrees()
            .map(|worktree| {
                let worktree = worktree.read(cx);
                assert!(!worktree.is_single_file());
                worktree.id()
            })
            .collect()
    });
    assert_eq!(worktree_ids.len(), 2);

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let can_trust_a = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(!can_trust_a, "project_a should be restricted initially");
    assert!(!can_trust_b, "project_b should be restricted initially");

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_ids[0])]),
            cx,
        );
    });

    let can_trust_a = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(can_trust_a, "project_a should be trusted after trust()");
    assert!(!can_trust_b, "project_b should still be restricted");

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_ids[1])]),
            cx,
        );
    });

    let can_trust_a = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_b = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    assert!(can_trust_a, "project_a should remain trusted");
    assert!(can_trust_b, "project_b should now be trusted");
}

#[gpui::test]
async fn test_directory_worktree_trust_enables_single_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/"),
        json!({
            "project": { "main.rs": "fn main() {}" },
            "standalone.rs": "fn standalone() {}"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [path!("/project").as_ref(), path!("/standalone.rs").as_ref()],
        cx,
    )
    .await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let (dir_worktree_id, file_worktree_id) = worktree_store.read_with(cx, |store, cx| {
        let worktrees: Vec<_> = store.worktrees().collect();
        assert_eq!(worktrees.len(), 2);
        let (dir_worktree, file_worktree) = if worktrees[0].read(cx).is_single_file() {
            (&worktrees[1], &worktrees[0])
        } else {
            (&worktrees[0], &worktrees[1])
        };
        assert!(!dir_worktree.read(cx).is_single_file());
        assert!(file_worktree.read(cx).is_single_file());
        (dir_worktree.read(cx).id(), file_worktree.read(cx).id())
    });

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let can_trust_file = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, file_worktree_id, cx)
    });
    assert!(
        !can_trust_file,
        "single-file worktree should be restricted initially"
    );

    let can_trust_directory = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, dir_worktree_id, cx)
    });
    assert!(
        !can_trust_directory,
        "directory worktree should be restricted initially"
    );

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(dir_worktree_id)]),
            cx,
        );
    });

    let can_trust_dir = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, dir_worktree_id, cx)
    });
    let can_trust_file_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, file_worktree_id, cx)
    });
    assert!(can_trust_dir, "directory worktree should be trusted");
    assert!(
        can_trust_file_after,
        "single-file worktree should be trusted after directory worktree trust"
    );
}

#[gpui::test]
async fn test_parent_path_trust_enables_single_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/"),
        json!({
            "project": { "main.rs": "fn main() {}" },
            "standalone.rs": "fn standalone() {}"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [path!("/project").as_ref(), path!("/standalone.rs").as_ref()],
        cx,
    )
    .await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let (dir_worktree_id, file_worktree_id) = worktree_store.read_with(cx, |store, cx| {
        let worktrees: Vec<_> = store.worktrees().collect();
        assert_eq!(worktrees.len(), 2);
        let (dir_worktree, file_worktree) = if worktrees[0].read(cx).is_single_file() {
            (&worktrees[1], &worktrees[0])
        } else {
            (&worktrees[0], &worktrees[1])
        };
        assert!(!dir_worktree.read(cx).is_single_file());
        assert!(file_worktree.read(cx).is_single_file());
        (dir_worktree.read(cx).id(), file_worktree.read(cx).id())
    });

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let can_trust_file = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, file_worktree_id, cx)
    });
    assert!(
        !can_trust_file,
        "single-file worktree should be restricted initially"
    );

    let can_trust_directory = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, dir_worktree_id, cx)
    });
    assert!(
        !can_trust_directory,
        "directory worktree should be restricted initially"
    );

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::AbsPath(PathBuf::from(path!("/project")))]),
            cx,
        );
    });

    let can_trust_dir = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, dir_worktree_id, cx)
    });
    let can_trust_file_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, file_worktree_id, cx)
    });
    assert!(
        can_trust_dir,
        "directory worktree should be trusted after its parent is trusted"
    );
    assert!(
        can_trust_file_after,
        "single-file worktree should be trusted after directory worktree trust via its parent directory trust"
    );
}

#[gpui::test]
async fn test_abs_path_trust_covers_multiple_worktrees(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project_a": { "main.rs": "fn main() {}" },
            "project_b": { "lib.rs": "pub fn lib() {}" }
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/root/project_a").as_ref(),
            path!("/root/project_b").as_ref(),
        ],
        cx,
    )
    .await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let worktree_ids: Vec<_> = worktree_store.read_with(cx, |store, cx| {
        store
            .worktrees()
            .map(|worktree| worktree.read(cx).id())
            .collect()
    });
    assert_eq!(worktree_ids.len(), 2);

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    for &worktree_id in &worktree_ids {
        let can_trust = trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, worktree_id, cx)
        });
        assert!(!can_trust, "worktree should be restricted initially");
    }

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::AbsPath(PathBuf::from(path!("/root")))]),
            cx,
        );
    });

    for &worktree_id in &worktree_ids {
        let can_trust = trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, worktree_id, cx)
        });
        assert!(
            can_trust,
            "worktree should be trusted after parent path trust"
        );
    }
}
