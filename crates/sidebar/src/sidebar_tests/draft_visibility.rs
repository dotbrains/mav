use super::*;

#[gpui::test]
async fn test_cmd_n_shows_new_thread_entry_in_absorbed_worktree(cx: &mut TestAppContext) {
    // When the active workspace is an absorbed git worktree, cmd-n
    // should activate the draft thread in the panel and the sidebar
    // should surface a placeholder row for the active empty draft.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    // Main repo with a linked worktree.
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    // Worktree checkout pointing back to the main repo.
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    // Switch to the worktree workspace.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().nth(1).unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });

    // Create a non-empty thread in the worktree workspace.
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&worktree_panel, connection, cx);
    send_message(&worktree_panel, cx);

    let session_id = active_session_id(&worktree_panel, cx);
    save_test_thread_metadata(&session_id, &worktree_project, cx).await;
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project]",
            "  Hello {wt-feature-a} *",
        ]
    );

    // Simulate Cmd-N in the worktree workspace.
    worktree_panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    worktree_workspace.update_in(cx, |workspace, window, cx| {
        workspace.focus_panel::<AgentPanel>(window, cx);
    });
    cx.run_until_parked();

    // After Cmd-N the sidebar surfaces the active empty draft as a
    // placeholder row. Its worktree chip identifies which workspace it
    // belongs to (the linked worktree).
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project]",
            "  New stub Thread {wt-feature-a}",
            "  Hello {wt-feature-a} *",
        ],
        "After Cmd-N the sidebar should show a placeholder row for the active empty draft"
    );

    // The panel should be on the draft and active_entry should track it.
    worktree_panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_thread_is_draft(cx),
            "panel should be showing the draft after Cmd-N",
        );
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_draft(
            sidebar,
            &worktree_workspace,
            "active_entry should be Draft after Cmd-N",
        );
    });
}

#[gpui::test]
async fn test_only_actively_viewed_empty_draft_is_visible_in_sidebar(cx: &mut TestAppContext) {
    // The sidebar surfaces an empty-draft placeholder row only for the
    // draft that the *active workspace's panel* is currently viewing.
    // Specifically:
    //   1. Empty ephemeral drafts in non-active workspaces (e.g. a
    //      sibling linked-worktree panel) are hidden.
    //   2. An empty ephemeral that is parked in its slot while the user
    //      is viewing a real thread is hidden (it's not the active view).
    //   3. When the active workspace switches, the placeholder follows
    //      the new active panel's current view.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let (sidebar, main_panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    // `mw.workspace()` returns the *currently active* workspace, so we
    // capture the main one here before adding the worktree workspace
    // (which would make it the active one).
    let main_workspace = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);
    cx.run_until_parked();

    // Give the main panel a real thread we can park the draft behind
    // later. Send a message to promote the draft→real thread.
    let real_connection = StubAgentConnection::new();
    real_connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&main_panel, real_connection, cx);
    agent_ui::test_support::send_message(&main_panel, cx);
    let main_real_thread_id =
        main_panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    cx.run_until_parked();

    // Now open a fresh ephemeral draft in the main panel.
    agent_ui::test_support::open_draft_with_connection(&main_panel, StubAgentConnection::new(), cx);
    cx.run_until_parked();

    // And an ephemeral draft in the worktree panel as well.
    agent_ui::test_support::open_draft_with_connection(
        &worktree_panel,
        StubAgentConnection::new(),
        cx,
    );
    cx.run_until_parked();

    // `open_draft_with_connection` focuses the panel it's called on,
    // which makes that workspace active. Explicitly re-activate the main
    // workspace so the baseline assertions below describe the
    // "main-workspace-is-active" case independently of call order above.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(main_workspace.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // The invariant under test is: at most one empty-draft placeholder is
    // visible at a time, and it corresponds to the active workspace's
    // panel's currently-active draft. Counting `is_empty_draft` rows is
    // more robust than tracking specific thread_ids because draft
    // creation flows can leave behind orphan ephemeral metadata that's
    // also hidden by the filter.
    let empty_draft_rows =
        |sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext| -> Vec<ThreadId> {
            sidebar.read_with(cx, |sidebar, _| {
                sidebar
                    .contents
                    .entries
                    .iter()
                    .filter_map(|entry| match entry {
                        ListEntry::Thread(t) if t.draft == Some(DraftKind::Empty) => {
                            Some(t.metadata.thread_id)
                        }
                        _ => None,
                    })
                    .collect()
            })
        };
    let active_panel_draft_id =
        |panel: &Entity<AgentPanel>, cx: &mut gpui::VisualTestContext| -> Option<ThreadId> {
            panel.read_with(cx, |panel, cx| {
                panel
                    .active_thread_id(cx)
                    .filter(|_| panel.active_thread_is_draft(cx))
            })
        };

    // Baseline: main workspace active, main panel viewing its draft.
    // Exactly one placeholder visible, matching the main panel's draft.
    let main_active_draft =
        active_panel_draft_id(&main_panel, cx).expect("main panel should be viewing a draft");
    let visible = empty_draft_rows(&sidebar, cx);
    assert_eq!(
        visible,
        vec![main_active_draft],
        "exactly the main panel's active empty draft should be visible"
    );

    // Navigate the main panel AWAY from its draft to the real thread.
    // The draft is no longer the active view of its panel, so its
    // placeholder must disappear from the sidebar.
    main_panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            agent_ui::Agent::NativeAgent,
            main_real_thread_id,
            None,
            None,
            false,
            agent_ui::AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    main_panel.read_with(cx, |panel, cx| {
        assert_eq!(
            panel.active_thread_id(cx),
            Some(main_real_thread_id),
            "main panel should now be viewing the real thread"
        );
    });
    assert!(
        empty_draft_rows(&sidebar, cx).is_empty(),
        "no placeholder should be visible: main panel is on a real thread and worktree workspace is inactive"
    );

    // Switch the active workspace to the worktree. Now the worktree
    // panel's draft is the active view, so its placeholder appears.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace.clone(), None, window, cx);
    });
    cx.run_until_parked();

    let worktree_active_draft = active_panel_draft_id(&worktree_panel, cx)
        .expect("worktree panel should be viewing a draft");
    let visible = empty_draft_rows(&sidebar, cx);
    assert_eq!(
        visible,
        vec![worktree_active_draft],
        "exactly the worktree panel's active empty draft should be visible after switching workspaces"
    );
}
