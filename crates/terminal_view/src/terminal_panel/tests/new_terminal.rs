use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_new_terminal_opens_in_center_when_center_terminal_focused(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    let (window_handle, terminal_panel) = init_workspace_with_panel(cx).await;

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::add_center_terminal(workspace, window, cx, |project, cx| {
                    project.create_terminal_shell(None, cx)
                })
            })
        })
        .expect("Failed to update workspace")
        .await
        .expect("Failed to create center terminal");
    cx.run_until_parked();

    let center_items_before = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");
    assert_eq!(center_items_before, 1, "Center pane should have 1 terminal");

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let active_item = workspace
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .expect("Center pane should have an active item");
                let terminal_view = active_item
                    .downcast::<TerminalView>()
                    .expect("Active center item should be a TerminalView");
                window.focus(&terminal_view.focus_handle(cx), cx);
            })
        })
        .expect("Failed to focus terminal view");
    cx.run_until_parked();

    let panel_items_before =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::new_terminal(
                    workspace,
                    &workspace::NewTerminal::default(),
                    window,
                    cx,
                );
            })
        })
        .expect("Failed to dispatch new_terminal");
    cx.run_until_parked();

    let center_items_after = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");
    let panel_items_after =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    assert_eq!(
        center_items_after,
        center_items_before + 1,
        "New terminal should be added to the center pane"
    );
    assert_eq!(
        panel_items_after, panel_items_before,
        "Terminal panel should not gain a new terminal"
    );
}

#[gpui::test]
async fn test_new_terminal_opens_in_panel_when_panel_focused(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    let (window_handle, terminal_panel) = init_workspace_with_panel(cx).await;

    window_handle
        .update(cx, |_, window, cx| {
            terminal_panel.update(cx, |panel, cx| {
                panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
            })
        })
        .expect("Failed to update workspace")
        .await
        .expect("Failed to create panel terminal");
    cx.run_until_parked();

    window_handle
        .update(cx, |_, window, cx| {
            window.focus(&terminal_panel.read(cx).focus_handle(cx), cx);
        })
        .expect("Failed to focus terminal panel");
    cx.run_until_parked();

    let panel_items_before =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    let center_items_before = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::new_terminal(
                    workspace,
                    &workspace::NewTerminal::default(),
                    window,
                    cx,
                );
            })
        })
        .expect("Failed to dispatch new_terminal");
    cx.run_until_parked();

    let panel_items_after =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());
    let center_items_after = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");

    assert_eq!(
        panel_items_after,
        panel_items_before + 1,
        "New terminal should be added to the panel when panel is focused"
    );
    assert_eq!(
        center_items_after, center_items_before,
        "Center pane should not gain a new terminal"
    );
}

#[gpui::test]
async fn test_new_local_terminal_opens_in_center_when_center_terminal_focused(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    init_test(cx);

    let (window_handle, terminal_panel) = init_workspace_with_panel(cx).await;

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::add_center_terminal(workspace, window, cx, |project, cx| {
                    project.create_terminal_shell(None, cx)
                })
            })
        })
        .expect("Failed to update workspace")
        .await
        .expect("Failed to create center terminal");
    cx.run_until_parked();

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let active_item = workspace
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .expect("Center pane should have an active item");
                let terminal_view = active_item
                    .downcast::<TerminalView>()
                    .expect("Active center item should be a TerminalView");
                window.focus(&terminal_view.focus_handle(cx), cx);
            })
        })
        .expect("Failed to focus terminal view");
    cx.run_until_parked();

    let center_items_before = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");
    let panel_items_before =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::new_terminal(
                    workspace,
                    &workspace::NewTerminal { local: true },
                    window,
                    cx,
                );
            })
        })
        .expect("Failed to dispatch new_terminal with local=true");
    cx.run_until_parked();

    let center_items_after = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");
    let panel_items_after =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    assert_eq!(
        center_items_after,
        center_items_before + 1,
        "New local terminal should be added to the center pane"
    );
    assert_eq!(
        panel_items_after, panel_items_before,
        "Terminal panel should not gain a new terminal"
    );
}

#[gpui::test]
async fn test_new_terminal_opens_in_panel_when_panel_focused_and_center_has_terminal(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    init_test(cx);

    let (window_handle, terminal_panel) = init_workspace_with_panel(cx).await;

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::add_center_terminal(workspace, window, cx, |project, cx| {
                    project.create_terminal_shell(None, cx)
                })
            })
        })
        .expect("Failed to update workspace")
        .await
        .expect("Failed to create center terminal");
    cx.run_until_parked();

    window_handle
        .update(cx, |_, window, cx| {
            terminal_panel.update(cx, |panel, cx| {
                panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
            })
        })
        .expect("Failed to update workspace")
        .await
        .expect("Failed to create panel terminal");
    cx.run_until_parked();

    window_handle
        .update(cx, |_, window, cx| {
            window.focus(&terminal_panel.read(cx).focus_handle(cx), cx);
        })
        .expect("Failed to focus terminal panel");
    cx.run_until_parked();

    let panel_items_before =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());
    let center_items_before = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                TerminalPanel::new_terminal(
                    workspace,
                    &workspace::NewTerminal::default(),
                    window,
                    cx,
                );
            })
        })
        .expect("Failed to dispatch new_terminal");
    cx.run_until_parked();

    let panel_items_after =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());
    let center_items_after = window_handle
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .items_len()
        })
        .expect("Failed to read center pane items");

    assert_eq!(
        panel_items_after,
        panel_items_before + 1,
        "New terminal should go to panel when panel is focused, even if center has a terminal"
    );
    assert_eq!(
        center_items_after, center_items_before,
        "Center pane should not gain a new terminal when panel is focused"
    );
}
