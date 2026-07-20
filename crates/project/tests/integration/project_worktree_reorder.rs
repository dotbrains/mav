use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_reordering_worktrees(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/dir",
        json!({
            "a.rs": "let a = 1;",
            "b.rs": "let b = 2;",
            "c.rs": "let c = 2;",
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            "/dir/a.rs".as_ref(),
            "/dir/b.rs".as_ref(),
            "/dir/c.rs".as_ref(),
        ],
        cx,
    )
    .await;

    // check the initial state and get the worktrees
    let (worktree_a, worktree_b, worktree_c) = project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let worktree_a = worktrees[0].read(cx);
        let worktree_b = worktrees[1].read(cx);
        let worktree_c = worktrees[2].read(cx);

        // check they start in the right order
        assert_eq!(worktree_a.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(worktree_b.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(worktree_c.abs_path().to_str().unwrap(), "/dir/c.rs");

        (
            worktrees[0].clone(),
            worktrees[1].clone(),
            worktrees[2].clone(),
        )
    });

    // move first worktree to after the second
    // [a, b, c] -> [b, a, c]
    project
        .update(cx, |project, cx| {
            let first = worktree_a.read(cx);
            let second = worktree_b.read(cx);
            project.move_worktree(first.id(), second.id(), cx)
        })
        .expect("moving first after second");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/c.rs");
    });

    // move the second worktree to before the first
    // [b, a, c] -> [a, b, c]
    project
        .update(cx, |project, cx| {
            let second = worktree_a.read(cx);
            let first = worktree_b.read(cx);
            project.move_worktree(first.id(), second.id(), cx)
        })
        .expect("moving second before first");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/c.rs");
    });

    // move the second worktree to after the third
    // [a, b, c] -> [a, c, b]
    project
        .update(cx, |project, cx| {
            let second = worktree_b.read(cx);
            let third = worktree_c.read(cx);
            project.move_worktree(second.id(), third.id(), cx)
        })
        .expect("moving second after third");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/c.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/b.rs");
    });

    // move the third worktree to before the second
    // [a, c, b] -> [a, b, c]
    project
        .update(cx, |project, cx| {
            let third = worktree_c.read(cx);
            let second = worktree_b.read(cx);
            project.move_worktree(third.id(), second.id(), cx)
        })
        .expect("moving third before second");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/c.rs");
    });

    // move the first worktree to after the third
    // [a, b, c] -> [b, c, a]
    project
        .update(cx, |project, cx| {
            let first = worktree_a.read(cx);
            let third = worktree_c.read(cx);
            project.move_worktree(first.id(), third.id(), cx)
        })
        .expect("moving first after third");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/c.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/a.rs");
    });

    // move the third worktree to before the first
    // [b, c, a] -> [a, b, c]
    project
        .update(cx, |project, cx| {
            let third = worktree_a.read(cx);
            let first = worktree_b.read(cx);
            project.move_worktree(third.id(), first.id(), cx)
        })
        .expect("moving third before first");

    // check the state after moving
    project.update(cx, |project, cx| {
        let worktrees = project.visible_worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 3);

        let first = worktrees[0].read(cx);
        let second = worktrees[1].read(cx);
        let third = worktrees[2].read(cx);

        // check they are now in the right order
        assert_eq!(first.abs_path().to_str().unwrap(), "/dir/a.rs");
        assert_eq!(second.abs_path().to_str().unwrap(), "/dir/b.rs");
        assert_eq!(third.abs_path().to_str().unwrap(), "/dir/c.rs");
    });
}
