use super::*;

#[gpui::test]
async fn test_active_terminal_serialize_and_load_round_trip(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        TerminalThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
    });
    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let workspace_id = workspace
        .read_with(cx, |workspace, _cx| workspace.database_id())
        .expect("workspace should have a database id");
    let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
    let serialized: SerializedAgentPanel = cx
        .background_spawn(async move { read_serialized_panel(workspace_id, &kvp) })
        .await
        .expect("workspace should serialize panel state");
    assert_eq!(
        serialized.last_active_terminal_id,
        Some(terminal_id.to_key_string())
    );
    assert!(
        serialized.last_active_thread.is_none(),
        "active terminal serialization should not also include a thread restore target"
    );

    cx.update(|_window, cx| {
        TerminalThreadMetadataStore::init_global(cx);
    });
    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    for _ in 0..8 {
        cx.run_until_parked();
    }

    loaded.read_with(cx, |panel, cx| {
        assert_eq!(panel.active_terminal_id(), Some(terminal_id));
        assert!(
            panel.active_conversation_view().is_none(),
            "the restored terminal should remain active instead of falling back to a draft"
        );
        assert!(
            panel
                .terminals(cx)
                .into_iter()
                .any(|terminal| terminal.id == terminal_id),
            "active terminal metadata should be restored into the loaded panel"
        );
    });
}

#[gpui::test]
async fn test_terminal_restore_working_directory_does_not_read_leased_workspace(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);

        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .terminal
                    .get_or_insert_default()
                    .project
                    .working_directory = Some(WorkingDirectory::AlwaysHome);
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    project.update(cx, |project, _cx| {
        project.mark_as_collab_for_testing();
    });
    project.read_with(cx, |project, _cx| {
        assert!(project.is_remote());
    });

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .expect("multi workspace should have an active workspace");
    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            terminal_view::default_working_directory(workspace, cx)
        }),
        None
    );

    let metadata = TerminalThreadMetadata {
        terminal_id: TerminalId::new(),
        title: "Dev Server".into(),
        custom_title: None,
        created_at: Utc::now(),
        worktree_paths: project.read_with(cx, |project, cx| project.worktree_paths(cx)),
        remote_connection: None,
        working_directory: None,
    };
    assert_eq!(metadata.working_directory, None);

    let working_directory = workspace.update_in(cx, |workspace, _window, cx| {
        panel
            .read(cx)
            .terminal_restore_working_directory(&metadata, Some(workspace), cx)
    });

    assert_eq!(working_directory, None);
}

#[gpui::test]
async fn test_pending_terminal_restore_prevents_initial_terminal_creation(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.last_created_entry_kind = AgentPanelEntryKind::Terminal;
        panel.pending_terminal_spawn = Some(TerminalId::new());
        panel.set_active(true, window, cx);
    });
    for _ in 0..4 {
        cx.run_until_parked();
    }

    panel.read_with(&cx, |panel, cx| {
        assert!(
            panel.terminals(cx).is_empty(),
            "activation while a terminal restore is pending should not create a second terminal"
        );
        assert!(
            panel.active_conversation_view().is_none(),
            "activation while a terminal restore is pending should not fall back to a draft"
        );
    });
}

#[gpui::test]
async fn test_repeated_activation_only_creates_one_initial_terminal(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.last_created_entry_kind = AgentPanelEntryKind::Terminal;
        panel.set_active(true, window, cx);
        panel.set_active(true, window, cx);
    });
    for _ in 0..8 {
        cx.run_until_parked();
    }

    panel.read_with(&cx, |panel, cx| {
        assert_eq!(
            panel.terminals(cx).len(),
            1,
            "repeated activation should only enqueue one initial terminal"
        );
        assert!(
            panel.active_terminal_id().is_some(),
            "the single initial terminal should become active"
        );
    });
}
