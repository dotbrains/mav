use super::*;

async fn test_new_tab_scrolls_into_view_completely(cx: &mut TestAppContext) {
    // Arrange
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    cx.simulate_resize(size(px(300.), px(300.)));

    add_labeled_item(&pane, "untitled", false, cx);
    add_labeled_item(&pane, "untitled", false, cx);
    add_labeled_item(&pane, "untitled", false, cx);
    add_labeled_item(&pane, "untitled", false, cx);
    // Act: this should trigger a scroll
    add_labeled_item(&pane, "untitled", false, cx);
    // Assert
    let tab_bar_scroll_handle =
        pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
    assert_eq!(tab_bar_scroll_handle.children_count(), 6);
    let tab_bounds = cx.debug_bounds("TAB-4").unwrap();
    let new_tab_button_bounds = cx.debug_bounds("ICON-Plus").unwrap();
    let scroll_bounds = tab_bar_scroll_handle.bounds();
    let scroll_offset = tab_bar_scroll_handle.offset();
    assert!(scroll_offset.x < px(0.));
    assert!(scroll_offset.x >= -tab_bar_scroll_handle.max_offset().x);
    assert!(tab_bounds.left() >= scroll_bounds.left());
    assert!(tab_bounds.right() <= scroll_bounds.right());
    assert!(
        !tab_bounds.intersects(&new_tab_button_bounds),
        "Tab should not overlap with the new tab button, if this is failing check if there's been a redesign!"
    );
}

#[gpui::test]
async fn test_pinned_tabs_scroll_to_item_uses_correct_index(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    cx.simulate_resize(size(px(400.), px(300.)));

    for label in ["A", "B", "C"] {
        add_labeled_item(&pane, label, false, cx);
    }

    pane.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(0, window, cx);
        pane.pin_tab_at(1, window, cx);
        pane.pin_tab_at(2, window, cx);
    });

    for label in ["D", "E", "F", "G", "H", "I", "J", "K"] {
        add_labeled_item(&pane, label, false, cx);
    }

    assert_item_labels(
        &pane,
        ["A!", "B!", "C!", "D", "E", "F", "G", "H", "I", "J", "K*"],
        cx,
    );

    cx.run_until_parked();

    // Verify overflow exists (precondition for scroll test)
    let scroll_handle = pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
    assert!(
        scroll_handle.max_offset().x > px(0.),
        "Test requires tab overflow to verify scrolling. Increase tab count or reduce window width."
    );

    // Activate a different tab first, then activate K
    // This ensures we're not just re-activating an already-active tab
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(3, true, true, window, cx);
    });
    cx.run_until_parked();

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(10, true, true, window, cx);
    });
    cx.run_until_parked();

    let scroll_handle = pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
    let k_tab_bounds = cx.debug_bounds("TAB-10").unwrap();
    let scroll_bounds = scroll_handle.bounds();

    assert!(
        k_tab_bounds.left() >= scroll_bounds.left(),
        "Active tab K should be scrolled into view"
    );
}

#[gpui::test]
async fn test_close_all_items_including_pinned(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: true,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, [], cx);
}

#[gpui::test]
async fn test_close_pinned_tab_with_non_pinned_in_same_pane(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // Non-pinned tabs in same pane
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(0, window, cx);
    });
    set_labeled_items(&pane, ["A*", "B", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
        .unwrap();
    });
    // Non-pinned tab should be active
    assert_item_labels(&pane, ["A!", "B*", "C"], cx);
}

#[gpui::test]
async fn test_close_pinned_tab_with_non_pinned_in_different_pane(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // No non-pinned tabs in same pane, non-pinned tabs in another pane
    let pane1 = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    let pane2 = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane1.clone(), SplitDirection::Right, window, cx)
    });
    add_labeled_item(&pane1, "A", false, cx);
    pane1.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(0, window, cx);
    });
    set_labeled_items(&pane1, ["A*"], cx);
    add_labeled_item(&pane2, "B", false, cx);
    set_labeled_items(&pane2, ["B"], cx);
    pane1.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
        .unwrap();
    });
    //  Non-pinned tab of other pane should be active
    assert_item_labels(&pane2, ["B*"], cx);
}

#[gpui::test]
async fn ensure_item_closing_actions_do_not_panic_when_no_items_exist(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    assert_item_labels(&pane, [], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update_in(cx, |pane, window, cx| {
        pane.close_other_items(
            &CloseOtherItems {
                save_intent: None,
                close_pinned: false,
            },
            None,
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update_in(cx, |pane, window, cx| {
        pane.close_clean_items(
            &CloseCleanItems {
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update_in(cx, |pane, window, cx| {
        pane.close_items_to_the_right_by_id(
            None,
            &CloseItemsToTheRight {
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update_in(cx, |pane, window, cx| {
        pane.close_items_to_the_left_by_id(
            None,
            &CloseItemsToTheLeft {
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
}
