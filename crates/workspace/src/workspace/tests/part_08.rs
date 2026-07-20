use super::*;

/// Tests that navigation history is cleaned up when files are auto-closed
/// due to deletion from disk.
#[gpui::test]
async fn test_close_on_disk_deletion_cleans_navigation_history(cx: &mut TestAppContext) {
    init_test(cx);

    // Enable the close_on_file_delete setting
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.close_on_file_delete = Some(true);
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Create test items
    let item1 = cx.new(|cx| {
        TestItem::new(cx)
            .with_label("test1.txt")
            .with_project_items(&[TestProjectItem::new(1, "test1.txt", cx)])
    });
    let item1_id = item1.item_id();

    let item2 = cx.new(|cx| {
        TestItem::new(cx)
            .with_label("test2.txt")
            .with_project_items(&[TestProjectItem::new(2, "test2.txt", cx)])
    });

    // Add items to workspace
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item(
            pane.clone(),
            Box::new(item1.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
        workspace.add_item(
            pane.clone(),
            Box::new(item2.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    // Activate item1 to ensure it gets navigation entries
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(0, true, true, window, cx);
    });

    // Switch to item2 and back to create navigation history
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, true, true, window, cx);
    });
    cx.run_until_parked();

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(0, true, true, window, cx);
    });
    cx.run_until_parked();

    // Simulate file deletion for item1
    item1.update(cx, |item, _| {
        item.set_has_deleted_file(true);
    });

    // Emit UpdateTab event to trigger the close behavior
    item1.update(cx, |_, cx| {
        cx.emit(ItemEvent::UpdateTab);
    });
    cx.run_until_parked();

    // Verify item1 was closed
    pane.read_with(cx, |pane, _| {
        assert_eq!(
            pane.items().count(),
            1,
            "Should have 1 item remaining after auto-close"
        );
    });

    // Check navigation history after close
    let has_item = pane.read_with(cx, |pane, cx| {
        let mut has_item = false;
        pane.nav_history().for_each_entry(cx, &mut |entry, _| {
            if entry.item.id() == item1_id {
                has_item = true;
            }
        });
        has_item
    });

    assert!(
        !has_item,
        "Navigation history should not contain closed item entries"
    );
}

#[gpui::test]
async fn test_no_save_prompt_when_dirty_multi_buffer_closed_with_all_of_its_dirty_items_present_in_the_pane(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let dirty_regular_buffer = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_label("1.txt")
            .with_project_items(&[dirty_project_item(1, "1.txt", cx)])
    });
    let dirty_regular_buffer_2 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_label("2.txt")
            .with_project_items(&[dirty_project_item(2, "2.txt", cx)])
    });
    let clear_regular_buffer = cx.new(|cx| {
        TestItem::new(cx)
            .with_label("3.txt")
            .with_project_items(&[TestProjectItem::new(3, "3.txt", cx)])
    });

    let dirty_multi_buffer = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_buffer_kind(ItemBufferKind::Multibuffer)
            .with_label("Fake Project Search")
            .with_project_items(&[
                dirty_regular_buffer.read(cx).project_items[0].clone(),
                dirty_regular_buffer_2.read(cx).project_items[0].clone(),
                clear_regular_buffer.read(cx).project_items[0].clone(),
            ])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item(
            pane.clone(),
            Box::new(dirty_regular_buffer.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
        workspace.add_item(
            pane.clone(),
            Box::new(dirty_regular_buffer_2.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
        workspace.add_item(
            pane.clone(),
            Box::new(dirty_multi_buffer.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(2, true, true, window, cx);
        assert_eq!(
            pane.active_item().unwrap().item_id(),
            dirty_multi_buffer.item_id(),
            "Should select the multi buffer in the pane"
        );
    });
    let close_multi_buffer_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    });
    cx.background_executor.run_until_parked();
    assert!(
        !cx.has_pending_prompt(),
        "All dirty items from the multi buffer are in the pane still, no save prompts should be shown"
    );
    close_multi_buffer_task
        .await
        .expect("Closing multi buffer failed");
    pane.update(cx, |pane, cx| {
        assert_eq!(dirty_regular_buffer.read(cx).save_count, 0);
        assert_eq!(dirty_multi_buffer.read(cx).save_count, 0);
        assert_eq!(dirty_regular_buffer_2.read(cx).save_count, 0);
        assert_eq!(
            pane.items()
                .map(|item| item.item_id())
                .sorted()
                .collect::<Vec<_>>(),
            vec![
                dirty_regular_buffer.item_id(),
                dirty_regular_buffer_2.item_id(),
            ],
            "Should have no multi buffer left in the pane"
        );
        assert!(dirty_regular_buffer.read(cx).is_dirty);
        assert!(dirty_regular_buffer_2.read(cx).is_dirty);
    });
}

#[gpui::test]
async fn test_active_pane_updates_to_focus_target_on_removal(cx: &mut TestAppContext) {
    assert_active_pane_is_replaced_after_removal(cx, true).await;
}

#[gpui::test]
async fn test_active_pane_updates_to_fallback_on_removal(cx: &mut TestAppContext) {
    assert_active_pane_is_replaced_after_removal(cx, false).await;
}

async fn assert_active_pane_is_replaced_after_removal(
    cx: &mut TestAppContext,
    use_focus_target: bool,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    workspace.update_in(cx, |workspace, window, cx| {
        let first_pane = workspace.active_pane().clone();
        let second_pane =
            workspace.split_pane(first_pane.clone(), SplitDirection::Right, window, cx);
        workspace.set_active_pane(&second_pane, window, cx);

        let focus_target = use_focus_target.then(|| first_pane.clone());
        workspace.remove_pane(second_pane, focus_target, window, cx);

        assert_eq!(workspace.active_pane(), &first_pane);
        assert!(
            workspace
                .panes()
                .iter()
                .any(|pane| pane == workspace.active_pane()),
            "active pane should be one of the remaining workspace panes"
        );
    });
}

#[gpui::test]
async fn test_moving_items_create_panes(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item_1 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "first.txt", cx)])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_1), None, true, window, cx);
        workspace.move_item_to_pane_in_direction(
            &MoveItemToPaneInDirection {
                direction: SplitDirection::Right,
                focus: true,
                clone: false,
            },
            window,
            cx,
        );
        workspace.move_item_to_pane_at_index(
            &MoveItemToPane {
                destination: 3,
                focus: true,
                clone: false,
            },
            window,
            cx,
        );

        assert_eq!(workspace.panes.len(), 1, "No new panes were created");
        assert_eq!(
            pane_items_paths(&workspace.active_pane, cx),
            vec!["first.txt".to_string()],
            "Single item was not moved anywhere"
        );
    });

    let item_2 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(2, "second.txt", cx)])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_2), None, true, window, cx);
        assert_eq!(
            pane_items_paths(&workspace.panes[0], cx),
            vec!["first.txt".to_string(), "second.txt".to_string()],
        );
        workspace.move_item_to_pane_in_direction(
            &MoveItemToPaneInDirection {
                direction: SplitDirection::Right,
                focus: true,
                clone: false,
            },
            window,
            cx,
        );

        assert_eq!(workspace.panes.len(), 2, "A new pane should be created");
        assert_eq!(
            pane_items_paths(&workspace.panes[0], cx),
            vec!["first.txt".to_string()],
            "After moving, one item should be left in the original pane"
        );
        assert_eq!(
            pane_items_paths(&workspace.panes[1], cx),
            vec!["second.txt".to_string()],
            "New item should have been moved to the new pane"
        );
    });

    let item_3 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(3, "third.txt", cx)])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        let original_pane = workspace.panes[0].clone();
        workspace.set_active_pane(&original_pane, window, cx);
        workspace.add_item_to_active_pane(Box::new(item_3), None, true, window, cx);
        assert_eq!(workspace.panes.len(), 2, "No new panes were created");
        assert_eq!(
            pane_items_paths(&workspace.active_pane, cx),
            vec!["first.txt".to_string(), "third.txt".to_string()],
            "New pane should be ready to move one item out"
        );

        workspace.move_item_to_pane_at_index(
            &MoveItemToPane {
                destination: 3,
                focus: true,
                clone: false,
            },
            window,
            cx,
        );
        assert_eq!(workspace.panes.len(), 3, "A new pane should be created");
        assert_eq!(
            pane_items_paths(&workspace.active_pane, cx),
            vec!["first.txt".to_string()],
            "After moving, one item should be left in the original pane"
        );
        assert_eq!(
            pane_items_paths(&workspace.panes[1], cx),
            vec!["second.txt".to_string()],
            "Previously created pane should be unchanged"
        );
        assert_eq!(
            pane_items_paths(&workspace.panes[2], cx),
            vec!["third.txt".to_string()],
            "New item should have been moved to the new pane"
        );
    });
}

#[gpui::test]
async fn test_moving_items_can_clone_panes(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item_1 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "first.txt", cx)])
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_1), None, true, window, cx);
        workspace.move_item_to_pane_in_direction(
            &MoveItemToPaneInDirection {
                direction: SplitDirection::Right,
                focus: true,
                clone: true,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.move_item_to_pane_at_index(
            &MoveItemToPane {
                destination: 3,
                focus: true,
                clone: true,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        assert_eq!(workspace.panes.len(), 3, "Two new panes were created");
        for pane in workspace.panes() {
            assert_eq!(
                pane_items_paths(pane, cx),
                vec!["first.txt".to_string()],
                "Single item exists in all panes"
            );
        }
    });

    // verify that the active pane has been updated after waiting for the
    // pane focus event to fire and resolve
    workspace.read_with(cx, |workspace, _app| {
        assert_eq!(
            workspace.active_pane(),
            &workspace.panes[2],
            "The third pane should be the active one: {:?}",
            workspace.panes
        );
    })
}
