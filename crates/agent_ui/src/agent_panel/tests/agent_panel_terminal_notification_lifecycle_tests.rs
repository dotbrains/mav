use super::*;

#[gpui::test]
async fn test_terminal_notification_dismissed_when_active_terminal_becomes_visible(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_window, cx| {
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

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
        .expect("hidden terminal bell should show a notification");

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
        workspace.add_panel(panel.clone(), window, cx);
        workspace.focus_panel::<AgentPanel>(window, cx);
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
async fn test_terminal_notification_closed_when_panel_dropped(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_window, cx| {
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Claude", true, window, cx)
        })
        .expect("test terminal should be inserted");
    let weak_panel = panel.downgrade();
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_bell(terminal_id, cx);
    });
    cx.run_until_parked();

    cx.windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("hidden terminal bell should show a notification");

    drop(panel);
    cx.update(|_window, _cx| {});
    cx.run_until_parked();

    assert!(
        !weak_panel.is_upgradable(),
        "agent panel should be released after dropping the last handle"
    );
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
}

#[gpui::test]
async fn test_terminal_notification_view_activates_terminal_workspace(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::PrimaryScreen,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs, [Path::new("/project_b")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let workspace_a = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let first_terminal_id = panel_a
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("first test terminal should be inserted");
    let second_terminal_id = panel_a
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("second test terminal should be inserted");
    cx.run_until_parked();

    multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            assert_eq!(multi_workspace.workspace(), &workspace_b);
        })
        .unwrap();
    panel_a.read_with(cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(second_terminal_id));
    });

    panel_a.update(cx, |panel, cx| {
        panel.emit_test_terminal_bell(first_terminal_id, cx);
    });
    cx.run_until_parked();

    let notification = cx
        .windows()
        .iter()
        .find_map(|window| window.downcast::<AgentNotification>())
        .expect("terminal bell should show a notification");
    notification
        .update(cx, |notification, _window, cx| notification.accept(cx))
        .unwrap();
    assert!(
        cx.windows()
            .iter()
            .all(|window| window.downcast::<AgentNotification>().is_none())
    );
    cx.run_until_parked();

    multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            assert_eq!(multi_workspace.workspace(), &workspace_a);
        })
        .unwrap();
    panel_a.read_with(cx, |panel, cx| {
        assert_eq!(panel.active_terminal_id(), Some(first_terminal_id));
        let first_terminal = panel
            .terminals(cx)
            .into_iter()
            .find(|terminal| terminal.id == first_terminal_id)
            .expect("first terminal should remain in the panel");
        assert!(!first_terminal.has_notification);
    });
}
