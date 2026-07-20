use super::*;

#[gpui::test]
async fn test_linked_worktree_workspace_reachable_after_adding_unrelated_project(
    cx: &mut TestAppContext,
) {
    // Regression test for a property-test finding:
    //   AddLinkedWorktree { project_group_index: 0 }
    //   AddProject { use_worktree: true }
    //   AddProject { use_worktree: false }
    // After these three steps, the linked-worktree workspace was not
    // reachable from any sidebar entry.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);

        cx.observe_new(
            |workspace: &mut Workspace,
             window: Option<&mut Window>,
             cx: &mut gpui::Context<Workspace>| {
                if let Some(window) = window {
                    let panel = cx.new(|cx| AgentPanel::test_new(workspace, window, cx));
                    workspace.add_panel(panel, window, cx);
                }
            },
        )
        .detach();
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/my-project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/my-project".as_ref()], cx).await;
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Step 1: Create a linked worktree for the main project.
    let worktree_name = "wt-0";
    let worktree_path = "/worktrees/wt-0";

    fs.insert_tree(
        worktree_path,
        serde_json::json!({
            ".git": "gitdir: /my-project/.git/worktrees/wt-0",
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/my-project/.git/worktrees/wt-0",
        serde_json::json!({
            "commondir": "../../",
            "HEAD": "ref: refs/heads/wt-0",
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/my-project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from(worktree_path),
            ref_name: Some(format!("refs/heads/{}", worktree_name).into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    let main_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let main_project = main_workspace.read_with(cx, |ws, _| ws.project().clone());
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    // Step 2: Open the linked worktree as its own workspace.
    let worktree_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, [worktree_path.as_ref()], cx).await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });
    cx.run_until_parked();

    // Step 3: Add an unrelated project.
    fs.insert_tree(
        "/other-project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    let other_project = project::Project::test(
        fs.clone() as Arc<dyn fs::Fs>,
        ["/other-project".as_ref()],
        cx,
    )
    .await;
    other_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(other_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Force a full sidebar rebuild with all groups expanded.
    sidebar.update_in(cx, |sidebar, _window, cx| {
        if let Some(mw) = sidebar.multi_workspace.upgrade() {
            mw.update(cx, |mw, _cx| mw.test_expand_all_groups());
        }
        sidebar.update_entries(cx);
    });
    cx.run_until_parked();

    // The linked-worktree workspace must be reachable from at least one
    // sidebar entry — otherwise the user has no way to navigate to it.
    let worktree_ws_id = worktree_workspace.entity_id();
    let (all_ids, reachable_ids) = sidebar.read_with(cx, |sidebar, cx| {
        let mw = multi_workspace.read(cx);

        let all: HashSet<gpui::EntityId> = mw.workspaces().map(|ws| ws.entity_id()).collect();
        let reachable: HashSet<gpui::EntityId> = sidebar
            .contents
            .entries
            .iter()
            .flat_map(|entry| entry.reachable_workspaces(mw, cx))
            .map(|ws| ws.entity_id())
            .collect();
        (all, reachable)
    });

    let unreachable = &all_ids - &reachable_ids;
    eprintln!("{}", visible_entries_as_strings(&sidebar, cx).join("\n"));

    assert!(
        unreachable.is_empty(),
        "workspaces not reachable from any sidebar entry: {:?}\n\
         (linked-worktree workspace id: {:?})",
        unreachable,
        worktree_ws_id,
    );
}

#[gpui::test]
async fn test_startup_failed_restoration_shows_no_draft(cx: &mut TestAppContext) {
    // Empty project groups no longer auto-create drafts via reconciliation.
    // A fresh startup with no restorable thread should show only the header.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let _workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(
        entries,
        vec!["v [my-project]"],
        "empty group should show only the header, no auto-created draft"
    );
}

#[gpui::test]
async fn test_startup_successful_restoration_no_spurious_draft(cx: &mut TestAppContext) {
    // Rule 5: When the app starts and the AgentPanel successfully loads
    // a thread, no spurious draft should appear.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Create and send a message to make a real thread.
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);
    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    // Should show the thread, NOT a spurious draft.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(entries, vec!["v [my-project]", "  Hello *"]);

    // active_entry should be Thread, not Draft.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(sidebar, &session_id, "should be on the thread, not a draft");
    });
}

#[gpui::test]
async fn test_project_header_click_restores_last_viewed(cx: &mut TestAppContext) {
    // Rule 9: Clicking a project header should restore whatever the
    // user was last looking at in that group, not create new drafts
    // or jump to the first entry.
    let project_a = init_test_project_with_agent_panel("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Create two threads in project-a.
    let conn1 = StubAgentConnection::new();
    conn1.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel_a, conn1, cx);
    send_message(&panel_a, cx);
    let thread_a1 = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&thread_a1, &project_a, cx).await;

    let conn2 = StubAgentConnection::new();
    conn2.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel_a, conn2, cx);
    send_message(&panel_a, cx);
    let thread_a2 = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&thread_a2, &project_a, cx).await;
    cx.run_until_parked();

    // The user is now looking at thread_a2.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(sidebar, &thread_a2, "should be on thread_a2");
    });

    // Add project-b and switch to it.
    let fs = cx.update(|_window, cx| <dyn fs::Fs>::global(cx));
    fs.as_fake()
        .insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    let project_b =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-b".as_ref()], cx).await;
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Now switch BACK to project-a by activating its workspace.
    let workspace_a = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|ws| {
                ws.read(cx)
                    .project()
                    .read(cx)
                    .visible_worktrees(cx)
                    .any(|wt| {
                        wt.read(cx)
                            .abs_path()
                            .to_string_lossy()
                            .contains("project-a")
                    })
            })
            .unwrap()
            .clone()
    });
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // The panel should still show thread_a2 (the last thing the user
    // was viewing in project-a), not a draft or thread_a1.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(
            sidebar,
            &thread_a2,
            "switching back to project-a should restore thread_a2",
        );
    });

    // No spurious draft entries should have been created in
    // project-a's group (project-b may have a placeholder).
    let entries = visible_entries_as_strings(&sidebar, cx);
    // Find project-a's section and check it has no drafts.
    let project_a_start = entries
        .iter()
        .position(|e| e.contains("project-a"))
        .unwrap();
    let project_a_end = entries[project_a_start + 1..]
        .iter()
        .position(|e| e.starts_with("v "))
        .map(|i| i + project_a_start + 1)
        .unwrap_or(entries.len());
    let project_a_drafts = entries[project_a_start..project_a_end]
        .iter()
        .filter(|e| e.contains("Draft"))
        .count();
    assert_eq!(
        project_a_drafts, 0,
        "switching back to project-a should not create drafts in its group"
    );
}
