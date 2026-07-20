use super::*;

#[gpui::test]
async fn test_collapse_state_survives_worktree_key_change(cx: &mut TestAppContext) {
    // When a worktree is added to a project, the project group key changes.
    // The sidebar's collapsed/expanded state is keyed by ProjectGroupKey, so
    // UI state must survive the key change.
    let (_fs, project) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project-a]", "  Thread 2", "  Thread 1",]
    );

    // Collapse the group.
    let old_key = project.read_with(cx, |project, cx| project.project_group_key(cx));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.toggle_collapse(&old_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["> [project-a]"]
    );

    // Add a second worktree — the key changes from [/project-a] to
    // [/project-a, /project-b].
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // The group should still be collapsed under the new key.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["> [project-a, project-b]"]
    );
}

#[gpui::test]
async fn test_visible_entries_as_strings(cx: &mut TestAppContext) {
    use workspace::ProjectGroup;

    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let expanded_path = PathList::new(&[std::path::PathBuf::from("/expanded")]);
    let collapsed_path = PathList::new(&[std::path::PathBuf::from("/collapsed")]);

    // Set the collapsed group state through multi_workspace
    multi_workspace.update(cx, |mw, _cx| {
        mw.test_add_project_group(ProjectGroup {
            key: ProjectGroupKey::new(None, collapsed_path.clone()),
            workspaces: Vec::new(),
            expanded: false,
        });
    });

    sidebar.update_in(cx, |s, _window, _cx| {
        let notified_thread_id = ThreadId::new();
        s.contents.notified_threads.insert(notified_thread_id);
        s.contents.entries = vec![
            // Expanded project header
            ListEntry::ProjectHeader {
                key: ProjectGroupKey::new(None, expanded_path.clone()),
                label: "expanded-project".into(),
                highlight_positions: Vec::new(),
                has_running_threads: false,
                waiting_thread_count: 0,
                has_notifications: false,
                is_active: true,
                has_threads: true,
            },
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-1"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Completed thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Completed,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: false,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Active thread with Running status
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-2"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Running thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Running,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Active thread with Error status
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-3"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Error thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Error,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Thread with WaitingForConfirmation status, not active
            // remote_connection: None,
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-4"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Waiting thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::WaitingForConfirmation,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: false,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Background thread that completed (should show notification)
            // remote_connection: None,
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: notified_thread_id,
                    session_id: Some(acp::SessionId::new(Arc::from("t-5"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Notified thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Completed,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: true,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Collapsed project header
            ListEntry::ProjectHeader {
                key: ProjectGroupKey::new(None, collapsed_path.clone()),
                label: "collapsed-project".into(),
                highlight_positions: Vec::new(),
                has_running_threads: false,
                waiting_thread_count: 0,
                has_notifications: false,
                is_active: false,
                has_threads: false,
            },
        ];

        // Select the Running thread (index 2)
        s.selection = Some(2);
    });

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [expanded-project]",
            "  Completed thread",
            "  Running thread * (running)  <== selected",
            "  Error thread * (error)",
            "  Waiting thread (waiting)",
            "  Notified thread * (!)",
            "> [collapsed-project]",
        ]
    );

    // Move selection to the collapsed header
    sidebar.update_in(cx, |s, _window, _cx| {
        s.selection = Some(6);
    });

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx).last().cloned(),
        Some("> [collapsed-project]  <== selected".to_string()),
    );

    // Clear selection
    sidebar.update_in(cx, |s, _window, _cx| {
        s.selection = None;
    });

    // No entry should have the selected marker
    let entries = visible_entries_as_strings(&sidebar, cx);
    for entry in &entries {
        assert!(
            !entry.contains("<== selected"),
            "unexpected selection marker in: {}",
            entry
        );
    }
}
