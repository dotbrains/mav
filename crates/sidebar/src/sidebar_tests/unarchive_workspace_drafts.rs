use super::*;

#[gpui::test]
async fn test_unarchive_into_new_workspace_does_not_create_duplicate_real_thread(
    cx: &mut TestAppContext,
) {
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
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    cx.run_until_parked();

    let session_id = acp::SessionId::new(Arc::from("restore-into-new-workspace"));
    let path_list_b = PathList::new(&[PathBuf::from("/project-b")]);
    let original_thread_id = ThreadId::new();
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id: original_thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Unarchived Thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&path_list_b),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(original_thread_id)
            .cloned()
            .expect("metadata should exist before unarchive")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });

    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "expected unarchive to open the target workspace"
    );

    let restored_workspace = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|workspace| PathList::new(&workspace.read(cx).root_paths(cx)) == path_list_b)
            .cloned()
            .expect("expected restored workspace for unarchived thread")
    });
    let restored_panel = restored_workspace.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("expected unarchive to install an agent panel in the new workspace")
    });

    let restored_thread_id = restored_panel.read_with(cx, |panel, cx| panel.active_thread_id(cx));
    assert_eq!(
        restored_thread_id,
        Some(original_thread_id),
        "expected the new workspace's agent panel to target the restored archived thread id"
    );

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
        "expected exactly one metadata row for restored session after opening a new workspace, got: {session_entries:?}"
    );
    assert_eq!(
        session_entries[0].thread_id, original_thread_id,
        "expected restore into a new workspace to reuse the original thread id"
    );
    assert!(
        !session_entries[0].archived,
        "expected restored thread metadata to be unarchived, got: {:?}",
        session_entries[0]
    );

    let mapped_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
    });
    assert_eq!(
        mapped_thread_id,
        Some(original_thread_id),
        "expected session mapping to remain stable after opening the new workspace"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let real_thread_rows = entries
        .iter()
        .filter(|entry| !entry.starts_with("v ") && !entry.starts_with("> "))
        .filter(|entry| !entry.contains("Draft"))
        .count();
    assert_eq!(
        real_thread_rows, 1,
        "expected exactly one visible real thread row after restore into a new workspace, got entries: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.contains("Unarchived Thread")),
        "expected restored thread row to be visible, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_into_existing_workspace_replaces_draft(cx: &mut TestAppContext) {
    // When a workspace already exists with an empty draft and a thread
    // is unarchived into it, the draft should be replaced — not kept
    // alongside the loaded thread.
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

    // Create a thread and send a message so it's no longer a draft.
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel, connection, cx);
    agent_ui::test_support::send_message(&panel, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    // Archive the thread — the group is left empty (no draft created).
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    // Un-archive the thread.
    let thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("thread should exist in store")
    });
    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("metadata should exist")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    // The draft should be gone — only the unarchived thread remains.
    let entries = visible_entries_as_strings(&sidebar, cx);
    let draft_count = entries.iter().filter(|e| e.contains("Draft")).count();
    assert_eq!(
        draft_count, 0,
        "expected no drafts after unarchiving, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_into_inactive_existing_workspace_does_not_leave_active_draft(
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

    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    let session_id = acp::SessionId::new(Arc::from("unarchive-into-inactive-existing-workspace"));
    let thread_id = ThreadId::new();
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Restored In Inactive Workspace".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[
                        PathBuf::from("/project-b"),
                    ])),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("archived metadata should exist before restore")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });

    let panel_b_before_settle = workspace_b.read_with(cx, |workspace, cx| {
        workspace.panel::<AgentPanel>(cx).expect(
            "target workspace should still have an agent panel immediately after activation",
        )
    });
    let immediate_active_thread_id =
        panel_b_before_settle.read_with(cx, |panel, cx| panel.active_thread_id(cx));

    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id,
            "unarchiving into an inactive existing workspace should end on the restored thread",
        );
    });

    let panel_b = workspace_b.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("target workspace should still have an agent panel")
    });
    assert_eq!(
        panel_b.read_with(cx, |panel, cx| panel.active_thread_id(cx)),
        Some(thread_id),
        "expected target panel to activate the restored thread id"
    );
    assert!(
        immediate_active_thread_id.is_none() || immediate_active_thread_id == Some(thread_id),
        "expected immediate panel state to be either still loading or already on the restored thread, got active_thread_id={immediate_active_thread_id:?}"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let target_rows: Vec<_> = entries
        .iter()
        .filter(|entry| entry.contains("Restored In Inactive Workspace") || entry.contains("Draft"))
        .cloned()
        .collect();
    assert_eq!(
        target_rows.len(),
        1,
        "expected only the restored row and no surviving draft in the target group, got entries: {entries:?}"
    );
    assert!(
        target_rows[0].contains("Restored In Inactive Workspace"),
        "expected the remaining row to be the restored thread, got entries: {entries:?}"
    );
    assert!(
        !target_rows[0].contains("Draft"),
        "expected no surviving draft row after unarchive into inactive existing workspace, got entries: {entries:?}"
    );
}
