use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_bypass_max_tabs_limit(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window_handle = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let terminal_panel = window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                cx.new(|cx| TerminalPanel::new(workspace, window, cx))
            })
        })
        .unwrap();

    set_max_tabs(cx, Some(3));

    for _ in 0..5 {
        let task = window_handle
            .update(cx, |_, window, cx| {
                terminal_panel.update(cx, |panel, cx| {
                    panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
                })
            })
            .unwrap();
        task.await.unwrap();
    }

    cx.run_until_parked();

    let item_count =
        terminal_panel.read_with(cx, |panel, cx| panel.active_pane.read(cx).items_len());

    assert_eq!(
        item_count, 5,
        "Terminal panel should bypass max_tabs limit and have all 5 terminals"
    );
}

#[gpui::test]
async fn renders_error_if_default_shell_fails(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.terminal.get_or_insert_default().project.shell =
                    Some(settings::Shell::Program("__nonexistent_shell__".to_owned()));
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window_handle = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let terminal_panel = window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                cx.new(|cx| TerminalPanel::new(workspace, window, cx))
            })
        })
        .unwrap();

    window_handle
        .update(cx, |_, window, cx| {
            terminal_panel.update(cx, |terminal_panel, cx| {
                terminal_panel.add_terminal_shell(None, RevealStrategy::Always, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap_err();

    window_handle
        .update(cx, |_, _, cx| {
            terminal_panel.update(cx, |terminal_panel, cx| {
                assert!(
                    terminal_panel
                        .active_pane
                        .read(cx)
                        .items()
                        .any(|item| item.downcast::<FailedToSpawnTerminal>().is_some()),
                    "should spawn `FailedToSpawnTerminal` pane"
                );
            })
        })
        .unwrap();
}

#[gpui::test]
async fn test_local_terminal_in_local_project(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window_handle = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    let terminal_panel = window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                cx.new(|cx| TerminalPanel::new(workspace, window, cx))
            })
        })
        .unwrap();

    let result = window_handle
        .update(cx, |_, window, cx| {
            terminal_panel.update(cx, |terminal_panel, cx| {
                terminal_panel.add_local_terminal_shell(RevealStrategy::Always, window, cx)
            })
        })
        .unwrap()
        .await;

    assert!(
        result.is_ok(),
        "local terminal should successfully create in local project"
    );
}
#[gpui::test]
async fn test_new_terminal_opens_in_panel_by_default(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    let (window_handle, terminal_panel) = init_workspace_with_panel(cx).await;

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
        "Terminal should be added to the panel when no center terminal is focused"
    );
    assert_eq!(
        center_items_after, center_items_before,
        "Center pane should not gain a new terminal"
    );
}
