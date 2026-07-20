use super::*;

#[gpui::test]
async fn test_autosave(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)]));
    let item_id = item.entity_id();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    // Autosave on window change.
    item.update(cx, |item, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnWindowChange);
            })
        });
        item.is_dirty = true;
    });

    // Deactivating the window saves the file.
    cx.deactivate_window();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 1));

    // Re-activating the window doesn't save the file.
    cx.update(|window, _| window.activate_window());
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 1));

    // Autosave on focus change.
    item.update_in(cx, |item, window, cx| {
        cx.focus_self(window);
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnFocusChange);
            })
        });
        item.is_dirty = true;
    });
    // Focus leaving the item (via window deactivation) saves the file.
    // Deferred autosaves are flushed when focus lands elsewhere (pane, panel)
    // or when the window is deactivated.
    cx.deactivate_window();
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 2));
    cx.update(|window, _| window.activate_window());

    // Deactivating the window still saves the file.
    item.update_in(cx, |item, window, cx| {
        cx.focus_self(window);
        item.is_dirty = true;
    });
    cx.deactivate_window();
    item.update(cx, |item, _| assert_eq!(item.save_count, 3));

    // Autosave after delay.
    item.update(cx, |item, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::AfterDelay {
                    milliseconds: 500.into(),
                });
            })
        });
        item.is_dirty = true;
        cx.emit(ItemEvent::Edit);
    });

    // Delay hasn't fully expired, so the file is still dirty and unsaved.
    cx.executor().advance_clock(Duration::from_millis(250));
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 3));

    // After delay expires, the file is saved.
    cx.executor().advance_clock(Duration::from_millis(250));
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 4));

    // Autosave after delay, should save earlier than delay if tab is closed
    item.update(cx, |item, cx| {
        item.is_dirty = true;
        cx.emit(ItemEvent::Edit);
    });
    cx.executor().advance_clock(Duration::from_millis(250));
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 4));

    // // Ensure auto save with delay saves the item on close, even if the timer hasn't yet run out.
    pane.update_in(cx, |pane, window, cx| {
        pane.close_items(window, cx, SaveIntent::Close, &move |id| id == item_id)
    })
    .await
    .unwrap();
    assert!(!cx.has_pending_prompt());
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 5));

    // Add the item again, ensuring autosave is prevented if the underlying file has been deleted.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });
    item.update_in(cx, |item, _window, cx| {
        item.is_dirty = true;
        for project_item in &mut item.project_items {
            project_item.update(cx, |project_item, _| project_item.is_dirty = true);
        }
    });
    cx.run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 5));

    // Autosave on focus change, ensuring closing the tab counts as such.
    item.update(cx, |item, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnFocusChange);
            })
        });
        item.is_dirty = true;
        for project_item in &mut item.project_items {
            project_item.update(cx, |project_item, _| project_item.is_dirty = true);
        }
    });

    pane.update_in(cx, |pane, window, cx| {
        pane.close_items(window, cx, SaveIntent::Close, &move |id| id == item_id)
    })
    .await
    .unwrap();
    assert!(!cx.has_pending_prompt());
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 6));

    // Add the item again, ensuring autosave is prevented if the underlying file has been deleted.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });
    item.update_in(cx, |item, window, cx| {
        item.project_items[0].update(cx, |item, _| {
            item.entry_id = None;
        });
        item.is_dirty = true;
        window.blur();
    });
    cx.run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 6));

    // Ensure autosave is prevented for deleted files also when closing the buffer.
    let _close_items = pane.update_in(cx, |pane, window, cx| {
        pane.close_items(window, cx, SaveIntent::Close, &move |id| id == item_id)
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 6));
}

#[gpui::test]
async fn test_autosave_on_focus_change_in_multibuffer(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // Create a multibuffer-like item with two child focus handles,
    // simulating individual buffer editors within a multibuffer.
    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
            .with_child_focus_handles(2, cx)
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    // Set autosave to OnFocusChange and focus the first child handle,
    // simulating the user's cursor being inside one of the multibuffer's excerpts.
    item.update_in(cx, |item, window, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnFocusChange);
            })
        });
        item.is_dirty = true;
        window.focus(&item.child_focus_handles[0], cx);
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 0));

    // Moving focus from one child to another within the same item should
    // NOT trigger autosave — focus is still within the item's focus hierarchy.
    item.update_in(cx, |item, window, cx| {
        window.focus(&item.child_focus_handles[1], cx);
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 0,
            "Switching focus between children within the same item should not autosave"
        );
    });

    // Focus leaving the item saves the file. This is the core regression scenario:
    // with `on_blur`, this would NOT trigger because `on_blur` only fires when
    // the item's own focus handle is the leaf that lost focus. In a multibuffer,
    // the leaf is always a child focus handle, so `on_blur` never detected
    // focus leaving the item.
    //
    // With deferred saves, the save happens when focus lands on a pane/panel or
    // the window deactivates.
    cx.deactivate_window();
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 1,
            "Window deactivation should trigger autosave when focus was on a child of the item"
        );
    });
    cx.update(|window, _| window.activate_window());

    // Deactivating the window should also trigger autosave when a child of
    // the multibuffer item currently owns focus.
    item.update_in(cx, |item, window, cx| {
        item.is_dirty = true;
        window.focus(&item.child_focus_handles[0], cx);
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| assert_eq!(item.save_count, 1));

    cx.deactivate_window();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 2,
            "Deactivating window should trigger autosave when focus was on a child"
        );
    });
}

#[gpui::test]
async fn test_autosave_deferred_for_modals(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let item =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)]));

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    item.update_in(cx, |item, window, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnFocusChange);
            })
        });
        item.is_dirty = true;
        cx.focus_self(window);
    });
    cx.executor().run_until_parked();

    // Opening a modal moves focus away from the item, but autosave should be
    // deferred until focus lands on a pane or panel (not saved immediately).
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, TestModal::new);
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 0,
            "Opening a modal should NOT immediately trigger autosave"
        );
    });

    // If focus returns to the same item (modal dismissed), the deferred save
    // should be skipped.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.modal_layer.update(cx, |modal, cx| {
            modal.hide_modal(window, cx);
        });
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 0,
            "Returning focus to the same item should skip deferred save"
        );
    });

    // Open modal again with a dirty item.
    item.update_in(cx, |item, window, cx| {
        item.is_dirty = true;
        cx.focus_self(window);
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, TestModal::new);
    });
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(item.save_count, 0, "Modal open should not trigger save");
    });

    // Window deactivation should flush deferred saves.
    cx.deactivate_window();
    cx.executor().run_until_parked();
    item.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 1,
            "Window deactivation should flush deferred saves"
        );
    });
}

#[gpui::test]
async fn test_autosave_deferred_until_pane_focus(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let item1 =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)]));
    let item2 =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(2, "2.txt", cx)]));

    let pane = workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item1.clone()), None, false, window, cx);
        workspace.add_item_to_active_pane(Box::new(item2.clone()), None, false, window, cx);
        workspace.active_pane().clone()
    });
    // Ensure added_to_pane is called for both items (sets up focus handlers)
    cx.executor().run_until_parked();

    // Activate item1 (at index 0) and focus it.
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(0, true, true, window, cx);
    });
    cx.executor().run_until_parked();

    // Set up OnFocusChange autosave and make item1 dirty.
    item1.update(cx, |item, cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.workspace.autosave = Some(AutosaveSetting::OnFocusChange);
            })
        });
        item.is_dirty = true;
    });
    cx.executor().run_until_parked();

    // Activate item2 via the pane - this should trigger autosave of item1.
    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, true, true, window, cx);
    });
    cx.executor().run_until_parked();

    item1.read_with(cx, |item, _| {
        assert_eq!(
            item.save_count, 1,
            "Switching to another item should trigger deferred save of the previous item"
        );
    });
}

#[gpui::test]
async fn test_pane_navigation(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let item =
        cx.new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "1.txt", cx)]));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    let toolbar = pane.read_with(cx, |pane, _| pane.toolbar().clone());
    let toolbar_notify_count = Rc::new(RefCell::new(0));

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
        let toolbar_notification_count = toolbar_notify_count.clone();
        cx.observe_in(&toolbar, window, move |_, _, _, _| {
            *toolbar_notification_count.borrow_mut() += 1
        })
        .detach();
    });

    pane.read_with(cx, |pane, _| {
        assert!(!pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    item.update_in(cx, |item, _, cx| {
        item.set_state("one".to_string(), cx);
    });

    // Toolbar must be notified to re-render the navigation buttons
    assert_eq!(*toolbar_notify_count.borrow(), 1);

    pane.read_with(cx, |pane, _| {
        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    assert_eq!(*toolbar_notify_count.borrow(), 2);
    pane.read_with(cx, |pane, _| {
        assert!(!pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });
}
