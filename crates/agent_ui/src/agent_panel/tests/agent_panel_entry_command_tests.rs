use super::*;

#[gpui::test]
async fn test_terminal_entry_kind_controls_new_entry(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    panel.read_with(&cx, |panel, cx| {
        assert!(panel.project.read(cx).supports_terminal(cx));
        assert!(!panel.should_create_terminal_for_new_entry(cx));
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        assert_eq!(panel.active_terminal_id(), Some(terminal_id));
        assert!(panel.has_terminal(terminal_id));
        assert!(panel.should_create_terminal_for_new_entry(cx));
        let terminals = panel.terminals(cx);
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].title.as_ref(), "Dev Server");
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        assert_eq!(panel.active_terminal_id(), None);
        assert!(panel.has_terminal(terminal_id));
        assert!(!panel.should_create_terminal_for_new_entry(cx));
    });
}

#[gpui::test]
async fn test_skills_menu_entry_shows_manage_skills_shortcut(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-macos.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    open_thread_with_connection(&panel, StubAgentConnection::new(), &mut cx);
    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.focus_panel::<AgentPanel>(window, cx);
    });
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.toggle_options_menu(&ToggleOptionsMenu, window, cx);
    });
    cx.run_until_parked();

    assert!(cx.debug_bounds("MENU_ITEM-Skills").is_some());
    assert!(cx.debug_bounds("KEY_BINDING-l").is_some());
}

#[gpui::test]
async fn test_terminal_close_event_closes_without_sidebar(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_, cx| {
        TerminalThreadMetadataStore::init_global(cx);
    });

    let terminal_id = panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        panel.emit_test_terminal_close(terminal_id, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert!(!panel.has_terminal(terminal_id));
    });
    cx.update(|_, cx| {
        assert!(
            TerminalThreadMetadataStore::global(cx)
                .read(cx)
                .entry(terminal_id)
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_new_thread_dismisses_settings_overlay(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        assert!(panel.active_view_is_new_draft(cx));
        assert!(!panel.is_overlay_open());
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.set_overlay(OverlayView::Configuration, true, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, _cx| {
        assert!(panel.is_overlay_open());
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        assert!(!panel.is_overlay_open());
        assert!(panel.active_view_is_new_draft(cx));
    });
}
