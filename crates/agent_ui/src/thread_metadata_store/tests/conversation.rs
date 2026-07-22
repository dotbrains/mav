use super::*;

async fn test_draft_thread_metadata_promotes_on_first_message(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None::<&Path>, cx).await;
    let connection = StubAgentConnection::new();

    let (panel, mut vcx) = setup_panel_with_project(project, cx);
    crate::test_support::open_thread_with_connection(&panel, connection, &mut vcx);

    let thread = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    let session_id = thread.read_with(&vcx, |t, _| t.session_id().clone());
    let thread_id = crate::test_support::active_thread_id(&panel, &vcx);

    // Empty (draft) threads are persisted with `session_id: None`.
    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(store.entry_ids().count(), 1);
        let entry = store.entry(thread_id).expect("draft metadata row");
        assert!(
            entry.is_draft(),
            "expected draft row to have session_id=None, got {:?}",
            entry.session_id
        );
    });

    // Updating the title while still a draft keeps the row as a draft.
    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.set_title("Draft Thread".into(), cx).detach();
    });
    vcx.run_until_parked();

    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store.entry(thread_id).expect("draft metadata row");
        assert!(entry.is_draft(), "still a draft after title update");
        assert_eq!(
            entry.title.as_ref().map(|t| t.as_ref()),
            Some("Draft Thread")
        );
    });

    // Pushing content promotes the draft: session_id is now populated.
    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "Hello".into(), cx);
    });
    vcx.run_until_parked();

    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(store.entry_ids().count(), 1);
        assert_eq!(
            store.entry(thread_id).unwrap().session_id.as_ref(),
            Some(&session_id),
        );
    });
}

async fn test_nonempty_thread_metadata_preserved_when_thread_released(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None::<&Path>, cx).await;
    let connection = StubAgentConnection::new();

    let (panel, mut vcx) = setup_panel_with_project(project, cx);
    crate::test_support::open_thread_with_connection(&panel, connection, &mut vcx);

    let session_id = crate::test_support::active_session_id(&panel, &vcx);
    let thread = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());

    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "Hello".into(), cx);
    });
    vcx.run_until_parked();

    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(store.entry_ids().count(), 1);
        assert!(store.entry_by_session(&session_id).is_some());
    });

    // Dropping the panel releases the ConversationView and its thread.
    drop(panel);
    cx.update(|_| {});
    cx.run_until_parked();

    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(store.entry_ids().count(), 1);
        assert!(store.entry_by_session(&session_id).is_some());
    });
}

async fn test_threads_without_project_association_are_archived_by_default(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project_without_worktree = Project::test(fs.clone(), None::<&Path>, cx).await;
    let project_with_worktree = Project::test(fs, [Path::new("/project-a")], cx).await;

    // Thread in project without worktree
    let (panel_no_wt, mut vcx_no_wt) = setup_panel_with_project(project_without_worktree, cx);
    crate::test_support::open_thread_with_connection(
        &panel_no_wt,
        StubAgentConnection::new(),
        &mut vcx_no_wt,
    );
    let thread_no_wt = panel_no_wt.read_with(&vcx_no_wt, |panel, cx| {
        panel.active_agent_thread(cx).unwrap()
    });
    thread_no_wt.update_in(&mut vcx_no_wt, |thread, _window, cx| {
        thread.push_user_content_block(None, "content".into(), cx);
        thread.set_title("No Project Thread".into(), cx).detach();
    });
    vcx_no_wt.run_until_parked();
    let session_without_worktree = crate::test_support::active_session_id(&panel_no_wt, &vcx_no_wt);

    // Thread in project with worktree
    let (panel_wt, mut vcx_wt) = setup_panel_with_project(project_with_worktree, cx);
    crate::test_support::open_thread_with_connection(
        &panel_wt,
        StubAgentConnection::new(),
        &mut vcx_wt,
    );
    let thread_wt = panel_wt.read_with(&vcx_wt, |panel, cx| panel.active_agent_thread(cx).unwrap());
    thread_wt.update_in(&mut vcx_wt, |thread, _window, cx| {
        thread.push_user_content_block(None, "content".into(), cx);
        thread.set_title("Project Thread".into(), cx).detach();
    });
    vcx_wt.run_until_parked();
    let session_with_worktree = crate::test_support::active_session_id(&panel_wt, &vcx_wt);

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let without_worktree = store
            .entry_by_session(&session_without_worktree)
            .expect("missing metadata for thread without project association");
        assert!(without_worktree.folder_paths().is_empty());
        assert!(
            without_worktree.archived,
            "expected thread without project association to be archived"
        );

        let with_worktree = store
            .entry_by_session(&session_with_worktree)
            .expect("missing metadata for thread with project association");
        assert_eq!(
            *with_worktree.folder_paths(),
            PathList::new(&[Path::new("/project-a")])
        );
        assert!(
            !with_worktree.archived,
            "expected thread with project association to remain unarchived"
        );
    });
}

async fn test_subagent_threads_excluded_from_sidebar_metadata(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None::<&Path>, cx).await;
    let connection = Rc::new(StubAgentConnection::new());

    // Create a regular (non-subagent) thread through the panel.
    let (panel, mut vcx) = setup_panel_with_project(project.clone(), cx);
    crate::test_support::open_thread_with_connection(&panel, (*connection).clone(), &mut vcx);

    let regular_thread = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    let regular_session_id = regular_thread.read_with(&vcx, |t, _| t.session_id().clone());

    regular_thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "content".into(), cx);
        thread.set_title("Regular Thread".into(), cx).detach();
    });
    vcx.run_until_parked();

    // Create a standalone subagent AcpThread (not wrapped in a
    // ConversationView). The ThreadMetadataStore only observes
    // ConversationView events, so this thread's events should
    // have no effect on sidebar metadata.
    let subagent_session_id = acp::SessionId::new("subagent-session");
    let subagent_thread = cx.update(|cx| {
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        cx.new(|cx| {
            acp_thread::AcpThread::new(
                Some(regular_session_id.clone()),
                Some("Subagent Thread".into()),
                None,
                connection.clone(),
                project.clone(),
                action_log,
                subagent_session_id.clone(),
                watch::Receiver::constant(acp::PromptCapabilities::new()),
                cx,
            )
        })
    });

    cx.update(|cx| {
        subagent_thread.update(cx, |thread, cx| {
            thread
                .set_title("Subagent Thread Title".into(), cx)
                .detach();
        });
    });
    cx.run_until_parked();

    // Only the regular thread should appear in sidebar metadata.
    // The subagent thread is excluded because the metadata store
    // only observes ConversationView events.
    let list = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.read(cx).entries().cloned().collect::<Vec<_>>()
    });

    assert_eq!(
        list.len(),
        1,
        "Expected only the regular thread in sidebar metadata, \
             but found {} entries (subagent threads are leaking into the sidebar)",
        list.len(),
    );
    assert_eq!(list[0].session_id.as_ref().unwrap(), &regular_session_id);
    assert_eq!(list[0].display_title(), "Regular Thread");
}
