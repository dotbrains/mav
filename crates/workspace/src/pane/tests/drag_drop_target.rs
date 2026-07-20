use super::*;

async fn test_handle_tab_drop_respects_is_pane_target(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let source_pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&source_pane, "A", false, cx);
    let item_b = add_labeled_item(&source_pane, "B", false, cx);

    let target_pane = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(source_pane.clone(), SplitDirection::Right, window, cx)
    });

    let custom_item = target_pane.update_in(cx, |pane, window, cx| {
        let custom_item = Box::new(cx.new(CustomDropHandlingItem::new));
        pane.add_item(custom_item.clone(), true, true, None, window, cx);
        custom_item
    });

    let moved_item_id = item_a.item_id();
    let other_item_id = item_b.item_id();
    let custom_item_id = custom_item.item_id();

    let pane_item_ids = |pane: &Entity<Pane>, cx: &mut VisualTestContext| {
        pane.read_with(cx, |pane, _| {
            pane.items().map(|item| item.item_id()).collect::<Vec<_>>()
        })
    };

    let source_before_item_ids = pane_item_ids(&source_pane, cx);
    assert_eq!(source_before_item_ids, vec![moved_item_id, other_item_id]);

    let target_before_item_ids = pane_item_ids(&target_pane, cx);
    assert_eq!(target_before_item_ids, vec![custom_item_id]);

    let dragged_tab = DraggedTab {
        pane: source_pane.clone(),
        item: item_a.boxed_clone(),
        ix: 0,
        detail: 0,
        is_active: true,
    };

    // Dropping item_a onto the target pane itself means the
    // custom item handles the drop and no tab move should occur
    target_pane.update_in(cx, |pane, window, cx| {
        pane.handle_tab_drop(&dragged_tab, pane.active_item_index(), true, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        custom_item.read_with(cx, |item, _| item.drop_call_count()),
        1
    );
    assert_eq!(pane_item_ids(&source_pane, cx), source_before_item_ids);
    assert_eq!(pane_item_ids(&target_pane, cx), target_before_item_ids);

    // Dropping item_a onto the tab target means the custom handler
    // should be skipped and the pane's default tab drop behavior should run.
    target_pane.update_in(cx, |pane, window, cx| {
        pane.handle_tab_drop(&dragged_tab, pane.active_item_index(), false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        custom_item.read_with(cx, |item, _| item.drop_call_count()),
        1
    );
    assert_eq!(pane_item_ids(&source_pane, cx), vec![other_item_id]);

    let target_item_ids = pane_item_ids(&target_pane, cx);
    assert_eq!(target_item_ids, vec![moved_item_id, custom_item_id]);
}

#[gpui::test]
async fn test_drag_unpinned_tab_to_split_creates_pane_with_unpinned_tab(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B. Pin B. Activate A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);

    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.activate_item(ix, true, true, window, cx);
    });

    // Drag A to create new split
    pane_a.update_in(cx, |pane, window, cx| {
        pane.drag_split_direction = Some(SplitDirection::Right);

        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, true, window, cx);
    });

    // A should be moved to new pane. B should remain pinned, A should not be pinned
    let (pane_a, pane_b) = workspace.read_with(cx, |workspace, _| {
        let panes = workspace.panes();
        (panes[0].clone(), panes[1].clone())
    });
    assert_item_labels(&pane_a, ["B*!"], cx);
    assert_item_labels(&pane_b, ["A*"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_to_split_creates_pane_with_pinned_tab(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B. Pin both. Activate A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);

    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.activate_item(ix, true, true, window, cx);
    });
    assert_item_labels(&pane_a, ["A*!", "B!"], cx);

    // Drag A to create new split
    pane_a.update_in(cx, |pane, window, cx| {
        pane.drag_split_direction = Some(SplitDirection::Right);

        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, true, window, cx);
    });

    // A should be moved to new pane. Both A and B should still be pinned
    let (pane_a, pane_b) = workspace.read_with(cx, |workspace, _| {
        let panes = workspace.panes();
        (panes[0].clone(), panes[1].clone())
    });
    assert_item_labels(&pane_a, ["B*!"], cx);
    assert_item_labels(&pane_b, ["A*!"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_into_existing_panes_pinned_region(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A to pane A and pin
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A*!"], cx);

    // Add B to pane B and pin
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });
    let item_b = add_labeled_item(&pane_b, "B", false, cx);
    pane_b.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_b, ["B*!"], cx);

    // Move A from pane A to pane B's pinned region
    pane_b.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // A should stay pinned
    assert_item_labels(&pane_a, [], cx);
    assert_item_labels(&pane_b, ["A*!", "B!"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_into_existing_panes_unpinned_region(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A to pane A and pin
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A*!"], cx);

    // Create pane B with pinned item B
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });
    let item_b = add_labeled_item(&pane_b, "B", false, cx);
    assert_item_labels(&pane_b, ["B*"], cx);

    pane_b.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_b, ["B*!"], cx);

    // Move A from pane A to pane B's unpinned region
    pane_b.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
    });

    // A should become pinned
    assert_item_labels(&pane_a, [], cx);
    assert_item_labels(&pane_b, ["B!", "A*"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_into_existing_panes_first_position_with_no_pinned_tabs(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A to pane A and pin
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A*!"], cx);

    // Add B to pane B
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });
    add_labeled_item(&pane_b, "B", false, cx);
    assert_item_labels(&pane_b, ["B*"], cx);

    // Move A from pane A to position 0 in pane B, indicating it should stay pinned
    pane_b.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // A should stay pinned
    assert_item_labels(&pane_a, [], cx);
    assert_item_labels(&pane_b, ["A*!", "B"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_into_existing_pane_at_max_capacity_closes_unpinned_tabs(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    set_max_tabs(cx, Some(2));

    // Add A, B to pane A. Pin both
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B*!"], cx);

    // Add C, D to pane B. Pin both
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });
    let item_c = add_labeled_item(&pane_b, "C", false, cx);
    let item_d = add_labeled_item(&pane_b, "D", false, cx);
    pane_b.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_b, ["C!", "D*!"], cx);

    // Add a third unpinned item to pane B (exceeds max tabs), but is allowed,
    // as we allow 1 tab over max if the others are pinned or dirty
    add_labeled_item(&pane_b, "E", false, cx);
    assert_item_labels(&pane_b, ["C!", "D!", "E*"], cx);

    // Drag pinned A from pane A to position 0 in pane B
    pane_b.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // E (unpinned) should be closed, leaving 3 pinned items
    assert_item_labels(&pane_a, ["B*!"], cx);
    assert_item_labels(&pane_b, ["A*!", "C!", "D!"], cx);
}

#[gpui::test]
async fn test_drag_last_pinned_tab_to_same_position_stays_pinned(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A to pane A and pin it
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A*!"], cx);

    // Drag pinned A to position 1 (directly to the right) in the same pane
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
    });

    // A should still be pinned and active
    assert_item_labels(&pane_a, ["A*!"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_beyond_last_pinned_tab_in_same_pane_stays_pinned(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B to pane A and pin both
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B*!"], cx);

    // Drag pinned A right of B in the same pane
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
    });

    // A stays pinned
    assert_item_labels(&pane_a, ["B!", "A*!"], cx);
}
