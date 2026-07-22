use super::*;

/// Regression test: archiving a thread created in a git worktree must
/// preserve the thread's folder paths so that restoring it later does
/// not prompt the user to re-associate a project.

#[gpui::test]
async fn test_archived_thread_retains_paths_after_worktree_removal(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/worktrees/feature",
        serde_json::json!({ "src": { "main.rs": "" } }),
    )
    .await;
    let project = Project::test(fs, [Path::new("/worktrees/feature")], cx).await;
    let connection = StubAgentConnection::new();

    let (panel, mut vcx) = setup_panel_with_project(project.clone(), cx);
    crate::test_support::open_thread_with_connection(&panel, connection, &mut vcx);

    let thread = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    let thread_id = crate::test_support::active_thread_id(&panel, &vcx);

    // Push content so the event handler saves metadata with the
    // project's worktree paths.
    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "Hello".into(), cx);
    });
    vcx.run_until_parked();

    // Verify paths were saved correctly.
    let (folder_paths_before, main_paths_before) = cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store.entry(thread_id).unwrap();
        assert!(
            !entry.folder_paths().is_empty(),
            "thread should have folder paths before archiving"
        );
        (
            entry.folder_paths().clone(),
            entry.main_worktree_paths().clone(),
        )
    });

    // Archive the thread.
    cx.update(|cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.archive(thread_id, None, cx);
        });
    });
    cx.run_until_parked();

    // Remove the worktree from the project, simulating what the
    // archive flow does for linked git worktrees.
    let worktree_id = cx.update(|cx| {
        project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .id()
    });
    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_id, cx);
    });
    cx.run_until_parked();

    // Trigger a thread event after archiving + worktree removal.
    // In production this happens when an async title-generation task
    // completes after the thread was archived.
    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.set_title("Generated title".into(), cx).detach();
    });
    vcx.run_until_parked();

    // The archived thread must still have its original folder paths.
    cx.read(|cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let entry = store.entry(thread_id).unwrap();
        assert!(entry.archived, "thread should still be archived");
        assert_eq!(
            entry.display_title().as_ref(),
            "Generated title",
            "title should still be updated for archived threads"
        );
        assert_eq!(
            entry.folder_paths(),
            &folder_paths_before,
            "archived thread must retain its folder paths after worktree \
                 removal + subsequent thread event, otherwise restoring it \
                 will prompt the user to re-associate a project"
        );
        assert_eq!(
            entry.main_worktree_paths(),
            &main_paths_before,
            "archived thread must retain its main worktree paths after \
                 worktree removal + subsequent thread event"
        );
    });
}

#[gpui::test]
async fn test_collab_guest_threads_not_saved_to_metadata_store(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [Path::new("/project-a")], cx).await;

    let (panel, mut vcx) = setup_panel_with_project(project.clone(), cx);
    crate::test_support::open_thread_with_connection(&panel, StubAgentConnection::new(), &mut vcx);
    let thread = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    let thread_id = crate::test_support::active_thread_id(&panel, &vcx);
    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "hello".into(), cx);
        thread.set_title("Thread".into(), cx).detach();
    });
    vcx.run_until_parked();

    // Confirm the thread is in the store while the project is local.
    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        assert!(
            store.read(cx).entry(thread_id).is_some(),
            "thread must be in the store while the project is local"
        );
    });

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.delete(thread_id, cx);
        });
    });
    project.update(cx, |project, _cx| {
        project.mark_as_collab_for_testing();
    });

    thread.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "more content".into(), cx);
    });
    vcx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        assert!(
            store.read(cx).entry(thread_id).is_none(),
            "threads must not be persisted while the project is a collab guest session"
        );
    });
}

// When a worktree is added to a collab project, update_thread_work_dirs
// fires with the new worktree paths. Without an is_via_collab() guard it
// overwrites the stored paths of any retained or active local threads with
// the new (expanded) path set, corrupting metadata that belonged to the
// guest's own local project.
#[gpui::test]
async fn test_collab_guest_retained_thread_paths_not_overwritten_on_worktree_change(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({})).await;
    fs.insert_tree("/project-b", serde_json::json!({})).await;
    let project = Project::test(fs, [Path::new("/project-a")], cx).await;

    let (panel, mut vcx) = setup_panel_with_project(project.clone(), cx);

    // Open thread A and give it content so its metadata is saved with /project-a.
    crate::test_support::open_thread_with_connection(&panel, StubAgentConnection::new(), &mut vcx);
    let thread_a_id = crate::test_support::active_thread_id(&panel, &vcx);
    let thread_a = panel.read_with(&vcx, |panel, cx| panel.active_agent_thread(cx).unwrap());
    thread_a.update_in(&mut vcx, |thread, _window, cx| {
        thread.push_user_content_block(None, "hello".into(), cx);
        thread.set_title("Thread A".into(), cx).detach();
    });
    vcx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store.read(cx).entry(thread_a_id).unwrap();
        assert_eq!(
            entry.folder_paths().paths(),
            &[std::path::PathBuf::from("/project-a")],
            "thread A must be saved with /project-a before collab"
        );
    });

    // Open thread B, making thread A a retained thread in the panel.
    crate::test_support::open_thread_with_connection(&panel, StubAgentConnection::new(), &mut vcx);
    vcx.run_until_parked();

    // Transition the project into collab mode (simulates joining as a guest).
    project.update(cx, |project, _cx| {
        project.mark_as_collab_for_testing();
    });

    // Add a second worktree. For a real collab guest this would be one of
    // the host's worktrees arriving via the collab protocol, but here we
    // use a local path because the test infrastructure cannot easily produce
    // a remote worktree with a fully-scanned root entry.
    //
    // This fires WorktreeAdded → update_thread_work_dirs. Without an
    // is_via_collab() guard that call overwrites the stored paths of
    // retained thread A from {/project-a} to {/project-a, /project-b},
    // polluting its metadata with a path it never belonged to.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(Path::new("/project-b"), true, cx)
        })
        .await
        .unwrap();
    vcx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store
            .read(cx)
            .entry(thread_a_id)
            .expect("thread A must still exist in the store");
        assert_eq!(
            entry.folder_paths().paths(),
            &[std::path::PathBuf::from("/project-a")],
            "retained thread A's stored path must not be updated while the project is via collab"
        );
    });
}
