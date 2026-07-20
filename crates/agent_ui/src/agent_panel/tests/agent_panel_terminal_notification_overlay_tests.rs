use super::*;

#[gpui::test]
async fn test_terminal_bell_marks_and_activation_clears_notification(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
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

    panel.read_with(&cx, |panel, cx| {
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(first_terminal.has_notification);
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.activate_terminal(first_terminal_id, true, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(!first_terminal.has_notification);
    });
}

#[gpui::test]
async fn test_visible_terminal_bell_is_suppressed(cx: &mut TestAppContext) {
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
        assert!(!terminal.has_notification);
    });
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}

#[gpui::test]
async fn test_visible_terminal_bell_is_suppressed_without_focus(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let workspace = cx.update(|window, cx| {
        window
            .root::<MultiWorkspace>()
            .flatten()
            .expect("test window should have a MultiWorkspace root")
            .read(cx)
            .workspace()
            .clone()
    });
    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.focus_handle(cx).focus(window, cx);
    });
    cx.update(|window, cx| {
        assert!(window.is_window_active());
        assert!(workspace.read(cx).focus_handle(cx).is_focused(window));
        assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
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
        assert!(!terminal.has_notification);
    });
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}

#[gpui::test]
async fn test_terminal_bell_notifies_when_configuration_overlay_covers_terminal(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.set_overlay(OverlayView::Configuration, true, window, cx);
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
        .expect("covered terminal bell should show a notification");
}

#[gpui::test]
async fn test_thread_notification_shows_when_configuration_overlay_covers_thread(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_visible_panel(cx).await;
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Default response".into()),
    )]);
    open_thread_with_connection(&panel, connection, &mut cx);

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.set_overlay(OverlayView::Configuration, true, window, cx);
    });
    send_message(&panel, &mut cx);

    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("covered thread should show a notification");
}
