use super::*;

#[gpui::test]
async fn test_remove_draft_deletes_metadata_row(cx: &mut TestAppContext) {
    // The close-draft button deletes the metadata row and the kvp draft prompt,
    // and the draft disappears from the sidebar.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Open a draft with content, park it by pressing Cmd-N.
    let connection = StubAgentConnection::new();
    agent_ui::test_support::open_draft_with_connection(&panel, connection, cx);
    cx.run_until_parked();
    let draft_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    agent_ui::test_support::type_draft_prompt(&panel, "will be discarded", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    // The parked draft is visible.
    let draft_index = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|e| matches!(e, ListEntry::Thread(t) if t.metadata.thread_id == draft_id))
            .expect("parked draft should be visible before removal")
    });

    // Select the parked draft and dispatch the action a real user would
    // (Shift-Backspace, bound to `ArchiveSelectedThread`). The handler
    // routes to `remove_draft` for parked drafts.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = Some(draft_index);
        sidebar.archive_selected_thread(&agent_ui::ArchiveSelectedThread, window, cx);
    });
    cx.run_until_parked();

    // Metadata row and persisted draft prompt should both be gone.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert!(
            store.entry(draft_id).is_none(),
            "removed draft metadata should be deleted"
        );
        assert!(
            agent_ui::draft_prompt_store::read(draft_id, cx).is_none(),
            "removed draft's kvp prompt should also be deleted"
        );
    });
    // And the row should be gone from the sidebar.
    let still_visible = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .contents
            .entries
            .iter()
            .any(|e| matches!(e, ListEntry::Thread(t) if t.metadata.thread_id == draft_id))
    });
    assert!(
        !still_visible,
        "removed draft should no longer appear in the sidebar"
    );
}
async fn test_sending_message_from_draft_promotes_in_place(cx: &mut TestAppContext) {
    // Sending a message from a draft should keep the same ThreadId, set the
    // session_id on its metadata row, and clear the `draft_thread` pointer.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (_sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("ok".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    let draft_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

    // Before sending: draft metadata row exists with session_id = None.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store.entry(draft_id).expect("draft metadata row");
        assert!(entry.is_draft(), "expected draft row before sending");
    });

    send_message(&panel, cx);
    cx.run_until_parked();

    // After sending: draft_thread is cleared, metadata row has a session_id.
    panel.read_with(cx, |panel, cx| {
        assert!(
            !panel.active_thread_is_draft(cx),
            "should no longer be a draft after send"
        );
        assert!(
            panel.ephemeral_draft_thread_id(cx).is_none(),
            "ephemeral draft pointer should be cleared after promotion"
        );
        assert_eq!(
            panel.active_thread_id(cx),
            Some(draft_id),
            "ThreadId stays the same across promotion"
        );
    });
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store.entry(draft_id).expect("promoted metadata row");
        assert!(
            !entry.is_draft(),
            "promoted thread should have a session_id"
        );
    });
}
async fn test_cmd_n_shows_new_thread_entry(cx: &mut TestAppContext) {
    // When the user presses Cmd-N (NewThread action) while viewing a
    // non-empty thread, the panel should switch to the draft thread and
    // the sidebar should surface a "New {agent} Thread" placeholder row
    // that mirrors the active empty draft.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Create a non-empty thread (has messages).
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);

    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Hello *",
        ]
    );

    // Simulate cmd-n
    let workspace = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.focus_panel::<AgentPanel>(window, cx);
    });
    cx.run_until_parked();

    // After Cmd-N the sidebar surfaces the active empty draft as a
    // placeholder row above the real thread.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  New stub Thread", "  Hello *"],
        "After Cmd-N the sidebar should show a placeholder row for the active empty draft"
    );

    // The panel should be on the draft and active_entry should track it.
    panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_thread_is_draft(cx),
            "panel should be showing the draft after Cmd-N",
        );
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_draft(
            sidebar,
            &workspace,
            "active_entry should be Draft after Cmd-N",
        );
    });
}
