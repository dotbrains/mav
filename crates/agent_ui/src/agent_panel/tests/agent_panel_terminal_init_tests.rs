use super::*;

#[gpui::test]
async fn test_restored_terminal_runs_init_command_once(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_, cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.terminal_init_command = Some(" claude --resume ".to_string());
        AgentSettings::override_global(settings, cx);
    });

    let metadata = TerminalThreadMetadata {
        terminal_id: TerminalId::new(),
        title: "Restored Terminal".into(),
        custom_title: None,
        created_at: Utc::now(),
        worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
            "/project",
        )])),
        remote_connection: None,
        working_directory: None,
    };
    let terminal_id = metadata.terminal_id;
    panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.restore_test_terminal(
                metadata.clone(),
                true,
                AgentThreadSource::AgentPanel,
                None,
                window,
                cx,
            )
        })
        .expect("test terminal should be restored");
    cx.run_until_parked();

    let terminal = panel.read_with(&cx, |panel, cx| {
        panel
            .terminals
            .get(&terminal_id)
            .expect("terminal should exist")
            .view
            .read(cx)
            .terminal()
            .clone()
    });
    let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
    assert_eq!(input_log, vec![b" claude --resume \r".to_vec()]);
    assert!(
        !terminal.read_with(&cx, |terminal, _| terminal.keyboard_input_sent()),
        "writing the init command must not mark the terminal as having received \
         user keyboard input, otherwise a shell that fails to spawn would be \
         auto-closed before the user can see the error"
    );

    panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.restore_test_terminal(
                metadata,
                true,
                AgentThreadSource::AgentPanel,
                None,
                window,
                cx,
            )
        })
        .expect("restoring an existing test terminal should succeed");
    cx.run_until_parked();

    let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
    assert!(
        input_log.is_empty(),
        "activating an already-restored terminal should not re-run the init command, got {input_log:?}"
    );
}

#[cfg(unix)]
#[gpui::test]
async fn test_spawn_terminal_runs_init_command_in_real_shell(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.executor().allow_parking();
    cx.update(|_, cx| {
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.terminal_init_command = Some("printf 'init_ran_%s\\n' 42".to_string());
        AgentSettings::override_global(settings, cx);

        let mut terminal_settings =
            terminal::terminal_settings::TerminalSettings::get_global(cx).clone();
        terminal_settings.shell = task::Shell::Program("/bin/sh".to_string());
        terminal::terminal_settings::TerminalSettings::override_global(terminal_settings, cx);
    });

    let terminal_id = TerminalId::new();
    panel.update_in(&mut cx, |panel, window, cx| {
        panel.spawn_terminal(
            terminal_id,
            None,
            None,
            None,
            None,
            true,
            true,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let terminal = loop {
        cx.run_until_parked();
        let terminal = panel.read_with(&cx, |panel, cx| {
            panel
                .terminals
                .get(&terminal_id)
                .map(|terminal| terminal.view.read(cx).terminal().clone())
        });
        if let Some(terminal) = &terminal
            && terminal
                .read_with(&cx, |terminal, _| terminal.get_content())
                .contains("init_ran_42")
        {
            break terminal.clone();
        }
        if Instant::now() >= deadline {
            let terminal_created = terminal.is_some();
            let (content, input_log) = if let Some(terminal) = terminal {
                let content = terminal.read_with(&cx, |terminal, _| terminal.get_content());
                let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
                (content, input_log)
            } else {
                (String::new(), Vec::new())
            };
            panic!(
                "init command output never appeared in the terminal; terminal_created={terminal_created}, content={content:?}, input_log={input_log:?}"
            );
        }
        cx.executor().timer(Duration::from_millis(50)).await;
    };

    let input_log = terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
    assert_eq!(
        input_log,
        vec![b"printf 'init_ran_%s\\n' 42\r".to_vec()],
        "init command should be written only after terminal startup has settled"
    );
    assert!(
        !terminal.read_with(&cx, |terminal, _| terminal.keyboard_input_sent()),
        "writing the init command must not mark the terminal as having received \
         user keyboard input"
    );
}

#[gpui::test]
async fn test_restored_terminal_does_not_update_global_entry_kind(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.update(|_, cx| {
        TerminalThreadMetadataStore::init_global(cx);
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
            Some(AgentPanelEntryKind::Thread)
        );
    });

    let metadata = TerminalThreadMetadata {
        terminal_id: TerminalId::new(),
        title: "Restored Terminal".into(),
        custom_title: None,
        created_at: Utc::now(),
        worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
            "/project",
        )])),
        remote_connection: None,
        working_directory: None,
    };
    panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.restore_test_terminal(
                metadata,
                true,
                AgentThreadSource::AgentPanel,
                None,
                window,
                cx,
            )
        })
        .expect("test terminal should be restored");
    cx.run_until_parked();

    cx.update(|_, cx| {
        assert_eq!(
            read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
            Some(AgentPanelEntryKind::Thread),
            "restoring a terminal should not change the global new-entry default"
        );
    });
}

#[gpui::test]
async fn test_new_workspace_load_uses_global_terminal_entry_kind(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        TerminalThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project-b", json!({ "file.txt": "" }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = Project::test(fs.clone(), [Path::new("/project-a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project-b")], cx).await;
    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let multi_workspace_entity = multi_workspace.root(cx).unwrap();
    let workspace_a = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    workspace_a.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });
    panel_a
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    cx.update(|_window, cx| {
        assert_eq!(
            read_global_last_created_entry_kind(&KeyValueStore::global(cx)),
            Some(AgentPanelEntryKind::Terminal)
        );
    });

    let workspace_b = multi_workspace_entity.update_in(cx, |multi_workspace, window, cx| {
        multi_workspace.test_add_workspace(project_b.clone(), window, cx)
    });
    workspace_b.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded = AgentPanel::load(workspace_b.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    workspace_b.update_in(cx, |workspace, window, cx| {
        workspace.add_panel(loaded.clone(), window, cx);
    });
    loaded.update_in(cx, |panel, window, cx| {
        panel.set_active(true, window, cx);
    });
    for _ in 0..8 {
        cx.run_until_parked();
    }

    loaded.read_with(cx, |panel, cx| {
        assert!(
            panel.active_terminal_id().is_some(),
            "new workspace should initialize to a terminal when terminal was the globally last used entry kind"
        );
        assert!(
            panel.active_conversation_view().is_none(),
            "new workspace should not initialize to a draft when terminal is the global entry kind"
        );
        assert!(panel.should_create_terminal_for_new_entry(cx));
    });
}
