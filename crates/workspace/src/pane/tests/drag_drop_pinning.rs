use super::*;

async fn test_dragging_pinned_tab_onto_unpinned_tab_reduces_unpinned_tab_count(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B to pane A and pin A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    add_labeled_item(&pane_a, "B", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B*"], cx);

    // Drag pinned A on top of B in the same pane, which changes tab order to B, A
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

    // Neither are pinned
    assert_item_labels(&pane_a, ["B", "A*"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_beyond_unpinned_tab_in_same_pane_becomes_unpinned(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B to pane A and pin A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    add_labeled_item(&pane_a, "B", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B*"], cx);

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

    // A becomes unpinned
    assert_item_labels(&pane_a, ["B", "A*"], cx);
}

#[gpui::test]
async fn test_drag_unpinned_tab_in_front_of_pinned_tab_in_same_pane_becomes_pinned(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B to pane A and pin A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B*"], cx);

    // Drag pinned B left of A in the same pane
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_b.boxed_clone(),
            ix: 1,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // A becomes unpinned
    assert_item_labels(&pane_a, ["B*!", "A!"], cx);
}

#[gpui::test]
async fn test_drag_unpinned_tab_to_the_pinned_region_stays_pinned(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C to pane A and pin A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    add_labeled_item(&pane_a, "B", false, cx);
    let item_c = add_labeled_item(&pane_a, "C", false, cx);
    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B", "C*"], cx);

    // Drag pinned C left of B in the same pane
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_c.boxed_clone(),
            ix: 2,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
    });

    // A stays pinned, B and C remain unpinned
    assert_item_labels(&pane_a, ["A!", "C*", "B"], cx);
}

#[gpui::test]
async fn test_drag_unpinned_tab_into_existing_panes_pinned_region(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add unpinned item A to pane A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    assert_item_labels(&pane_a, ["A*"], cx);

    // Create pane B with pinned item B
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

    // A should become pinned since it was dropped in the pinned region
    assert_item_labels(&pane_a, [], cx);
    assert_item_labels(&pane_b, ["A*!", "B!"], cx);
}

#[gpui::test]
async fn test_drag_unpinned_tab_into_existing_panes_unpinned_region(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add unpinned item A to pane A
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    assert_item_labels(&pane_a, ["A*"], cx);

    // Create pane B with one pinned item B
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });
    let item_b = add_labeled_item(&pane_b, "B", false, cx);
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
        pane.handle_tab_drop(&dragged_tab, 1, true, window, cx);
    });

    // A should remain unpinned since it was dropped outside the pinned region
    assert_item_labels(&pane_a, [], cx);
    assert_item_labels(&pane_b, ["B!", "A*"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_throughout_entire_range_of_pinned_tabs_both_directions(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C and pin all
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    let item_b = add_labeled_item(&pane_a, "B", false, cx);
    let item_c = add_labeled_item(&pane_a, "C", false, cx);
    assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

    pane_a.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane_a, ["A!", "B!", "C*!"], cx);

    // Move A to right of B
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

    // A should be after B and all are pinned
    assert_item_labels(&pane_a, ["B!", "A*!", "C!"], cx);

    // Move A to right of C
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 1,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
    });

    // A should be after C and all are pinned
    assert_item_labels(&pane_a, ["B!", "C!", "A*!"], cx);

    // Move A to left of C
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 2,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
    });

    // A should be before C and all are pinned
    assert_item_labels(&pane_a, ["B!", "A*!", "C!"], cx);

    // Move A to left of B
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_a.boxed_clone(),
            ix: 1,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // A should be before B and all are pinned
    assert_item_labels(&pane_a, ["A*!", "B!", "C!"], cx);
}

#[gpui::test]
async fn test_drag_first_tab_to_last_position(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C
    let item_a = add_labeled_item(&pane_a, "A", false, cx);
    add_labeled_item(&pane_a, "B", false, cx);
    add_labeled_item(&pane_a, "C", false, cx);
    assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

    // Move A to the end
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

    // A should be at the end
    assert_item_labels(&pane_a, ["B", "C", "A*"], cx);
}

#[gpui::test]
async fn test_drag_last_tab_to_first_position(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C
    add_labeled_item(&pane_a, "A", false, cx);
    add_labeled_item(&pane_a, "B", false, cx);
    let item_c = add_labeled_item(&pane_a, "C", false, cx);
    assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

    // Move C to the beginning
    pane_a.update_in(cx, |pane, window, cx| {
        let dragged_tab = DraggedTab {
            pane: pane_a.clone(),
            item: item_c.boxed_clone(),
            ix: 2,
            detail: 0,
            is_active: true,
        };
        pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
    });

    // C should be at the beginning
    assert_item_labels(&pane_a, ["C*", "A", "B"], cx);
}
