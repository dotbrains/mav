use super::*;

#[gpui::test]
async fn test_pane_zoom_in_out(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let pane = workspace.update_in(cx, |workspace, _window, _cx| {
        workspace.active_pane().clone()
    });

    // Add an item to the pane so it can be zoomed
    workspace.update_in(cx, |workspace, window, cx| {
        let item = cx.new(TestItem::new);
        workspace.add_item(pane.clone(), Box::new(item), None, true, true, window, cx);
    });

    // Initially not zoomed
    workspace.update_in(cx, |workspace, _window, cx| {
        assert!(!pane.read(cx).is_zoomed(), "Pane starts unzoomed");
        assert!(
            workspace.zoomed.is_none(),
            "Workspace should track no zoomed pane"
        );
        assert!(pane.read(cx).items_len() > 0, "Pane should have items");
    });

    // Zoom In
    pane.update_in(cx, |pane, window, cx| {
        pane.zoom_in(&crate::ZoomIn, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            pane.read(cx).is_zoomed(),
            "Pane should be zoomed after ZoomIn"
        );
        assert!(
            workspace.zoomed.is_some(),
            "Workspace should track the zoomed pane"
        );
        assert!(
            pane.read(cx).focus_handle(cx).contains_focused(window, cx),
            "ZoomIn should focus the pane"
        );
    });

    // Zoom In again is a no-op
    pane.update_in(cx, |pane, window, cx| {
        pane.zoom_in(&crate::ZoomIn, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(pane.read(cx).is_zoomed(), "Second ZoomIn keeps pane zoomed");
        assert!(
            workspace.zoomed.is_some(),
            "Workspace still tracks zoomed pane"
        );
        assert!(
            pane.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Pane remains focused after repeated ZoomIn"
        );
    });

    // Zoom Out
    pane.update_in(cx, |pane, window, cx| {
        pane.zoom_out(&crate::ZoomOut, window, cx);
    });

    workspace.update_in(cx, |workspace, _window, cx| {
        assert!(
            !pane.read(cx).is_zoomed(),
            "Pane should unzoom after ZoomOut"
        );
        assert!(
            workspace.zoomed.is_none(),
            "Workspace clears zoom tracking after ZoomOut"
        );
    });

    // Zoom Out again is a no-op
    pane.update_in(cx, |pane, window, cx| {
        pane.zoom_out(&crate::ZoomOut, window, cx);
    });

    workspace.update_in(cx, |workspace, _window, cx| {
        assert!(
            !pane.read(cx).is_zoomed(),
            "Second ZoomOut keeps pane unzoomed"
        );
        assert!(
            workspace.zoomed.is_none(),
            "Workspace remains without zoomed pane"
        );
    });
}

#[gpui::test]
async fn test_join_pane_into_next(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // Let's arrange the panes like this:
    //
    // +-----------------------+
    // |         top           |
    // +------+--------+-------+
    // | left | center | right |
    // +------+--------+-------+
    // |        bottom         |
    // +-----------------------+

    let top_item = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "top.txt", cx)]));
    let bottom_item = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(2, "bottom.txt", cx)])
    });
    let left_item = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(3, "left.txt", cx)]));
    let right_item = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(4, "right.txt", cx)])
    });
    let center_item = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(5, "center.txt", cx)])
    });

    let top_pane_id = workspace.update_in(cx, |workspace, window, cx| {
        let top_pane_id = workspace.active_pane().entity_id();
        workspace.add_item_to_active_pane(Box::new(top_item.clone()), None, false, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Down,
            window,
            cx,
        );
        top_pane_id
    });
    let bottom_pane_id = workspace.update_in(cx, |workspace, window, cx| {
        let bottom_pane_id = workspace.active_pane().entity_id();
        workspace.add_item_to_active_pane(Box::new(bottom_item.clone()), None, false, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Up,
            window,
            cx,
        );
        bottom_pane_id
    });
    let left_pane_id = workspace.update_in(cx, |workspace, window, cx| {
        let left_pane_id = workspace.active_pane().entity_id();
        workspace.add_item_to_active_pane(Box::new(left_item.clone()), None, false, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );
        left_pane_id
    });
    let right_pane_id = workspace.update_in(cx, |workspace, window, cx| {
        let right_pane_id = workspace.active_pane().entity_id();
        workspace.add_item_to_active_pane(Box::new(right_item.clone()), None, false, window, cx);
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Left,
            window,
            cx,
        );
        right_pane_id
    });
    let center_pane_id = workspace.update_in(cx, |workspace, window, cx| {
        let center_pane_id = workspace.active_pane().entity_id();
        workspace.add_item_to_active_pane(Box::new(center_item.clone()), None, false, window, cx);
        center_pane_id
    });
    cx.executor().run_until_parked();

    workspace.update_in(cx, |workspace, window, cx| {
        assert_eq!(center_pane_id, workspace.active_pane().entity_id());

        // Join into next from center pane into right
        workspace.join_pane_into_next(workspace.active_pane().clone(), window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let active_pane = workspace.active_pane();
        assert_eq!(right_pane_id, active_pane.entity_id());
        assert_eq!(2, active_pane.read(cx).items_len());
        let item_ids_in_pane =
            HashSet::from_iter(active_pane.read(cx).items().map(|item| item.item_id()));
        assert!(item_ids_in_pane.contains(&center_item.item_id()));
        assert!(item_ids_in_pane.contains(&right_item.item_id()));

        // Join into next from right pane into bottom
        workspace.join_pane_into_next(workspace.active_pane().clone(), window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let active_pane = workspace.active_pane();
        assert_eq!(bottom_pane_id, active_pane.entity_id());
        assert_eq!(3, active_pane.read(cx).items_len());
        let item_ids_in_pane =
            HashSet::from_iter(active_pane.read(cx).items().map(|item| item.item_id()));
        assert!(item_ids_in_pane.contains(&center_item.item_id()));
        assert!(item_ids_in_pane.contains(&right_item.item_id()));
        assert!(item_ids_in_pane.contains(&bottom_item.item_id()));

        // Join into next from bottom pane into left
        workspace.join_pane_into_next(workspace.active_pane().clone(), window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let active_pane = workspace.active_pane();
        assert_eq!(left_pane_id, active_pane.entity_id());
        assert_eq!(4, active_pane.read(cx).items_len());
        let item_ids_in_pane =
            HashSet::from_iter(active_pane.read(cx).items().map(|item| item.item_id()));
        assert!(item_ids_in_pane.contains(&center_item.item_id()));
        assert!(item_ids_in_pane.contains(&right_item.item_id()));
        assert!(item_ids_in_pane.contains(&bottom_item.item_id()));
        assert!(item_ids_in_pane.contains(&left_item.item_id()));

        // Join into next from left pane into top
        workspace.join_pane_into_next(workspace.active_pane().clone(), window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let active_pane = workspace.active_pane();
        assert_eq!(top_pane_id, active_pane.entity_id());
        assert_eq!(5, active_pane.read(cx).items_len());
        let item_ids_in_pane =
            HashSet::from_iter(active_pane.read(cx).items().map(|item| item.item_id()));
        assert!(item_ids_in_pane.contains(&center_item.item_id()));
        assert!(item_ids_in_pane.contains(&right_item.item_id()));
        assert!(item_ids_in_pane.contains(&bottom_item.item_id()));
        assert!(item_ids_in_pane.contains(&left_item.item_id()));
        assert!(item_ids_in_pane.contains(&top_item.item_id()));

        // Single pane left: no-op
        workspace.join_pane_into_next(workspace.active_pane().clone(), window, cx)
    });

    workspace.update(cx, |workspace, _cx| {
        let active_pane = workspace.active_pane();
        assert_eq!(top_pane_id, active_pane.entity_id());
    });
}

fn add_an_item_to_active_pane(
    cx: &mut VisualTestContext,
    workspace: &Entity<Workspace>,
    item_id: u64,
) -> Entity<TestItem> {
    let item = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(
            item_id,
            "item{item_id}.txt",
            cx,
        )])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, false, window, cx);
    });
    item
}

fn split_pane(cx: &mut VisualTestContext, workspace: &Entity<Workspace>) -> Entity<Pane> {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        )
    })
}
