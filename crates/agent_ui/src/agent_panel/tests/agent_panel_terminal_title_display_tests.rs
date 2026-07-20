use super::*;

#[gpui::test]
async fn test_terminal_title_omits_placeholder_title(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "");
        assert_eq!(
            panel
                .terminals
                .get(&terminal_id)
                .unwrap()
                .title(cx)
                .as_ref(),
            ""
        );
    });

    let terminal_view = panel.read_with(&cx, |panel, _cx| {
        panel.terminals.get(&terminal_id).unwrap().view.clone()
    });
    let terminal_entity =
        terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
    terminal_entity.update(&mut cx, |_terminal, cx| {
        cx.emit(TerminalEvent::TitleChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "");
        assert_eq!(
            panel
                .terminals
                .get(&terminal_id)
                .unwrap()
                .title(cx)
                .as_ref(),
            ""
        );
    });

    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "Shell Breadcrumb".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Shell Breadcrumb");
        assert_eq!(
            panel
                .terminals
                .get(&terminal_id)
                .unwrap()
                .title(cx)
                .as_ref(),
            "Shell Breadcrumb"
        );
    });
}

#[gpui::test]
async fn test_title_edit_affordance_matches_threads_and_terminals(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        assert!(matches!(
            panel.visible_surface(),
            VisibleSurface::AgentThread(_)
        ));
        assert!(panel.should_show_title_edit(window, cx));
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        assert!(matches!(
            panel.visible_surface(),
            VisibleSurface::Terminal(_)
        ));
        assert!(panel.should_show_title_edit(window, cx));

        panel.edit_terminal_title(terminal_id, window, cx);
        assert!(!panel.should_show_title_edit(window, cx));
    });
}

#[gpui::test]
async fn test_restored_terminal_uses_metadata_title_until_shell_title_arrives(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = TerminalId::new();
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Persisted Shell Title".into(),
        custom_title: None,
        created_at: Utc::now(),
        worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
            "/project",
        )])),
        remote_connection: None,
        working_directory: None,
    };

    panel.update_in(&mut cx, |panel, window, cx| {
        panel
            .restore_test_terminal(metadata, true, AgentThreadSource::Sidebar, None, window, cx)
            .expect("test terminal should be restored");
    });
    cx.run_until_parked();

    let terminal_view = panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Persisted Shell Title");
        panel.terminals.get(&terminal_id).unwrap().view.clone()
    });

    let terminal_entity =
        terminal_view.read_with(&cx, |terminal_view, _cx| terminal_view.terminal().clone());
    terminal_entity.update(&mut cx, |terminal, cx| {
        terminal.breadcrumb_text = "Fresh Shell Title".to_string();
        cx.emit(TerminalEvent::BreadcrumbsChanged);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Fresh Shell Title");
    });
}

#[gpui::test]
async fn test_restored_terminal_selects_without_focusing(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    let terminal_id = TerminalId::new();
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Persisted Shell Title".into(),
        custom_title: None,
        created_at: Utc::now(),
        worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
            "/project",
        )])),
        remote_connection: None,
        working_directory: None,
    };

    panel.update_in(&mut cx, |panel, window, cx| {
        panel
            .restore_test_terminal(
                metadata,
                false,
                AgentThreadSource::Sidebar,
                None,
                window,
                cx,
            )
            .expect("test terminal should be restored");
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(terminal_id));
    });
}
