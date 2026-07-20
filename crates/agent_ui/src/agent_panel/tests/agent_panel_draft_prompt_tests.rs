use super::*;

#[gpui::test]
async fn test_draft_prompt_blocks_use_current_editor_snapshot(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

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

    let _stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    let thread_id = active_thread_id(&panel, cx);
    let thread = panel.read_with(cx, |panel, cx| {
        panel
            .active_agent_thread(cx)
            .expect("draft thread should be active")
    });
    let message_editor = panel.read_with(cx, |panel, cx| {
        panel
            .active_thread_view(cx)
            .expect("draft thread view should be active")
            .read(cx)
            .message_editor
            .clone()
    });

    thread.update(cx, |thread, cx| {
        thread.set_draft_prompt(
            Some(vec![acp::ContentBlock::Text(acp::TextContent::new(
                "stale prompt",
            ))]),
            cx,
        );
    });
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("fresh prompt", window, cx);
    });
    let blocks = panel.read_with(cx, |panel, cx| {
        panel
            .draft_prompt_blocks_if_in_memory(thread_id, cx)
            .expect("draft should be in memory")
    });
    assert_eq!(blocks.len(), 1);
    assert_eq!(expect_text_block(&blocks[0]), "fresh prompt");

    thread.update(cx, |thread, cx| {
        thread.set_draft_prompt(
            Some(vec![acp::ContentBlock::Text(acp::TextContent::new(
                "stale prompt after clear",
            ))]),
            cx,
        );
    });
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("", window, cx);
    });
    let blocks = panel.read_with(cx, |panel, cx| {
        panel
            .draft_prompt_blocks_if_in_memory(thread_id, cx)
            .expect("draft should be in memory")
    });
    assert!(
        blocks.is_empty(),
        "cleared editor snapshot should override stale saved draft prompt"
    );
}

#[gpui::test]
async fn test_draft_has_user_content_checks_all_live_copies(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

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
    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let _stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
    panel_a.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    let thread_id = active_thread_id(&panel_a, cx);

    panel_b.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            thread_id,
            Some(PathList::new(&[PathBuf::from("/project_b")])),
            None,
            false,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    crate::test_support::type_draft_prompt(&panel_b, "content in second panel", cx);
    let panel_a_blocks = panel_a.read_with(cx, |panel, cx| {
        panel
            .draft_prompt_blocks_if_in_memory(thread_id, cx)
            .expect("draft should be live in first panel")
    });
    assert!(
        panel_a_blocks.is_empty(),
        "first live draft copy should be empty"
    );

    let has_user_content = cx.update(|_, cx| {
        crate::draft_prompt_store::draft_has_user_content(
            thread_id,
            [&workspace_a, &workspace_b],
            cx,
        )
    });
    assert!(
        has_user_content,
        "a later live draft copy with content should keep the draft"
    );
}
