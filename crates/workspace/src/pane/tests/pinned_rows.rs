use super::*;

async fn test_separate_pinned_row_disabled_by_default(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);

    // Pin one tab
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B", "C*"], cx);

    // Verify setting is disabled by default
    let is_separate_row_enabled = pane.read_with(cx, |_, cx| {
        TabBarSettings::get_global(cx).show_pinned_tabs_in_separate_row
    });
    assert!(
        !is_separate_row_enabled,
        "Separate pinned row should be disabled by default"
    );

    // Verify pinned_tabs_row element does NOT exist (single row layout)
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_none(),
        "pinned_tabs_row should not exist when setting is disabled"
    );
}

#[gpui::test]
async fn test_separate_pinned_row_two_rows_when_both_tab_types_exist(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Enable separate row setting
    set_pinned_tabs_separate_row(cx, true);

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);

    // Pin one tab - now we have both pinned and unpinned tabs
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B", "C*"], cx);

    // Verify pinned_tabs_row element exists (two row layout)
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_some(),
        "pinned_tabs_row should exist when setting is enabled and both tab types exist"
    );
}

#[gpui::test]
async fn test_separate_pinned_row_single_row_when_only_pinned_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Enable separate row setting
    set_pinned_tabs_separate_row(cx, true);

    let item_a = add_labeled_item(&pane, "A", false, cx);
    let item_b = add_labeled_item(&pane, "B", false, cx);

    // Pin all tabs - only pinned tabs exist
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*!"], cx);

    // Verify pinned_tabs_row does NOT exist (single row layout for pinned-only)
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_none(),
        "pinned_tabs_row should not exist when only pinned tabs exist (uses single row)"
    );
}

#[gpui::test]
async fn test_separate_pinned_row_single_row_when_only_unpinned_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Enable separate row setting
    set_pinned_tabs_separate_row(cx, true);

    // Add only unpinned tabs
    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Verify pinned_tabs_row does NOT exist (single row layout for unpinned-only)
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_none(),
        "pinned_tabs_row should not exist when only unpinned tabs exist (uses single row)"
    );
}

#[gpui::test]
async fn test_separate_pinned_row_toggles_between_layouts(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);

    // Pin one tab
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    // Initially disabled - single row
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_none(),
        "Should be single row when disabled"
    );

    // Enable - two rows
    set_pinned_tabs_separate_row(cx, true);
    cx.run_until_parked();
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_some(),
        "Should be two rows when enabled"
    );

    // Disable again - back to single row
    set_pinned_tabs_separate_row(cx, false);
    cx.run_until_parked();
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_none(),
        "Should be single row when disabled again"
    );
}

#[gpui::test]
async fn test_separate_pinned_row_has_right_border(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Enable separate row setting
    set_pinned_tabs_separate_row(cx, true);

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);

    // Pin one tab - now we have both pinned and unpinned tabs (two-row layout)
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B", "C*"], cx);
    cx.run_until_parked();

    // Verify two-row layout is active
    let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
    assert!(
        pinned_row_bounds.is_some(),
        "Two-row layout should be active when both pinned and unpinned tabs exist"
    );

    // Verify pinned_tabs_border element exists (the right border after pinned tabs)
    let border_bounds = cx.debug_bounds("pinned_tabs_border");
    assert!(
        border_bounds.is_some(),
        "pinned_tabs_border should exist in two-row layout to show right border"
    );
}

#[gpui::test]
async fn test_pinning_active_tab_without_position_change_maintains_focus(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A
    let item_a = add_labeled_item(&pane, "A", false, cx);
    assert_item_labels(&pane, ["A*"], cx);

    // Add B
    add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A", "B*"], cx);

    // Activate A again
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.activate_item(ix, true, true, window, cx);
    });
    assert_item_labels(&pane, ["A*", "B"], cx);

    // Pin A - remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A*!", "B"], cx);

    // Unpin A - remain active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A*", "B"], cx);
}

#[gpui::test]
async fn test_pinning_active_tab_with_position_change_maintains_focus(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C
    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Pin C - moves to pinned area, remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C*!", "A", "B"], cx);

    // Unpin C - moves after pinned area, remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C*", "A", "B"], cx);
}

#[gpui::test]
async fn test_pinning_inactive_tab_without_position_change_preserves_existing_focus(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B
    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A", "B*"], cx);

    // Pin A - already in pinned area, B remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*"], cx);

    // Unpin A - stays in place, B remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A", "B*"], cx);
}

#[gpui::test]
async fn test_pinning_inactive_tab_with_position_change_preserves_existing_focus(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add A, B, C
    add_labeled_item(&pane, "A", false, cx);
    let item_b = add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Activate B
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.activate_item(ix, true, true, window, cx);
    });
    assert_item_labels(&pane, ["A", "B*", "C"], cx);

    // Pin C - moves to pinned area, B remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C!", "A", "B*"], cx);

    // Unpin C - moves after pinned area, B remains active
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C", "A", "B*"], cx);
}
