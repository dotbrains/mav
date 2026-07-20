use super::*;

#[gpui::test]
async fn test_draft_promotion_creates_metadata_and_new_session_on_reload(cx: &mut TestAppContext) {
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
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
    stub_connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);
    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_thread_is_draft(cx),
            "thread should be a draft before any message is sent"
        );
        assert!(
            panel.draft_thread.is_some(),
            "draft_thread field should be set"
        );
    });
    let draft_session_id = active_session_id(&panel, cx);
    let thread_id = active_thread_id(&panel, cx);

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store
            .entry(thread_id)
            .expect("draft thread should have a metadata row");
        assert!(
            entry.is_draft(),
            "draft thread metadata should have session_id=None, got {:?}",
            entry.session_id,
        );
    });

    crate::test_support::type_draft_prompt(&panel, "Hello from draft", cx);
    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let reloaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load with draft should succeed");
    cx.run_until_parked();

    reloaded_panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_thread_is_draft(cx),
            "reloaded panel should still show the draft as active"
        );
        assert!(
            panel.active_view_is_new_draft(cx),
            "reloaded draft should still occupy the new-draft slot"
        );
        let active_entity = panel.active_conversation_view().map(|v| v.entity_id());
        let draft_entity = panel.draft_thread.as_ref().map(|v| v.entity_id());
        assert!(
            active_entity.is_some() && active_entity == draft_entity,
            "active view and draft slot should share a single ConversationView entity \
             (active={active_entity:?}, draft={draft_entity:?})"
        );
    });

    let reloaded_thread_id = active_thread_id(&reloaded_panel, cx);
    assert_eq!(
        reloaded_thread_id, thread_id,
        "reloaded draft should preserve its ThreadId"
    );

    let reloaded_session_id = active_session_id(&reloaded_panel, cx);
    assert_ne!(
        reloaded_session_id, draft_session_id,
        "reloaded draft should have a fresh ACP session ID"
    );

    let restored_text =
        reloaded_panel.read_with(cx, |panel, cx| panel.editor_text(reloaded_thread_id, cx));
    assert_eq!(
        restored_text.as_deref(),
        Some("Hello from draft"),
        "draft prompt text should be restored from the draft-prompt kvp store"
    );

    let panel = reloaded_panel;
    let promoted_session_id = reloaded_session_id;
    send_message(&panel, cx);

    panel.read_with(cx, |panel, cx| {
        assert!(
            !panel.active_thread_is_draft(cx),
            "thread should no longer be a draft after sending a message"
        );
        assert!(
            panel.draft_thread.is_none(),
            "draft_thread should be None after promotion"
        );
        assert_eq!(
            panel.active_thread_id(cx),
            Some(thread_id),
            "same ThreadId should remain active after promotion"
        );
    });

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let metadata = store
            .entry(thread_id)
            .expect("promoted thread should have metadata");
        assert!(
            !metadata.is_draft(),
            "promoted thread metadata should no longer be a draft"
        );
        assert_eq!(
            metadata.session_id.as_ref(),
            Some(&promoted_session_id),
            "metadata session_id should match the thread's ACP session"
        );
    });

    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    cx.run_until_parked();

    loaded_panel.read_with(cx, |panel, cx| {
        let active_id = panel.active_thread_id(cx);
        assert_eq!(
            active_id,
            Some(thread_id),
            "loaded panel should restore the promoted thread"
        );
        assert!(
            !panel.active_thread_is_draft(cx),
            "restored thread should not be a draft"
        );
    });
}
