use super::*;

async fn test_new_thread_button_works_after_adding_folder(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/project-a", cx).await;
    let fs = cx.update(|cx| <dyn fs::Fs>::global(cx));
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Start a thread and send a message so it has history.
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);
    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    // Verify the thread appears in the sidebar.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Hello *",
        ]
    );

    // The "New Thread" button should NOT be in "active/draft" state
    // because the panel has a thread with messages.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { .. })),
            "Panel has a thread with messages, so active_entry should be Thread, got {:?}",
            sidebar.active_entry,
        );
    });

    // Now add a second folder to the workspace, changing the path_list.
    fs.as_fake()
        .insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    // The workspace path_list is now [project-a, project-b]. The active
    // thread's metadata was re-saved with the new paths by the agent panel's
    // project subscription. The old [project-a] key is replaced by the new
    // key since no other workspace claims it.
    let entries = visible_entries_as_strings(&sidebar, cx);
    // After adding a worktree, the thread migrates to the new group key.
    // A reconciliation draft may appear during the transition.
    assert!(
        entries.contains(&"  Hello *".to_string()),
        "thread should still be present after adding folder: {entries:?}"
    );
    assert_eq!(entries[0], "v [project-a, project-b]");

    // The "New Thread" button must still be clickable (not stuck in
    // "active/draft" state). Verify that `active_thread_is_draft` is
    // false — the panel still has the old thread with messages.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { .. })),
            "After adding a folder the panel still has a thread with messages, \
                 so active_entry should be Thread, got {:?}",
            sidebar.active_entry,
        );
    });

    // Actually click "New Thread" by calling create_new_thread and
    // verify a new draft is created.
    let workspace = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_thread(&workspace, window, cx);
    });
    cx.run_until_parked();

    // After creating a new thread, the panel should now be in draft
    // state (no messages on the new thread).
    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_draft(
            sidebar,
            &workspace,
            "After creating a new thread active_entry should be Draft",
        );
    });
}

#[gpui::test]
async fn test_draft_title_updates_from_editor_text(cx: &mut TestAppContext) {
    // When the user types into a draft, the parked draft entry's title in
    // the sidebar should reflect the editor's text — both while the
    // draft's `ConversationView` is still loaded (source: live message
    // editor) and after it has been evicted (source: kvp draft prompt
    // store, the same path used when drafts are restored from disk).
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Open an ephemeral draft via a stub connection so the conversation
    // view reaches Connected synchronously and the panel's draft_thread
    // pointer is populated.
    let connection = StubAgentConnection::new();
    agent_ui::test_support::open_draft_with_connection(&panel, connection, cx);
    cx.run_until_parked();
    let draft_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

    // Type into the (active) draft's message editor. The helper drains the
    // kvp-write debounce, so by the time it returns the prompt is on disk
    // — important for Phase 2 below, which exercises the kvp fallback.
    agent_ui::test_support::type_draft_prompt(&panel, "Fix the login bug", cx);

    // Park the draft by pressing Cmd-N while it has content.
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    let draft_title = |sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext| {
        sidebar.read_with(cx, |sidebar, _cx| {
            sidebar
                .contents
                .entries
                .iter()
                .find_map(|entry| match entry {
                    ListEntry::Thread(thread)
                        if thread.draft.is_some() && thread.metadata.thread_id == draft_id =>
                    {
                        Some(thread.metadata.display_title())
                    }
                    _ => None,
                })
                .expect("parked draft entry should be present")
        })
    };

    // Phase 1: ConversationView is still loaded in `retained_threads`;
    // the title comes from its live message editor.
    assert_eq!(
        draft_title(&sidebar, cx).as_ref(),
        "Fix the login bug",
        "parked draft title should match its editor text while loaded"
    );
    panel.read_with(cx, |panel, _cx| {
        assert!(
            panel.retained_threads().contains_key(&draft_id),
            "draft should be in retained_threads while loaded"
        );
    });

    // Phase 2: drop the draft's ConversationView from memory, mirroring
    // the state the sidebar sees immediately after a process restart
    // — the metadata row and the kvp draft prompt are on disk, but no
    // ConversationView has been rehydrated yet.
    let unloaded = panel.update(cx, |panel, _cx| panel.test_unload_retained_thread(draft_id));
    assert!(unloaded, "draft should have been present before unload");
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    assert_eq!(
        draft_title(&sidebar, cx).as_ref(),
        "Fix the login bug",
        "parked draft title should still come from the kvp draft prompt store \
         even after its ConversationView is unloaded"
    );
}

#[gpui::test]
async fn test_thread_switcher_includes_parked_draft(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-existing")),
        Some("Existing Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    let connection = StubAgentConnection::new();
    agent_ui::test_support::open_draft_with_connection(&panel, connection, cx);
    cx.run_until_parked();
    let draft_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    agent_ui::test_support::type_draft_prompt(&panel, "Fix the login bug", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(sidebar.contents.entries.iter().any(|entry| {
            matches!(entry, ListEntry::Thread(thread) if thread.metadata.thread_id == draft_id)
        }));
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, cx| {
        let switcher = sidebar
            .thread_switcher
            .as_ref()
            .expect("switcher should be open");
        assert!(switcher.read(cx).entries().iter().any(|entry| {
            matches!(entry.thread_id(), Some(thread_id) if thread_id == draft_id)
        }));
    });
}

#[gpui::test]
async fn test_plus_button_reuses_empty_draft(cx: &mut TestAppContext) {
    // Clicking `+` when an empty draft is already active should focus it
    // instead of creating and parking a new one.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Open an initial draft against a stub so it connects synchronously.
    let connection = StubAgentConnection::new();
    agent_ui::test_support::open_draft_with_connection(&panel, connection, cx);
    cx.run_until_parked();

    let first_id = panel.read_with(cx, |panel, cx| {
        panel
            .active_thread_id(cx)
            .expect("draft should be active after open_draft_with_connection")
    });

    // Cmd-N with an empty draft should reuse it.
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    let second_id = panel.read_with(cx, |panel, cx| {
        panel
            .active_thread_id(cx)
            .expect("draft should still be active after Cmd-N")
    });
    assert_eq!(
        first_id, second_id,
        "an empty draft should be reused, not replaced"
    );
    // The active empty draft is surfaced in the sidebar as a single
    // "New {agent} Thread" placeholder so the sidebar mirrors the panel.
    let draft_rows: Vec<_> = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .contents
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ListEntry::Thread(t) if t.draft.is_some() => Some(t.clone()),
                _ => None,
            })
            .collect()
    });
    assert_eq!(
        draft_rows.len(),
        1,
        "active empty draft should appear as exactly one placeholder row"
    );
    assert_eq!(
        draft_rows[0].draft,
        Some(DraftKind::Empty),
        "the row should be the empty-draft placeholder"
    );
    assert_eq!(draft_rows[0].metadata.thread_id, first_id);
}

#[gpui::test]
async fn test_plus_button_parks_nonempty_draft(cx: &mut TestAppContext) {
    // Clicking `+` while the current draft has content should park the
    // current draft (surface it as a sidebar row) and create a new empty
    // draft as active.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Open a draft via a stub so the ConversationView reaches Connected and
    // we can type into its editor.
    let connection = StubAgentConnection::new();
    agent_ui::test_support::open_draft_with_connection(&panel, connection, cx);
    cx.run_until_parked();
    let first_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    let thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let editor = thread_view.read_with(cx, |view, _| view.message_editor.clone());
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("something the user typed", window, cx);
    });
    cx.run_until_parked();

    // Cmd-N parks the first draft and creates a new empty draft.
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    let second_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    assert_ne!(
        first_id, second_id,
        "non-empty draft should be parked and a fresh draft activated"
    );

    // Both drafts now appear as sidebar rows: the parked one with its
    // editor-derived title (real user state), and the newly-created empty
    // draft as a "New {agent} Thread" placeholder. The placeholder mirrors
    // the panel's current view; the parked row preserves typed content.
    let draft_rows: Vec<_> = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .contents
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ListEntry::Thread(t) if t.draft.is_some() => Some(t.clone()),
                _ => None,
            })
            .collect()
    });
    assert_eq!(
        draft_rows.len(),
        2,
        "expected two draft rows (parked + new empty placeholder), got {:?}",
        draft_rows
            .iter()
            .map(|t| t.metadata.display_title())
            .collect::<Vec<_>>()
    );
    let parked = draft_rows
        .iter()
        .find(|t| t.metadata.thread_id == first_id)
        .expect("parked draft should be present");
    assert_eq!(
        parked.draft,
        Some(DraftKind::WithContent),
        "the parked draft has user content and is not an empty placeholder"
    );
    let new_empty = draft_rows
        .iter()
        .find(|t| t.metadata.thread_id == second_id)
        .expect("new empty draft should be present");
    assert_eq!(
        new_empty.draft,
        Some(DraftKind::Empty),
        "the freshly-created draft should be an empty placeholder"
    );
    assert_eq!(
        parked.metadata.display_title().as_ref(),
        "something the user typed"
    );

    // Reproduce the real-world inversion deterministically: parking
    // re-saves the filled draft, which can leave its display time newer
    // than the brand-new empty draft's. Force that here by pushing the
    // parked draft's `updated_at` into the future.
    cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx);
        let mut parked_meta = store
            .read(cx)
            .entry(first_id)
            .expect("parked draft metadata should exist")
            .clone();
        parked_meta.interacted_at = None;
        parked_meta.updated_at = Utc::now() + chrono::Duration::hours(1);
        store.update(cx, |store, cx| store.save(parked_meta, cx));
    });
    cx.run_until_parked();

    // The empty-draft placeholder must still sort ABOVE the parked draft
    // despite the parked draft's newer timestamp — it's pinned to the top.
    let (empty_ix, parked_ix) = sidebar.read_with(cx, |sidebar, _| {
        let position = |id: ThreadId| {
            sidebar.contents.entries.iter().position(
                |entry| matches!(entry, ListEntry::Thread(t) if t.metadata.thread_id == id),
            )
        };
        (
            position(second_id).expect("empty draft row should be present"),
            position(first_id).expect("parked draft row should be present"),
        )
    });
    assert!(
        empty_ix < parked_ix,
        "the new empty draft (ix {empty_ix}) should sort above the parked filled draft (ix {parked_ix})"
    );
}
