use super::*;

#[gpui::test]
async fn test_thread_title_update_propagates_to_sidebar(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Hi there!".into()),
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

    // Simulate the agent generating a title. The notification chain is:
    // AcpThread::set_title emits TitleUpdated →
    // ConnectionView::handle_thread_event calls cx.notify() →
    // AgentPanel observer fires and emits AgentPanelEvent →
    // Sidebar subscription calls update_entries / rebuild_contents.
    //
    // Before the fix, handle_thread_event did NOT call cx.notify() for
    // TitleUpdated, so the AgentPanel observer never fired and the
    // sidebar kept showing the old title.
    let thread = panel.read_with(cx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    thread.update(cx, |thread, cx| {
        thread
            .set_title("Friendly Greeting with AI".into(), cx)
            .detach();
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Friendly Greeting with AI *",
        ]
    );
}

#[gpui::test]
async fn test_rename_thread_from_sidebar_updates_title_override(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Hi there!".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);

    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    let (entry_ix, thread_id, title) = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .enumerate()
            .find_map(|(ix, entry)| match entry {
                ListEntry::Thread(thread) => Some((
                    ix,
                    thread.metadata.thread_id,
                    thread.metadata.display_title(),
                )),
                ListEntry::ProjectHeader { .. } | ListEntry::Terminal(_) => None,
            })
            .expect("sidebar should have a thread entry")
    });

    let renamed_title = "abcdefghijklmnopqrstuvwxyé renamed";
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.start_renaming_thread(entry_ix, thread_id, title, window, cx);
    });
    cx.run_until_parked();
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.thread_rename_editor.update(cx, |editor, cx| {
            editor.set_text(renamed_title, window, cx);
        });
    });
    cx.run_until_parked();
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.finish_thread_rename(window, cx);
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("thread metadata should exist")
    });
    assert_eq!(metadata.title_override.as_deref(), Some(renamed_title));
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  abcdefghijklmnopqrstuvwxyé renamed *  <== selected",
        ]
    );

    let active_thread = panel.read_with(cx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    assert_eq!(
        active_thread.read_with(cx, |thread, _| thread.title()),
        Some(renamed_title.into())
    );
    let active_thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let title_editor_text =
        active_thread_view.read_with(cx, |view, cx| view.title_editor.read(cx).text(cx));
    assert_eq!(title_editor_text, renamed_title);

    active_thread.update(cx, |thread, cx| {
        thread
            .set_title("abcdefghijklmnopqrstuvwxyz0".into(), cx)
            .detach();
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  abcdefghijklmnopqrstuvwxyé renamed *  <== selected",
        ]
    );

    type_in_search(&sidebar, "0", cx);
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        Vec::<String>::new()
    );

    type_in_search(&sidebar, "é", cx);
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  abcdefghijklmnopqrstuvwxyé renamed *  <== selected",
        ]
    );
    sidebar.read_with(cx, |sidebar, _cx| {
        let thread = sidebar
            .contents
            .entries
            .iter()
            .find_map(|entry| match entry {
                ListEntry::Thread(thread) => Some(thread),
                ListEntry::ProjectHeader { .. } | ListEntry::Terminal(_) => None,
            })
            .expect("renamed thread should match the search");
        let title = thread.metadata.display_title();
        assert!(
            thread
                .highlight_positions
                .iter()
                .all(|position| { title.is_char_boundary(*position) })
        );
    });
}

#[gpui::test]
async fn test_rename_selected_thread_action_renames_selected_thread(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Hi there!".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);

    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    let (entry_ix, thread_id) = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .enumerate()
            .find_map(|(ix, entry)| match entry {
                ListEntry::Thread(thread) => Some((ix, thread.metadata.thread_id)),
                ListEntry::ProjectHeader { .. } | ListEntry::Terminal(_) => None,
            })
            .expect("sidebar should have a thread entry")
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(entry_ix);
    });
    cx.dispatch_action(RenameSelectedThread);
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_eq!(
            sidebar.renaming_thread_id,
            Some(thread_id),
            "dispatching RenameSelectedThread should start renaming the selected thread"
        );
    });

    let renamed_title = "Renamed via action";
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.thread_rename_editor.update(cx, |editor, cx| {
            editor.set_text(renamed_title, window, cx);
        });
    });
    cx.run_until_parked();
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.finish_thread_rename(window, cx);
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("thread metadata should exist")
    });
    assert_eq!(metadata.title_override.as_deref(), Some(renamed_title));
}
