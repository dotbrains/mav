use super::*;

#[gpui::test]
async fn test_terminal_working_directory_uses_active_workspace_while_workspace_is_updating(
    cx: &mut TestAppContext,
) {
    let (workspace, panel, mut cx) = setup_workspace_panel(cx).await;
    panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", false, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        assert_eq!(panel.last_created_entry_kind, AgentPanelEntryKind::Terminal);
        assert!(panel.should_create_terminal_for_new_entry(cx));
    });

    workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = workspace
            .panel::<AgentPanel>(cx)
            .expect("agent panel should be registered in workspace");
        panel.read_with(cx, |panel, cx| {
            panel.terminal_working_directory(Some(workspace), cx);
        });
        workspace.focus_panel::<AgentPanel>(window, cx);
    });

    panel.read_with(&cx, |panel, cx| {
        assert_eq!(panel.last_created_entry_kind, AgentPanelEntryKind::Terminal);
        assert!(panel.should_create_terminal_for_new_entry(cx));
    });
}

#[gpui::test]
async fn test_terminal_title_editor_is_created_only_while_editing(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        assert!(terminal.title_editor.is_none());
    });

    panel.update(&mut cx, |panel, cx| {
        panel.refresh_terminal_metadata(terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        assert!(terminal.title_editor.is_none());
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.edit_terminal_title(terminal_id, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        let title_editor = terminal
            .title_editor
            .as_ref()
            .expect("terminal title editor should be active while editing");
        assert_eq!(title_editor.read(cx).text(cx), "Dev Server");
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.stop_editing_terminal_title(terminal_id, false, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        assert!(terminal.title_editor.is_none());
    });
}

#[gpui::test]
async fn test_terminal_title_editor_does_not_set_custom_title_when_unchanged(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Initial Custom Title", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let terminal_view = panel.read_with(&cx, |panel, _cx| {
        panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel")
            .view
            .clone()
    });
    terminal_view.update(&mut cx, |terminal_view, cx| {
        terminal_view.set_custom_title(None, cx);
    });
    let terminal_entity =
        terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "Shell Breadcrumb".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Shell Breadcrumb");
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.edit_terminal_title(terminal_id, window, cx);
    });
    cx.run_until_parked();

    let title_editor = panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        let title_editor = terminal
            .title_editor
            .as_ref()
            .expect("terminal title editor should be active while editing")
            .clone();
        assert_eq!(title_editor.read(cx).text(cx), "Shell Breadcrumb");
        title_editor
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.handle_terminal_title_editor_event(
            terminal_id,
            &title_editor,
            &editor::EditorEvent::BufferEdited,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    terminal_view.read_with(&cx, |terminal_view, _cx| {
        assert!(terminal_view.custom_title().is_none());
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.stop_editing_terminal_title(terminal_id, false, window, cx);
    });
    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "Updated Shell Breadcrumb".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Updated Shell Breadcrumb");
    });
}

#[gpui::test]
async fn test_terminal_custom_title_recomposes_with_live_spinner(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Fix bug", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let terminal_entity = panel.read_with(&cx, |panel, _cx| {
        panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel")
            .view
            .clone()
    });
    let terminal_entity =
        terminal_entity.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());

    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "⠋ Thinking".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "⠋ Fix bug");
        let metadata = panel
            .terminal_metadata(terminal_id, cx)
            .expect("terminal metadata should be available");
        assert_eq!(metadata.title.as_ref(), "⠋ Thinking");
        assert_eq!(
            metadata.custom_title.as_ref().map(|title| title.as_ref()),
            Some("Fix bug")
        );
        assert_eq!(metadata.display_title().as_ref(), "⠋ Fix bug");
    });

    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "⠙ Thinking".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "⠙ Fix bug");
        let metadata = panel
            .terminal_metadata(terminal_id, cx)
            .expect("terminal metadata should be available");
        assert_eq!(metadata.title.as_ref(), "⠙ Thinking");
        assert_eq!(metadata.display_title().as_ref(), "⠙ Fix bug");
    });

    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "Thinking".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Fix bug");
        let metadata = panel
            .terminal_metadata(terminal_id, cx)
            .expect("terminal metadata should be available");
        assert_eq!(metadata.title.as_ref(), "Thinking");
        assert_eq!(metadata.display_title().as_ref(), "Fix bug");
    });
}

#[gpui::test]
async fn test_terminal_title_editor_excludes_spinner_prefix(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Initial Custom Title", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    let terminal_view = panel.read_with(&cx, |panel, _cx| {
        panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel")
            .view
            .clone()
    });
    terminal_view.update(&mut cx, |terminal_view, cx| {
        terminal_view.set_custom_title(None, cx);
    });
    let terminal_entity =
        terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "⠋ Thinking".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.edit_terminal_title(terminal_id, window, cx);
    });
    cx.run_until_parked();

    let title_editor = panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        let title_editor = terminal
            .title_editor
            .as_ref()
            .expect("terminal title editor should be active while editing")
            .clone();
        assert_eq!(title_editor.read(cx).text(cx), "Thinking");
        title_editor
    });

    title_editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text("Fix bug", window, cx);
        editor.focus_handle(cx).focus(window, cx);
    });
    panel.update_in(&mut cx, |panel, window, cx| {
        panel.handle_terminal_title_editor_event(
            terminal_id,
            &title_editor,
            &editor::EditorEvent::BufferEdited,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    terminal_view.read_with(&cx, |terminal_view, _cx| {
        assert_eq!(terminal_view.custom_title(), Some("Fix bug"));
    });
    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "⠋ Fix bug");
        let metadata = panel
            .terminal_metadata(terminal_id, cx)
            .expect("terminal metadata should be available");
        assert_eq!(metadata.title.as_ref(), "⠋ Thinking");
        assert_eq!(
            metadata.custom_title.as_ref().map(|title| title.as_ref()),
            Some("Fix bug")
        );
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.stop_editing_terminal_title(terminal_id, false, window, cx);
        panel.edit_terminal_title(terminal_id, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminal = panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should remain in the panel");
        let title_editor = terminal
            .title_editor
            .as_ref()
            .expect("terminal title editor should be active while editing");
        assert_eq!(title_editor.read(cx).text(cx), "Fix bug");
    });
}
