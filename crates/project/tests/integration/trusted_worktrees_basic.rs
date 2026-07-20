use crate::trusted_worktrees::*;

#[gpui::test]
async fn test_single_worktree_trust(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let worktree_id = worktree_store.read_with(cx, |store, cx| {
        store.worktrees().next().unwrap().read(cx).id()
    });

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let events: Rc<RefCell<Vec<TrustedWorktreesEvent>>> = Rc::default();
    cx.update({
        let events = events.clone();
        |cx| {
            cx.subscribe(&trusted_worktrees, move |_, event, _| {
                events.borrow_mut().push(match event {
                    TrustedWorktreesEvent::Trusted(host, paths) => {
                        TrustedWorktreesEvent::Trusted(host.clone(), paths.clone())
                    }
                    TrustedWorktreesEvent::Restricted(host, paths) => {
                        TrustedWorktreesEvent::Restricted(host.clone(), paths.clone())
                    }
                });
            })
        }
    })
    .detach();

    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(!can_trust, "worktree should be restricted by default");

    {
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TrustedWorktreesEvent::Restricted(event_worktree_store, paths) => {
                assert_eq!(event_worktree_store, &worktree_store.downgrade());
                assert!(paths.contains(&PathTrust::Worktree(worktree_id)));
            }
            _ => panic!("expected Restricted event"),
        }
    }

    let has_restricted = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(has_restricted, "should have restricted worktrees");

    let restricted = trusted_worktrees.read_with(cx, |trusted_worktrees, cx| {
        trusted_worktrees.restricted_worktrees(&worktree_store, cx)
    });
    assert!(restricted.iter().any(|(id, _)| *id == worktree_id));

    events.borrow_mut().clear();

    let can_trust_again = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(!can_trust_again, "worktree should still be restricted");
    assert!(
        events.borrow().is_empty(),
        "no duplicate Restricted event on repeated can_trust"
    );

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });

    {
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TrustedWorktreesEvent::Trusted(event_worktree_store, paths) => {
                assert_eq!(event_worktree_store, &worktree_store.downgrade());
                assert!(paths.contains(&PathTrust::Worktree(worktree_id)));
            }
            _ => panic!("expected Trusted event"),
        }
    }

    let can_trust_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(can_trust_after, "worktree should be trusted after trust()");

    let has_restricted_after = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(
        !has_restricted_after,
        "should have no restricted worktrees after trust"
    );

    let restricted_after = trusted_worktrees.read_with(cx, |trusted_worktrees, cx| {
        trusted_worktrees.restricted_worktrees(&worktree_store, cx)
    });
    assert!(
        restricted_after.is_empty(),
        "restricted set should be empty"
    );
}

#[gpui::test]
async fn test_single_file_worktree_trust(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "foo.rs": "fn foo() {}" }))
        .await;

    let project = Project::test(fs, [path!("/root/foo.rs").as_ref()], cx).await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let worktree_id = worktree_store.read_with(cx, |store, cx| {
        let worktree = store.worktrees().next().unwrap();
        let worktree = worktree.read(cx);
        assert!(worktree.is_single_file(), "expected single-file worktree");
        worktree.id()
    });

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let events: Rc<RefCell<Vec<TrustedWorktreesEvent>>> = Rc::default();
    cx.update({
        let events = events.clone();
        |cx| {
            cx.subscribe(&trusted_worktrees, move |_, event, _| {
                events.borrow_mut().push(match event {
                    TrustedWorktreesEvent::Trusted(host, paths) => {
                        TrustedWorktreesEvent::Trusted(host.clone(), paths.clone())
                    }
                    TrustedWorktreesEvent::Restricted(host, paths) => {
                        TrustedWorktreesEvent::Restricted(host.clone(), paths.clone())
                    }
                });
            })
        }
    })
    .detach();

    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(
        !can_trust,
        "single-file worktree should be restricted by default"
    );

    {
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TrustedWorktreesEvent::Restricted(event_worktree_store, paths) => {
                assert_eq!(event_worktree_store, &worktree_store.downgrade());
                assert!(paths.contains(&PathTrust::Worktree(worktree_id)));
            }
            _ => panic!("expected Restricted event"),
        }
    }

    events.borrow_mut().clear();

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });

    {
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TrustedWorktreesEvent::Trusted(event_worktree_store, paths) => {
                assert_eq!(event_worktree_store, &worktree_store.downgrade());
                assert!(paths.contains(&PathTrust::Worktree(worktree_id)));
            }
            _ => panic!("expected Trusted event"),
        }
    }

    let can_trust_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(
        can_trust_after,
        "single-file worktree should be trusted after trust()"
    );
}

#[gpui::test]
async fn test_multiple_single_file_worktrees_trust_one(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a.rs": "fn a() {}",
            "b.rs": "fn b() {}",
            "c.rs": "fn c() {}"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/root/a.rs").as_ref(),
            path!("/root/b.rs").as_ref(),
            path!("/root/c.rs").as_ref(),
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
                assert!(worktree.is_single_file());
                worktree.id()
            })
            .collect()
    });
    assert_eq!(worktree_ids.len(), 3);

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    for &worktree_id in &worktree_ids {
        let can_trust = trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, worktree_id, cx)
        });
        assert!(
            !can_trust,
            "worktree {worktree_id:?} should be restricted initially"
        );
    }

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_ids[1])]),
            cx,
        );
    });

    let can_trust_0 = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[0], cx)
    });
    let can_trust_1 = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[1], cx)
    });
    let can_trust_2 = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_ids[2], cx)
    });

    assert!(!can_trust_0, "worktree 0 should still be restricted");
    assert!(can_trust_1, "worktree 1 should be trusted");
    assert!(!can_trust_2, "worktree 2 should still be restricted");
}
