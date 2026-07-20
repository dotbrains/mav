use super::*;

#[gpui::test]
async fn test_empty_workspace_does_not_create_agent_entries(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;
    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.read_with(cx, |panel, cx| {
        assert_eq!(
            panel
                .connection_store()
                .read(cx)
                .connection_status(&Agent::NativeAgent, cx),
            crate::agent_connection_store::AgentConnectionStatus::Disconnected
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
        panel.new_external_agent_thread(
            &NewExternalAgentThread {
                agent: AgentId::new("external-agent"),
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(panel.active_conversation_view().is_none());
        assert!(panel.draft_thread.is_none());
        assert!(panel.terminals(cx).is_empty());
    });

    cx.update(|_, cx| {
        cx.update_flags(true, vec!["agent-panel-terminal".to_string()]);
    });
    panel.update_in(cx, |panel, window, cx| {
        panel.new_terminal(None, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(panel.terminals(cx).is_empty());
        assert_eq!(
            panel
                .connection_store()
                .read(cx)
                .connection_status(&Agent::NativeAgent, cx),
            crate::agent_connection_store::AgentConnectionStatus::Disconnected
        );
    });
}

#[gpui::test]
async fn test_add_selection_to_terminal_thread_pastes_mention(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({ "file.rs": "line one\nline two\nline three\n" }),
    )
    .await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let terminal_id = TerminalId::new();
    panel
        .update_in(&mut cx, |panel, window, cx| {
            panel.insert_display_only_terminal(
                terminal_id,
                Some(PathBuf::from("/project")),
                Some("Terminal".into()),
                None,
                None,
                true,
                true,
                false,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
        .expect("display-only terminal should be inserted");
    cx.run_until_parked();

    workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_paths(
                vec![PathBuf::from("/project/file.rs")],
                workspace::OpenOptions::default(),
                None,
                window,
                cx,
            )
        })
        .await;
    cx.run_until_parked();

    let editor = workspace.update(&mut cx, |workspace, cx| {
        workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
            .expect("opened file should be an editor")
    });

    cx.focus(&editor);
    cx.run_until_parked();

    let terminal = panel.read_with(&cx, |panel, cx| {
        panel
            .terminals
            .get(&terminal_id)
            .unwrap()
            .view
            .read(cx)
            .terminal()
            .clone()
    });
    terminal.update(&mut cx, |terminal, _| {
        terminal.take_input_log();
    });

    workspace.update_in(&mut cx, |_, window, cx| {
        window.dispatch_action(AddSelectionToThread.boxed_clone(), cx);
    });
    cx.run_until_parked();
    let pasted_without_selection =
        terminal.update(&mut cx, |terminal, _| terminal.take_input_log());
    assert!(pasted_without_selection.is_empty());

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([text::Point::new(1, 0)..text::Point::new(2, 4)]);
        });
    });
    cx.run_until_parked();

    workspace.update_in(&mut cx, |_, window, cx| {
        window.dispatch_action(AddSelectionToThread.boxed_clone(), cx);
    });
    cx.run_until_parked();

    let pasted: String = terminal
        .update(&mut cx, |terminal, _| terminal.take_input_log())
        .into_iter()
        .map(|bytes| String::from_utf8(bytes).expect("pasted bytes should be valid UTF-8"))
        .collect();
    assert_eq!(pasted, "file.rs:2-3 ");
}
