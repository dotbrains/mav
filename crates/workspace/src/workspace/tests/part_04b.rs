use super::*;

#[gpui::test]
async fn test_toggle_docks_and_panels(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);

        workspace
            .right_dock()
            .update(cx, |right_dock, cx| right_dock.set_open(true, window, cx));

        panel
    });

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        let item = cx.new(TestItem::new);
        pane.add_item(Box::new(item), true, true, None, window, cx);
    });

    // Transfer focus from center to panel
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(!panel.is_zoomed(window, cx));
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Transfer focus from panel to center
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(!panel.is_zoomed(window, cx));
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        assert!(pane.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Close the dock
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.right_dock().read(cx).is_open());
        assert!(!panel.is_zoomed(window, cx));
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        assert!(pane.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Open the dock
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(!panel.is_zoomed(window, cx));
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Focus and zoom panel
    panel.update_in(cx, |panel, window, cx| {
        cx.focus_self(window);
        panel.set_zoomed(true, window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Transfer focus to the center closes the dock
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Transferring focus back to the panel keeps it zoomed
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Close the dock while it is zoomed
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(workspace.zoomed.is_none());
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Opening the dock, when it's zoomed, retains focus
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(panel.is_zoomed(window, cx));
        assert!(workspace.zoomed.is_some());
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });

    // Unzoom and close the panel, zoom the active pane.
    panel.update_in(cx, |panel, window, cx| panel.set_zoomed(false, window, cx));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx)
    });
    pane.update_in(cx, |pane, window, cx| {
        pane.toggle_zoom(&Default::default(), window, cx)
    });

    // Opening a dock unzooms the pane.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx)
    });
    workspace.update_in(cx, |workspace, window, cx| {
        let pane = pane.read(cx);
        assert!(!pane.is_zoomed());
        assert!(!pane.focus_handle(cx).is_focused(window));
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(workspace.zoomed.is_none());
    });
}

#[gpui::test]
async fn test_close_panel_on_toggle(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| TestPanel::new(DockPosition::Right, 100, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        let item = cx.new(TestItem::new);
        pane.add_item(Box::new(item), true, true, None, window, cx);
    });

    // Enable close_panel_on_toggle
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.close_panel_on_toggle = Some(true);
        });
    });

    // Panel starts closed. Toggling should open and focus it.
    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.right_dock().read(cx).is_open());
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "Dock should be open after toggling from center"
        );
        assert!(
            panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Panel should be focused after toggling from center"
        );
    });

    // Panel is open and focused. Toggling should close the panel and
    // return focus to the center.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            !workspace.right_dock().read(cx).is_open(),
            "Dock should be closed after toggling from focused panel"
        );
        assert!(
            !panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Panel should not be focused after toggling from focused panel"
        );
    });

    // Open the dock and focus something else so the panel is open but not
    // focused. Toggling should focus the panel (not close it).
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .right_dock()
            .update(cx, |dock, cx| dock.set_open(true, window, cx));
        window.focus(&pane.read(cx).focus_handle(cx), cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(workspace.right_dock().read(cx).is_open());
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "Dock should remain open when toggling focuses an open-but-unfocused panel"
        );
        assert!(
            panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Panel should be focused after toggling an open-but-unfocused panel"
        );
    });

    // Now disable the setting and verify the original behavior: toggling
    // from a focused panel moves focus to center but leaves the dock open.
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.workspace.close_panel_on_toggle = Some(false);
        });
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel_focus::<TestPanel>(window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.right_dock().read(cx).is_open(),
            "Dock should remain open when setting is disabled"
        );
        assert!(
            !panel.read(cx).focus_handle(cx).contains_focused(window, cx),
            "Panel should not be focused after toggling with setting disabled"
        );
    });
}
