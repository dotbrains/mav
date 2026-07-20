use super::*;

async fn test_add_item_capped_to_max_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    for i in 0..7 {
        add_labeled_item(&pane, format!("{}", i).as_str(), false, cx);
    }

    set_max_tabs(cx, Some(5));
    add_labeled_item(&pane, "7", false, cx);
    // Remove items to respect the max tab cap.
    assert_item_labels(&pane, ["3", "4", "5", "6", "7*"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(0, false, false, window, cx);
    });
    add_labeled_item(&pane, "X", false, cx);
    // Respect activation order.
    assert_item_labels(&pane, ["3", "X*", "5", "6", "7"], cx);

    for i in 0..7 {
        add_labeled_item(&pane, format!("D{}", i).as_str(), true, cx);
    }
    // Keeps dirty items, even over max tab cap.
    assert_item_labels(
        &pane,
        ["D0^", "D1^", "D2^", "D3^", "D4^", "D5^", "D6*^"],
        cx,
    );

    set_max_tabs(cx, None);
    for i in 0..7 {
        add_labeled_item(&pane, format!("N{}", i).as_str(), false, cx);
    }
    // No cap when max tabs is None.
    assert_item_labels(
        &pane,
        [
            "D0^", "D1^", "D2^", "D3^", "D4^", "D5^", "D6^", "N0", "N1", "N2", "N3", "N4", "N5",
            "N6*",
        ],
        cx,
    );
}

#[gpui::test]
async fn test_reduce_max_tabs_closes_existing_items(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    let item_d = add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    add_labeled_item(&pane, "Settings", false, cx);
    assert_item_labels(&pane, ["A", "B", "C", "D", "E", "Settings*"], cx);

    set_max_tabs(cx, Some(5));
    assert_item_labels(&pane, ["B", "C", "D", "E", "Settings*"], cx);

    set_max_tabs(cx, Some(4));
    assert_item_labels(&pane, ["C", "D", "E", "Settings*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C!", "D!", "E", "Settings*"], cx);

    set_max_tabs(cx, Some(2));
    assert_item_labels(&pane, ["C!", "D!", "Settings*"], cx);
}

#[gpui::test]
async fn test_allow_pinning_dirty_item_at_max_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(1));
    let item_a = add_labeled_item(&pane, "A", true, cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A*^!"], cx);
}

#[gpui::test]
async fn test_allow_pinning_non_dirty_item_at_max_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(1));
    let item_a = add_labeled_item(&pane, "A", false, cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A*!"], cx);
}

#[gpui::test]
async fn test_pin_tabs_incrementally_at_max_capacity(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(3));

    let item_a = add_labeled_item(&pane, "A", false, cx);
    assert_item_labels(&pane, ["A*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A*!"], cx);

    let item_b = add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A!", "B*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*!"], cx);

    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);
}

#[gpui::test]
async fn test_pin_tabs_left_to_right_after_opening_at_max_capacity(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(3));

    let item_a = add_labeled_item(&pane, "A", false, cx);
    assert_item_labels(&pane, ["A*"], cx);

    let item_b = add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A", "B*"], cx);

    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);
}

#[gpui::test]
async fn test_pin_tabs_right_to_left_after_opening_at_max_capacity(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(3));

    let item_a = add_labeled_item(&pane, "A", false, cx);
    assert_item_labels(&pane, ["A*"], cx);

    let item_b = add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A", "B*"], cx);

    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C*!", "A", "B"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C*!", "B!", "A"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["C*!", "B!", "A!"], cx);
}

#[gpui::test]
async fn test_pinned_tabs_never_closed_at_max_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    let item_b = add_labeled_item(&pane, "B", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    add_labeled_item(&pane, "C", false, cx);
    add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C", "D", "E*"], cx);

    set_max_tabs(cx, Some(3));
    add_labeled_item(&pane, "F", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "F*"], cx);

    add_labeled_item(&pane, "G", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "G*"], cx);

    add_labeled_item(&pane, "H", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "H*"], cx);
}

#[gpui::test]
async fn test_always_allows_one_unpinned_item_over_max_tabs_regardless_of_pinned_count(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(3));

    let item_a = add_labeled_item(&pane, "A", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    let item_b = add_labeled_item(&pane, "B", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    let item_c = add_labeled_item(&pane, "C", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });

    assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);

    let item_d = add_labeled_item(&pane, "D", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C!", "D*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C!", "D*!"], cx);

    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C!", "D!", "E*"], cx);

    add_labeled_item(&pane, "F", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C!", "D!", "F*"], cx);
}

#[gpui::test]
async fn test_can_open_one_item_when_all_tabs_are_dirty_at_max(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_max_tabs(cx, Some(3));

    add_labeled_item(&pane, "A", true, cx);
    assert_item_labels(&pane, ["A*^"], cx);

    add_labeled_item(&pane, "B", true, cx);
    assert_item_labels(&pane, ["A^", "B*^"], cx);

    add_labeled_item(&pane, "C", true, cx);
    assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

    add_labeled_item(&pane, "D", false, cx);
    assert_item_labels(&pane, ["A^", "B^", "C^", "D*"], cx);

    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A^", "B^", "C^", "E*"], cx);

    add_labeled_item(&pane, "F", false, cx);
    assert_item_labels(&pane, ["A^", "B^", "C^", "F*"], cx);

    add_labeled_item(&pane, "G", true, cx);
    assert_item_labels(&pane, ["A^", "B^", "C^", "G*^"], cx);
}

#[gpui::test]
async fn test_toggle_pin_tab(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_labeled_items(&pane, ["A", "B*", "C"], cx);
    assert_item_labels(&pane, ["A", "B*", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.toggle_pin_tab(&TogglePinTab, window, cx);
    });
    assert_item_labels(&pane, ["B*!", "A", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.toggle_pin_tab(&TogglePinTab, window, cx);
    });
    assert_item_labels(&pane, ["B*", "A", "C"], cx);
}

#[gpui::test]
async fn test_unpin_all_tabs(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Unpin all, in an empty pane
    pane.update_in(cx, |pane, window, cx| {
        pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
    });

    assert_item_labels(&pane, [], cx);

    let item_a = add_labeled_item(&pane, "A", false, cx);
    let item_b = add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Unpin all, when no tabs are pinned
    pane.update_in(cx, |pane, window, cx| {
        pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
    });

    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Pin inactive tabs only
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
    });

    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // Pin all tabs
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);

        let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);

    // Activate middle tab
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, false, false, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*!", "C!"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
    });

    // Order has not changed
    assert_item_labels(&pane, ["A", "B*", "C"], cx);
}
