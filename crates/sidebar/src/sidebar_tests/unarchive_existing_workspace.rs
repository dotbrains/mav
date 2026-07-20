use super::*;

#[gpui::test]
async fn test_unarchive_after_removing_parent_project_group_restores_real_thread(
    cx: &mut TestAppContext,
) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_b, connection, cx);
    agent_ui::test_support::send_message(&panel_b, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel_b, cx);
    save_test_thread_metadata(&session_id, &project_b, cx).await;
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });

    cx.run_until_parked();

    let archived_metadata = cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let thread_id = store
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("archived thread should still exist in metadata store");
        let metadata = store
            .entry(thread_id)
            .cloned()
            .expect("archived metadata should still exist after archive");
        assert!(
            metadata.archived,
            "thread should be archived before project removal"
        );
        metadata
    });

    let group_key_b =
        project_b.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));
    let remove_task = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&group_key_b, window, cx)
    });
    remove_task
        .await
        .expect("remove project group task should complete");
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "removing the archived thread's parent project group should remove its workspace"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(archived_metadata.clone(), window, cx);
    });
    cx.run_until_parked();

    let restored_workspace = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|workspace| {
                PathList::new(&workspace.read(cx).root_paths(cx))
                    == PathList::new(&[PathBuf::from("/project-b")])
            })
            .cloned()
            .expect("expected unarchive to recreate the removed project workspace")
    });
    let restored_panel = restored_workspace.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("expected restored workspace to bootstrap an agent panel")
    });

    let restored_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("session should still map to restored thread id")
    });
    assert_eq!(
        restored_panel.read_with(cx, |panel, cx| panel.active_thread_id(cx)),
        Some(restored_thread_id),
        "expected unarchive after project removal to activate the restored real thread"
    );

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id,
            "expected sidebar active entry to track the restored thread after project removal",
        );
    });

    let entries = visible_entries_as_strings(&sidebar, cx);
    let restored_title = archived_metadata.display_title().to_string();
    let matching_rows: Vec<_> = entries
        .iter()
        .filter(|entry| entry.contains(&restored_title) || entry.contains("Draft"))
        .cloned()
        .collect();
    assert_eq!(
        matching_rows.len(),
        1,
        "expected only one restored row and no surviving draft after unarchive following project removal, got entries: {entries:?}"
    );
    assert!(
        !matching_rows[0].contains("Draft"),
        "expected no draft row after unarchive following project removal, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_does_not_create_duplicate_real_thread_metadata(cx: &mut TestAppContext) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/my-project", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project = project::Project::test(fs.clone(), ["/my-project".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel, connection, cx);
    agent_ui::test_support::send_message(&panel, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    let original_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("thread should exist in store before archiving")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(original_thread_id)
            .cloned()
            .expect("metadata should exist after archiving")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    let session_entries = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .filter(|entry| entry.session_id.as_ref() == Some(&session_id))
            .cloned()
            .collect::<Vec<_>>()
    });

    assert_eq!(
        session_entries.len(),
        1,
        "expected exactly one metadata row for the restored session, got: {session_entries:?}"
    );
    assert_eq!(
        session_entries[0].thread_id, original_thread_id,
        "expected unarchive to reuse the original thread id instead of creating a duplicate row"
    );
    assert!(
        session_entries[0].session_id.is_some(),
        "expected restored metadata to be a real thread, got: {:?}",
        session_entries[0]
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let real_thread_rows = entries
        .iter()
        .filter(|entry| !entry.starts_with("v ") && !entry.starts_with("> "))
        .filter(|entry| !entry.contains("Draft"))
        // Parked drafts render with the default title until the user types.
        .filter(|entry| !entry.contains(DEFAULT_THREAD_TITLE))
        .count();
    assert_eq!(
        real_thread_rows, 1,
        "expected exactly one visible real thread row after unarchive, got entries: {entries:?}"
    );
    assert!(
        !entries.iter().any(|entry| entry.contains("Draft")),
        "expected no draft rows after restoring, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_switch_to_workspace_with_archived_thread_shows_no_active_entry(
    cx: &mut TestAppContext,
) {
    // When a thread is archived while the user is in a different workspace,
    // clear_base_view creates a draft on the archived workspace's panel.
    // Switching back to that workspace shows the draft as active_entry.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Create a thread in project-a's panel (currently non-active).
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_a, connection, cx);
    agent_ui::test_support::send_message(&panel_a, cx);
    let thread_a = agent_ui::test_support::active_session_id(&panel_a, cx);
    cx.run_until_parked();

    // Archive it while project-b is active.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_a, window, cx);
    });
    cx.run_until_parked();

    // Switch back to project-a. Its panel was cleared during archiving
    // (clear_base_view activated a draft), so active_entry should point
    // to the draft on workspace_a.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.update_entries(cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _| {
        assert_active_draft(
            sidebar,
            &workspace_a,
            "after switching to workspace with archived thread, active_entry should be the draft",
        );
    });
}
