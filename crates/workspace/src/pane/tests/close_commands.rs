use super::*;

async fn test_close_inactive_items(cx: &mut TestAppContext) {
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
    assert_item_labels(&pane, ["A*!"], cx);

    let item_b = add_labeled_item(&pane, "B", false, cx);
    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*!"], cx);

    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

    add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A!", "B!", "C", "D", "E*"], cx);

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
    assert_item_labels(&pane, ["A!", "B!", "E*"], cx);
}

#[gpui::test]
async fn test_running_close_inactive_items_via_an_inactive_item(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    assert_item_labels(&pane, ["A*"], cx);

    let item_b = add_labeled_item(&pane, "B", false, cx);
    assert_item_labels(&pane, ["A", "B*"], cx);

    add_labeled_item(&pane, "C", false, cx);
    add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A", "B", "C", "D", "E*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_other_items(
            &CloseOtherItems {
                save_intent: None,
                close_pinned: false,
            },
            Some(item_b.item_id()),
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["B*"], cx);
}

#[gpui::test]
async fn test_close_other_items_unpreviews_active_item(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update(cx, |pane, cx| {
        pane.set_preview_item_id(Some(item_c.item_id()), cx);
    });
    assert!(pane.read_with(cx, |pane, _| pane.preview_item_id()
        == Some(item_c.item_id())));

    pane.update_in(cx, |pane, window, cx| {
        pane.close_other_items(
            &CloseOtherItems {
                save_intent: None,
                close_pinned: false,
            },
            Some(item_c.item_id()),
            window,
            cx,
        )
    })
    .await
    .unwrap();

    assert!(pane.read_with(cx, |pane, _| pane.preview_item_id().is_none()));
    assert_item_labels(&pane, ["C*"], cx);
}

#[gpui::test]
async fn test_close_clean_items(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", true, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", true, cx);
    add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    assert_item_labels(&pane, ["A^", "B", "C^", "D", "E*"], cx);

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
    assert_item_labels(&pane, ["A^", "C*^"], cx);
}

#[gpui::test]
async fn test_close_items_to_the_left(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_labeled_items(&pane, ["A", "B", "C*", "D", "E"], cx);

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
    assert_item_labels(&pane, ["C*", "D", "E"], cx);
}

#[gpui::test]
async fn test_close_items_to_the_right(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    set_labeled_items(&pane, ["A", "B", "C*", "D", "E"], cx);

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
    assert_item_labels(&pane, ["A", "B", "C*"], cx);
}
