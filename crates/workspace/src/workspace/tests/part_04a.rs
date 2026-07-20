use super::*;

/// Tests that the navigation history deduplicates entries for the same item.
///
/// When navigating back and forth between items (e.g., A -> B -> A -> B -> A -> B -> C),
/// the navigation history deduplicates by keeping only the most recent visit to each item,
/// resulting in [A, B, C] instead of [A, B, A, B, A, B, C]. This ensures that Go Back (Ctrl-O)
/// navigates through unique items efficiently: C -> B -> A, rather than bouncing between
/// repeated entries: C -> B -> A -> B -> A -> B -> A.
///
/// This behavior prevents the navigation history from growing unnecessarily large and provides
/// a better user experience by eliminating redundant navigation steps when jumping between files.
#[gpui::test]
async fn test_navigation_history_deduplication(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let item_a =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "a.txt", cx)]));
    let item_b =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(2, "b.txt", cx)]));
    let item_c =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(3, "c.txt", cx)]));

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_a.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(Box::new(item_b.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(Box::new(item_c.clone()), None, true, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_a, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_b, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_a, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_b, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_a, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_b, false, false, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_item(&item_c, false, false, window, cx);
    });
    cx.run_until_parked();

    let backward_count = pane.read_with(cx, |pane, cx| {
        let mut count = 0;
        pane.nav_history().for_each_entry(cx, &mut |_, _| {
            count += 1;
        });
        count
    });
    assert!(
        backward_count <= 4,
        "Should have at most 4 entries, got {}",
        backward_count
    );

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    let active_item = workspace.read_with(cx, |workspace, cx| {
        workspace.active_item(cx).unwrap().item_id()
    });
    assert_eq!(
        active_item,
        item_b.entity_id(),
        "After first go_back, should be at item B"
    );

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    let active_item = workspace.read_with(cx, |workspace, cx| {
        workspace.active_item(cx).unwrap().item_id()
    });
    assert_eq!(
        active_item,
        item_a.entity_id(),
        "After second go_back, should be at item A"
    );

    pane.read_with(cx, |pane, _| {
        assert!(pane.can_navigate_forward(), "Should be able to go forward");
    });
}

#[gpui::test]
async fn test_activate_last_pane(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace.update_in(cx, |workspace, window, cx| {
        let first_item = cx.new(|cx| {
            TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
        });
        workspace.add_item_to_active_pane(Box::new(first_item), None, true, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );
    });

    let (first_pane_id, target_last_pane_id) = workspace.update(cx, |workspace, _cx| {
        let panes = workspace.center.panes();
        assert!(panes.len() >= 2);
        (
            panes.first().expect("at least one pane").entity_id(),
            panes.last().expect("at least one pane").entity_id(),
        )
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_pane_at_index(&ActivatePane(0), window, cx);
    });
    workspace.update(cx, |workspace, _| {
        assert_eq!(workspace.active_pane().entity_id(), first_pane_id);
        assert_ne!(workspace.active_pane().entity_id(), target_last_pane_id);
    });

    cx.dispatch_action(ActivateLastPane);

    workspace.update(cx, |workspace, _| {
        assert_eq!(workspace.active_pane().entity_id(), target_last_pane_id);
    });
}

#[gpui::test]
async fn test_reset_pane_sizes(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // A horizontal split of three panes whose last child is itself a vertical
    // split, so equalizing has to recurse into the nested axis.
    workspace.update_in(cx, |workspace, window, cx| {
        let item = cx.new(|cx| {
            TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
        });
        workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Down,
            window,
            cx,
        );
    });

    let nested_axis = |workspace: &Workspace| {
        let Member::Axis(top) = &workspace.center.root else {
            panic!("expected the center to be a split axis");
        };
        let nested = top
            .members
            .iter()
            .find_map(|member| match member {
                Member::Axis(axis) => Some(axis.clone()),
                Member::Pane(_) => None,
            })
            .expect("expected a nested split axis");
        (top.clone(), nested)
    };

    // Skew every axis away from uniform sizes.
    workspace.update(cx, |workspace, _| {
        let (top, nested) = nested_axis(workspace);
        *top.flexes.lock() = vec![1.6, 0.7, 0.7];
        *nested.flexes.lock() = vec![1.3, 0.7];
    });

    cx.run_until_parked();
    cx.dispatch_action(ResetPaneSizes);

    workspace.update(cx, |workspace, _| {
        let (top, nested) = nested_axis(workspace);
        assert_eq!(*top.flexes.lock(), vec![1.0; top.members.len()]);
        assert_eq!(*nested.flexes.lock(), vec![1.0; nested.members.len()]);
    });
}
