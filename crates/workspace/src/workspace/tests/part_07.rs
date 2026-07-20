use super::*;

#[gpui::test]
async fn test_no_save_prompt_when_multi_buffer_dirty_items_closed(cx: &mut TestAppContext) {
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
    let dirty_multi_buffer_with_both = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_buffer_kind(ItemBufferKind::Multibuffer)
            .with_label("Fake Project Search")
            .with_project_items(&[
                dirty_regular_buffer.read(cx).project_items[0].clone(),
                dirty_regular_buffer_2.read(cx).project_items[0].clone(),
            ])
    });
    let multi_buffer_with_both_files_id = dirty_multi_buffer_with_both.item_id();
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
            Box::new(dirty_multi_buffer_with_both.clone()),
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
            multi_buffer_with_both_files_id,
            "Should select the multi buffer in the pane"
        );
    });
    let close_all_but_multi_buffer_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_other_items(
            &CloseOtherItems {
                save_intent: Some(SaveIntent::Save),
                close_pinned: true,
            },
            None,
            window,
            cx,
        )
    });
    cx.background_executor.run_until_parked();
    assert!(!cx.has_pending_prompt());
    close_all_but_multi_buffer_task
        .await
        .expect("Closing all buffers but the multi buffer failed");
    pane.update(cx, |pane, cx| {
        assert_eq!(dirty_regular_buffer.read(cx).save_count, 1);
        assert_eq!(dirty_multi_buffer_with_both.read(cx).save_count, 0);
        assert_eq!(dirty_regular_buffer_2.read(cx).save_count, 1);
        assert_eq!(pane.items_len(), 1);
        assert_eq!(
            pane.active_item().unwrap().item_id(),
            multi_buffer_with_both_files_id,
            "Should have only the multi buffer left in the pane"
        );
        assert!(
            dirty_multi_buffer_with_both.read(cx).is_dirty,
            "The multi buffer containing the unsaved buffer should still be dirty"
        );
    });

    dirty_regular_buffer.update(cx, |buffer, cx| {
        buffer.project_items[0].update(cx, |pi, _| pi.is_dirty = true)
    });

    let close_multi_buffer_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
            window,
            cx,
        )
    });
    cx.background_executor.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "Dirty multi buffer should prompt a save dialog"
    );
    cx.simulate_prompt_answer("Save");
    cx.background_executor.run_until_parked();
    close_multi_buffer_task
        .await
        .expect("Closing the multi buffer failed");
    pane.update(cx, |pane, cx| {
        assert_eq!(
            dirty_multi_buffer_with_both.read(cx).save_count,
            1,
            "Multi buffer item should get be saved"
        );
        // Test impl does not save inner items, so we do not assert them
        assert_eq!(
            pane.items_len(),
            0,
            "No more items should be left in the pane"
        );
        assert!(pane.active_item().is_none());
    });
}

#[gpui::test]
async fn test_save_prompt_when_dirty_multi_buffer_closed_with_some_of_its_dirty_items_not_present_in_the_pane(
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

    let dirty_multi_buffer_with_both = cx.new(|cx| {
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
    let multi_buffer_with_both_files_id = dirty_multi_buffer_with_both.item_id();
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
            Box::new(dirty_multi_buffer_with_both.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, true, true, window, cx);
        assert_eq!(
            pane.active_item().unwrap().item_id(),
            multi_buffer_with_both_files_id,
            "Should select the multi buffer in the pane"
        );
    });
    let _close_multi_buffer_task = pane.update_in(cx, |pane, window, cx| {
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
        cx.has_pending_prompt(),
        "With one dirty item from the multi buffer not being in the pane, a save prompt should be shown"
    );
}

/// Tests that when `close_on_file_delete` is enabled, files are automatically
/// closed when they are deleted from disk.
#[gpui::test]
async fn test_close_on_disk_deletion_enabled(cx: &mut TestAppContext) {
    init_test(cx);

    // Enable the close_on_disk_deletion setting
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.close_on_file_delete = Some(true);
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Create a test item that simulates a file
    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_label("test.txt")
            .with_project_items(&[TestProjectItem::new(1, "test.txt", cx)])
    });

    // Add item to workspace
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item(
            pane.clone(),
            Box::new(item.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    // Verify the item is in the pane
    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items().count(), 1);
    });

    // Simulate file deletion by setting the item's deleted state
    item.update(cx, |item, _| {
        item.set_has_deleted_file(true);
    });

    // Emit UpdateTab event to trigger the close behavior
    cx.run_until_parked();
    item.update(cx, |_, cx| {
        cx.emit(ItemEvent::UpdateTab);
    });

    // Allow the close operation to complete
    cx.run_until_parked();

    // Verify the item was automatically closed
    pane.read_with(cx, |pane, _| {
        assert_eq!(
            pane.items().count(),
            0,
            "Item should be automatically closed when file is deleted"
        );
    });
}

/// Tests that when `close_on_file_delete` is disabled (default), files remain
/// open with a strikethrough when they are deleted from disk.
#[gpui::test]
async fn test_close_on_disk_deletion_disabled(cx: &mut TestAppContext) {
    init_test(cx);

    // Ensure close_on_disk_deletion is disabled (default)
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.close_on_file_delete = Some(false);
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // Create a test item that simulates a file
    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_label("test.txt")
            .with_project_items(&[TestProjectItem::new(1, "test.txt", cx)])
    });

    // Add item to workspace
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item(
            pane.clone(),
            Box::new(item.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    // Verify the item is in the pane
    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items().count(), 1);
    });

    // Simulate file deletion
    item.update(cx, |item, _| {
        item.set_has_deleted_file(true);
    });

    // Emit UpdateTab event
    cx.run_until_parked();
    item.update(cx, |_, cx| {
        cx.emit(ItemEvent::UpdateTab);
    });

    // Allow any potential close operation to complete
    cx.run_until_parked();

    // Verify the item remains open (with strikethrough)
    pane.read_with(cx, |pane, _| {
        assert_eq!(
            pane.items().count(),
            1,
            "Item should remain open when close_on_disk_deletion is disabled"
        );
    });

    // Verify the item shows as deleted
    item.read_with(cx, |item, _| {
        assert!(
            item.has_deleted_file,
            "Item should be marked as having deleted file"
        );
    });
}

/// Tests that dirty files are not automatically closed when deleted from disk,
/// even when `close_on_file_delete` is enabled. This ensures users don't lose
/// unsaved changes without being prompted.
#[gpui::test]
async fn test_close_on_disk_deletion_with_dirty_file(cx: &mut TestAppContext) {
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

    // Create a dirty test item
    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_label("test.txt")
            .with_project_items(&[TestProjectItem::new(1, "test.txt", cx)])
    });

    // Add item to workspace
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item(
            pane.clone(),
            Box::new(item.clone()),
            None,
            false,
            false,
            window,
            cx,
        );
    });

    // Simulate file deletion
    item.update(cx, |item, _| {
        item.set_has_deleted_file(true);
    });

    // Emit UpdateTab event to trigger the close behavior
    cx.run_until_parked();
    item.update(cx, |_, cx| {
        cx.emit(ItemEvent::UpdateTab);
    });

    // Allow any potential close operation to complete
    cx.run_until_parked();

    // Verify the item remains open (dirty files are not auto-closed)
    pane.read_with(cx, |pane, _| {
        assert_eq!(
            pane.items().count(),
            1,
            "Dirty items should not be automatically closed even when file is deleted"
        );
    });

    // Verify the item is marked as deleted and still dirty
    item.read_with(cx, |item, _| {
        assert!(
            item.has_deleted_file,
            "Item should be marked as having deleted file"
        );
        assert!(item.is_dirty, "Item should still be dirty");
    });
}
