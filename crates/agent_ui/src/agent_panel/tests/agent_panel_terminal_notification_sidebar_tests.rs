use super::*;

#[gpui::test]
async fn test_terminal_bell_marks_without_popup_when_sidebar_open(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let first_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("first test terminal should be inserted");
    let second_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("second test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
    });
    cx.update(|window, cx| {
        let multi_workspace = window
            .root::<MultiWorkspace>()
            .flatten()
            .expect("test window should have a MultiWorkspace root");
        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.open_sidebar(cx);
        });
    });
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(first_terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(first_terminal.has_notification);
    });
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}

#[gpui::test]
async fn test_terminal_bell_notifies_when_sidebar_history_open(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel_with_sidebar(cx, false).await;
    let first_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("first test terminal should be inserted");
    let second_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("second test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
    });
    cx.update(|window, cx| {
        let multi_workspace = window
            .root::<MultiWorkspace>()
            .flatten()
            .expect("test window should have a MultiWorkspace root");
        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.open_sidebar(cx);
        });
    });
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(first_terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(first_terminal.has_notification);
    });
    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("terminal bell should notify when the sidebar thread list is hidden");
}

#[gpui::test]
async fn test_terminal_notification_dismissed_when_sidebar_opens(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let first_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("first test terminal should be inserted");
    let second_terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("second test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
    });
    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(first_terminal_id, cx);
    });
    cx.run_until_parked();

    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("inactive terminal bell should show a notification");

    cx.update(|window, cx| {
        let multi_workspace = window
            .root::<MultiWorkspace>()
            .flatten()
            .expect("test window should have a MultiWorkspace root");
        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.open_sidebar(cx);
        });
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(first_terminal.has_notification);
    });
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}

#[gpui::test]
async fn test_focused_terminal_bell_notifies_when_window_inactive(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    cx.update(|window, cx| {
        assert!(window.is_window_active());
        assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
    });
    cx.deactivate_window();
    cx.update(|window, _cx| {
        assert!(!window.is_window_active());
    });

    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == terminal_id)
            .expect("terminal should remain in the panel");
        assert!(terminal.has_notification);
    });
    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("background terminal bell should show a notification");
}

#[gpui::test]
async fn test_active_terminal_notification_clears_when_window_reactivates(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    cx.deactivate_window();
    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == terminal_id)
            .expect("terminal should remain in the panel");
        assert!(terminal.has_notification);
    });
    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("background terminal bell should show a notification");

    cx.update(|window, _cx| {
        window.activate_window();
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == terminal_id)
            .expect("terminal should remain in the panel");
        assert!(!terminal.has_notification);
    });
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}
