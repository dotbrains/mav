use super::*;

async fn test_item_swapping_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    assert_item_labels(&pane, [], cx);

    // Test that these actions do not panic
    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_right(&Default::default(), window, cx);
    });

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_left(&Default::default(), window, cx);
    });

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_right(&Default::default(), window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_left(&Default::default(), window, cx);
    });
    assert_item_labels(&pane, ["A", "C*", "B"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_left(&Default::default(), window, cx);
    });
    assert_item_labels(&pane, ["C*", "A", "B"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_left(&Default::default(), window, cx);
    });
    assert_item_labels(&pane, ["C*", "A", "B"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.swap_item_right(&Default::default(), window, cx);
    });
    assert_item_labels(&pane, ["A", "C*", "B"], cx);
}

#[gpui::test]
async fn test_split_empty(cx: &mut TestAppContext) {
    for split_direction in SplitDirection::all() {
        test_single_pane_split(["A"], split_direction, SplitMode::EmptyPane, cx).await;
    }
}

#[gpui::test]
async fn test_split_clone(cx: &mut TestAppContext) {
    for split_direction in SplitDirection::all() {
        test_single_pane_split(["A"], split_direction, SplitMode::ClonePane, cx).await;
    }
}

#[gpui::test]
async fn test_split_move_right_on_single_pane(cx: &mut TestAppContext) {
    test_single_pane_split(["A"], SplitDirection::Right, SplitMode::MovePane, cx).await;
}

#[gpui::test]
async fn test_split_move(cx: &mut TestAppContext) {
    for split_direction in SplitDirection::all() {
        test_single_pane_split(["A", "B"], split_direction, SplitMode::MovePane, cx).await;
    }
}

#[gpui::test]
async fn test_reopening_closed_item_after_unpreview(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update_global::<SettingsStore, ()>(|store, cx| {
        store.update_user_settings(cx, |settings| {
            settings.preview_tabs.get_or_insert_default().enabled = Some(true);
        });
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Add an item as preview
    let item = pane.update_in(cx, |pane, window, cx| {
        let item = Box::new(cx.new(|cx| TestItem::new(cx).with_label("A")));
        pane.add_item(item.clone(), true, true, None, window, cx);
        pane.set_preview_item_id(Some(item.item_id()), cx);
        item
    });

    // Verify item is preview
    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.preview_item_id(), Some(item.item_id()));
    });

    // Unpreview the item
    pane.update_in(cx, |pane, _window, _cx| {
        pane.unpreview_item_if_preview(item.item_id());
    });

    // Verify item is no longer preview
    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.preview_item_id(), None);
    });

    // Close the item
    pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(item.item_id(), SaveIntent::Skip, window, cx)
            .detach_and_log_err(cx);
    });

    cx.run_until_parked();

    // The item should be in the closed_stack and reopenable
    let has_closed_items = pane.read_with(cx, |pane, _| {
        !pane.nav_history.0.lock().closed_stack.is_empty()
    });
    assert!(
        has_closed_items,
        "closed item should be in closed_stack and reopenable"
    );
}

#[gpui::test]
async fn test_activate_item_with_wrap_around(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_next_item(&ActivateNextItem { wrap_around: false }, window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_next_item(&ActivateNextItem::default(), window, cx);
    });
    assert_item_labels(&pane, ["A*", "B", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_previous_item(&ActivatePreviousItem { wrap_around: false }, window, cx);
    });
    assert_item_labels(&pane, ["A*", "B", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_previous_item(&ActivatePreviousItem::default(), window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_previous_item(&ActivatePreviousItem { wrap_around: false }, window, cx);
    });
    assert_item_labels(&pane, ["A", "B*", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_next_item(&ActivateNextItem { wrap_around: false }, window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);
}
