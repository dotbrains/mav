use crate::trusted_worktrees::*;

#[gpui::test]
async fn test_auto_trust_all(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/"),
        json!({
            "project_a": { "main.rs": "fn main() {}" },
            "project_b": { "lib.rs": "pub fn lib() {}" },
            "single.rs": "fn single() {}"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/project_a").as_ref(),
            path!("/project_b").as_ref(),
            path!("/single.rs").as_ref(),
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
    assert_eq!(worktree_ids.len(), 3);

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

    for &worktree_id in &worktree_ids {
        let can_trust = trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, worktree_id, cx)
        });
        assert!(!can_trust, "worktree should be restricted initially");
    }

    let has_restricted = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(has_restricted, "should have restricted worktrees");

    events.borrow_mut().clear();

    trusted_worktrees.update(cx, |store, cx| {
        store.auto_trust_all(cx);
    });

    for &worktree_id in &worktree_ids {
        let can_trust = trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, worktree_id, cx)
        });
        assert!(
            can_trust,
            "worktree {worktree_id:?} should be trusted after auto_trust_all"
        );
    }

    let has_restricted_after = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(
        !has_restricted_after,
        "should have no restricted worktrees after auto_trust_all"
    );

    let trusted_event_count = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, TrustedWorktreesEvent::Trusted(..)))
        .count();
    assert!(
        trusted_event_count > 0,
        "should have emitted Trusted events"
    );
}

#[gpui::test]
async fn test_trust_restrict_trust_cycle(cx: &mut TestAppContext) {
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
    assert!(!can_trust, "should be restricted initially");
    assert_eq!(events.borrow().len(), 1);
    events.borrow_mut().clear();

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(can_trust, "should be trusted after trust()");
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(
        &events.borrow()[0],
        TrustedWorktreesEvent::Trusted(..)
    ));
    events.borrow_mut().clear();

    trusted_worktrees.update(cx, |store, cx| {
        store.restrict(
            worktree_store.downgrade(),
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(!can_trust, "should be restricted after restrict()");
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(
        &events.borrow()[0],
        TrustedWorktreesEvent::Restricted(..)
    ));

    let has_restricted = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(has_restricted);
    events.borrow_mut().clear();

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(can_trust, "should be trusted again after second trust()");
    assert_eq!(events.borrow().len(), 1);
    assert!(matches!(
        &events.borrow()[0],
        TrustedWorktreesEvent::Trusted(..)
    ));

    let has_restricted = trusted_worktrees.read_with(cx, |store, cx| {
        store.has_restricted_worktrees(&worktree_store, cx)
    });
    assert!(!has_restricted);
}

#[gpui::test]
async fn test_multi_host_trust_isolation(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/"),
        json!({
            "local_project": { "main.rs": "fn main() {}" },
            "remote_project": { "lib.rs": "pub fn lib() {}" }
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            path!("/local_project").as_ref(),
            path!("/remote_project").as_ref(),
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
    let local_worktree = worktree_ids[0];
    let _remote_worktree = worktree_ids[1];

    let trusted_worktrees = init_trust_global(worktree_store.clone(), cx);

    let can_trust_local = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, local_worktree, cx)
    });
    assert!(!can_trust_local, "local worktree restricted on host_a");

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            HashSet::from_iter([PathTrust::Worktree(local_worktree)]),
            cx,
        );
    });

    let can_trust_local_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, local_worktree, cx)
    });
    assert!(
        can_trust_local_after,
        "local worktree should be trusted on local host"
    );
}

#[gpui::test]
async fn test_invisible_worktree_stores_do_not_affect_trust(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/"),
        json!({
            "visible": { "main.rs": "fn main() {}" },
            "other": { "a.rs": "fn other() {}" },
            "invisible": { "b.rs": "fn invisible() {}" }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/visible").as_ref()], cx).await;
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());
    let visible_worktree_id = worktree_store.read_with(cx, |store, cx| {
        store
            .worktrees()
            .find(|worktree| worktree.read(cx).root_dir().unwrap().ends_with("visible"))
            .expect("visible worktree")
            .read(cx)
            .id()
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

    assert!(
        !trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, visible_worktree_id, cx)
        }),
        "visible worktree should be restricted initially"
    );
    assert_eq!(
        HashSet::from_iter([(visible_worktree_id)]),
        trusted_worktrees.read_with(cx, |store, _| {
            store.restricted_worktrees_for_store(&worktree_store)
        }),
        "only visible worktree should be restricted",
    );

    let (new_visible_worktree, new_invisible_worktree) =
        worktree_store.update(cx, |worktree_store, cx| {
            let new_visible_worktree = worktree_store.create_worktree("/other", true, cx);
            let new_invisible_worktree = worktree_store.create_worktree("/invisible", false, cx);
            (new_visible_worktree, new_invisible_worktree)
        });
    let (new_visible_worktree, new_invisible_worktree) = (
        new_visible_worktree.await.unwrap(),
        new_invisible_worktree.await.unwrap(),
    );

    let new_visible_worktree_id =
        new_visible_worktree.read_with(cx, |new_visible_worktree, _| new_visible_worktree.id());
    assert!(
        !trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, new_visible_worktree_id, cx)
        }),
        "new visible worktree should be restricted initially",
    );
    assert!(
        trusted_worktrees.update(cx, |store, cx| {
            store.can_trust(&worktree_store, new_invisible_worktree.read(cx).id(), cx)
        }),
        "invisible worktree should be skipped",
    );
    assert_eq!(
        HashSet::from_iter([visible_worktree_id, new_visible_worktree_id]),
        trusted_worktrees.read_with(cx, |store, _| {
            store.restricted_worktrees_for_store(&worktree_store)
        }),
        "only visible worktrees should be restricted"
    );
}
