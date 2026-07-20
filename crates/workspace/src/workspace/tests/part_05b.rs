use super::*;

#[gpui::test]
async fn test_join_all_panes(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    add_an_item_to_active_pane(cx, &workspace, 1);
    split_pane(cx, &workspace);
    add_an_item_to_active_pane(cx, &workspace, 2);
    split_pane(cx, &workspace); // empty pane
    split_pane(cx, &workspace);
    let last_item = add_an_item_to_active_pane(cx, &workspace, 3);

    cx.executor().run_until_parked();

    workspace.update(cx, |workspace, cx| {
        let num_panes = workspace.panes().len();
        let num_items_in_current_pane = workspace.active_pane().read(cx).items().count();
        let active_item = workspace
            .active_pane()
            .read(cx)
            .active_item()
            .expect("item is in focus");

        assert_eq!(num_panes, 4);
        assert_eq!(num_items_in_current_pane, 1);
        assert_eq!(active_item.item_id(), last_item.item_id());
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.join_all_panes(window, cx);
    });

    workspace.update(cx, |workspace, cx| {
        let num_panes = workspace.panes().len();
        let num_items_in_current_pane = workspace.active_pane().read(cx).items().count();
        let active_item = workspace
            .active_pane()
            .read(cx)
            .active_item()
            .expect("item is in focus");

        assert_eq!(num_panes, 1);
        assert_eq!(num_items_in_current_pane, 3);
        assert_eq!(active_item.item_id(), last_item.item_id());
    });
}

#[gpui::test]
async fn test_flexible_dock_sizing(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new_flexible(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);
        workspace.toggle_dock(DockPosition::Right, window, cx);

        let right_dock = workspace.right_dock().clone();
        right_dock.update(cx, |dock, cx| {
            dock.set_panel_size_state(
                &panel,
                dock::PanelSizeState {
                    size: None,
                    flex: Some(1.0),
                },
                cx,
            );
        });
    });

    workspace.update_in(cx, |workspace, window, cx| {
        let item = cx.new(|cx| {
            TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "one.txt", cx)])
        });
        workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);
        workspace.bounds.size.width = px(1920.);

        let dock = workspace.right_dock().read(cx);
        let initial_width = workspace
            .dock_size(&dock, window, cx)
            .expect("flexible dock should have an initial width");

        assert_eq!(initial_width, px(960.));
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            SplitDirection::Right,
            window,
            cx,
        );

        let center_column_count = workspace.center.full_height_column_count();
        assert_eq!(center_column_count, 2);

        let dock = workspace.right_dock().read(cx);
        assert_eq!(workspace.dock_size(&dock, window, cx).unwrap(), px(640.));

        workspace.bounds.size.width = px(2400.);

        let dock = workspace.right_dock().read(cx);
        assert_eq!(workspace.dock_size(&dock, window, cx).unwrap(), px(800.));
    });
}

#[gpui::test]
async fn test_panel_size_state_persistence(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    // Fixed-width panel: pixel size is persisted to KVP and restored on re-add.
    {
        let project = Project::test(fs.clone(), [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
            workspace.bounds.size.width = px(800.);
        });

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| TestPanel::new(DockPosition::Left, 100, cx));
            workspace.add_panel(panel.clone(), window, cx);
            workspace.toggle_dock(DockPosition::Left, window, cx);
            panel
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.resize_left_dock(px(350.), window, cx);
        });

        cx.run_until_parked();

        let persisted = workspace.read_with(cx, |workspace, cx| {
            workspace.persisted_panel_size_state(TestPanel::panel_key(), cx)
        });
        assert_eq!(
            persisted.and_then(|s| s.size),
            Some(px(350.)),
            "fixed-width panel size should be persisted to KVP"
        );

        // Remove the panel and re-add a fresh instance with the same key.
        // The new instance should have its size state restored from KVP.
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.remove_panel(&panel, window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let new_panel = cx.new(|cx| TestPanel::new(DockPosition::Left, 100, cx));
            workspace.add_panel(new_panel, window, cx);

            let left_dock = workspace.left_dock().read(cx);
            let size_state = left_dock
                .panel::<TestPanel>()
                .and_then(|p| left_dock.stored_panel_size_state(&p));
            assert_eq!(
                size_state.and_then(|s| s.size),
                Some(px(350.)),
                "re-added fixed-width panel should restore persisted size from KVP"
            );
        });
    }

    // Flexible panel: both pixel size and ratio are persisted and restored.
    {
        let project = Project::test(fs.clone(), [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        workspace.update(cx, |workspace, _cx| {
            workspace.set_random_database_id();
            workspace.bounds.size.width = px(800.);
        });

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let item = cx.new(|cx| {
                TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "one.txt", cx)])
            });
            workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);

            let panel = cx.new(|cx| TestPanel::new_flexible(DockPosition::Right, 100, cx));
            workspace.add_panel(panel.clone(), window, cx);
            workspace.toggle_dock(DockPosition::Right, window, cx);
            panel
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.resize_right_dock(px(300.), window, cx);
        });

        cx.run_until_parked();

        let persisted = workspace
            .read_with(cx, |workspace, cx| {
                workspace.persisted_panel_size_state(TestPanel::panel_key(), cx)
            })
            .expect("flexible panel state should be persisted to KVP");
        assert_eq!(
            persisted.size, None,
            "flexible panel should not persist a redundant pixel size"
        );
        let original_ratio = persisted.flex.expect("panel's flex should be persisted");

        // Remove the panel and re-add: both size and ratio should be restored.
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.remove_panel(&panel, window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let new_panel = cx.new(|cx| TestPanel::new_flexible(DockPosition::Right, 100, cx));
            workspace.add_panel(new_panel, window, cx);

            let right_dock = workspace.right_dock().read(cx);
            let size_state = right_dock
                .panel::<TestPanel>()
                .and_then(|p| right_dock.stored_panel_size_state(&p))
                .expect("re-added flexible panel should have restored size state from KVP");
            assert_eq!(
                size_state.size, None,
                "re-added flexible panel should not have a persisted pixel size"
            );
            assert_eq!(
                size_state.flex,
                Some(original_ratio),
                "re-added flexible panel should restore persisted flex"
            );
        });
    }
}
