use super::*;

#[gpui::test]
async fn test_flush_serialization_completes_before_quit(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let db = cx.update(|_, cx| WorkspaceDb::global(cx));

    // Assign a database_id so serialization will actually persist.
    let workspace_id = db.next_id().await.unwrap();
    workspace.update(cx, |ws, _cx| {
        ws.set_database_id(workspace_id);
    });

    // Mutate some workspace state.
    db.set_centered_layout(workspace_id, true).await.unwrap();

    // Call flush_serialization and await the returned task directly
    // (without run_until_parked — the point is that awaiting the task
    // alone is sufficient).
    let task = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.workspace()
            .update(cx, |ws, cx| ws.flush_serialization(window, cx))
    });
    task.await;

    // Read the workspace back from the DB and verify serialization happened.
    let serialized = db.workspace_for_id(workspace_id);
    assert!(
        serialized.is_some(),
        "flush_serialization should have persisted the workspace to DB"
    );
}

#[gpui::test]
async fn test_create_workspace_serialization(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    // Give the first workspace a database_id.
    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.set_random_database_id(cx);
    });

    let window_id =
        multi_workspace.update_in(cx, |_, window, _cx| window.window_handle().window_id());

    // Create a new workspace via the MultiWorkspace API (triggers next_id()).
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.create_test_workspace(window, cx).detach();
    });

    // Let the async next_id() and re-serialization tasks complete.
    cx.run_until_parked();

    // The new workspace should now have a database_id.
    let new_workspace_db_id =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).database_id());
    assert!(
        new_workspace_db_id.is_some(),
        "New workspace should have a database_id after run_until_parked"
    );

    // The multi-workspace state should record it as the active workspace.
    let state = cx.update(|_, cx| read_multi_workspace_state(window_id, cx));
    assert_eq!(
        state.active_workspace_id, new_workspace_db_id,
        "Serialized active_workspace_id should match the new workspace's database_id"
    );

    // The individual workspace row should exist with real data
    // (not just the bare DEFAULT VALUES row from next_id).
    let workspace_id = new_workspace_db_id.unwrap();
    let db = cx.update(|_, cx| WorkspaceDb::global(cx));
    let serialized = db.workspace_for_id(workspace_id);
    assert!(
        serialized.is_some(),
        "Newly created workspace should be fully serialized in the DB after database_id assignment"
    );
}

#[gpui::test]
async fn test_remove_workspace_clears_session_binding(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir = unique_test_dir(&fs, "remove").await;
    let project1 = Project::test(fs.clone(), [], cx).await;
    let project2 = Project::test(fs.clone(), [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project1.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.set_random_database_id(cx);
    });

    let db = cx.update(|_, cx| WorkspaceDb::global(cx));

    // Get a real DB id for workspace2 so the row actually exists.
    let workspace2_db_id = db.next_id().await.unwrap();

    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = cx.new(|cx| crate::Workspace::test_new(project2.clone(), window, cx));
        workspace.update(cx, |ws: &mut crate::Workspace, _cx| {
            ws.set_database_id(workspace2_db_id)
        });
        mw.add(workspace.clone(), window, cx);
    });

    // Save a full workspace row to the DB directly.
    let session_id = format!("remove-test-session-{}", Uuid::new_v4());
    db.save_workspace(SerializedWorkspace {
        id: workspace2_db_id,
        paths: PathList::new(&[&dir]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some(session_id.clone()),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        window_id: Some(99),
        user_toolchains: Default::default(),
    })
    .await;

    assert!(
        db.workspace_for_id(workspace2_db_id).is_some(),
        "Workspace2 should exist in DB before removal"
    );

    // Remove workspace at index 1 (the second workspace).
    multi_workspace.update_in(cx, |mw, window, cx| {
        let ws = mw.workspaces().nth(1).unwrap().clone();
        mw.remove([ws], |_, _, _| unreachable!(), window, cx)
            .detach_and_log_err(cx);
    });

    cx.run_until_parked();

    // The row should still exist so it continues to appear in recent
    // projects, but the session binding should be cleared so it is not
    // restored as part of any future session.
    assert!(
        db.workspace_for_id(workspace2_db_id).is_some(),
        "Removed workspace's DB row should be preserved for recent projects"
    );

    let session_workspaces = db
        .last_session_workspace_locations("remove-test-session", None, fs.as_ref())
        .await
        .unwrap();
    let restored_ids: Vec<WorkspaceId> = session_workspaces
        .iter()
        .map(|sw| sw.workspace_id)
        .collect();
    assert!(
        !restored_ids.contains(&workspace2_db_id),
        "Removed workspace should not appear in session restoration"
    );
}

#[gpui::test]
async fn test_remove_workspace_not_restored_as_zombie(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir1 = tempfile::TempDir::with_prefix("zombie_test1").unwrap();
    let dir2 = tempfile::TempDir::with_prefix("zombie_test2").unwrap();
    fs.insert_tree(dir1.path(), json!({})).await;
    fs.insert_tree(dir2.path(), json!({})).await;

    let project1 = Project::test(fs.clone(), [], cx).await;
    let project2 = Project::test(fs.clone(), [], cx).await;

    let db = cx.update(|cx| WorkspaceDb::global(cx));

    // Get real DB ids so the rows actually exist.
    let ws1_id = db.next_id().await.unwrap();
    let ws2_id = db.next_id().await.unwrap();

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project1.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.workspace().update(cx, |ws, _cx| {
            ws.set_database_id(ws1_id);
        });
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = cx.new(|cx| crate::Workspace::test_new(project2.clone(), window, cx));
        workspace.update(cx, |ws: &mut crate::Workspace, _cx| {
            ws.set_database_id(ws2_id)
        });
        mw.add(workspace.clone(), window, cx);
    });

    let session_id = "test-zombie-session";
    let window_id_val: u64 = 42;

    db.save_workspace(SerializedWorkspace {
        id: ws1_id,
        paths: PathList::new(&[dir1.path()]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some(session_id.to_owned()),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        window_id: Some(window_id_val),
        user_toolchains: Default::default(),
    })
    .await;

    db.save_workspace(SerializedWorkspace {
        id: ws2_id,
        paths: PathList::new(&[dir2.path()]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some(session_id.to_owned()),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        window_id: Some(window_id_val),
        user_toolchains: Default::default(),
    })
    .await;

    // Remove workspace2 (index 1).
    multi_workspace.update_in(cx, |mw, window, cx| {
        let ws = mw.workspaces().nth(1).unwrap().clone();
        mw.remove([ws], |_, _, _| unreachable!(), window, cx)
            .detach_and_log_err(cx);
    });

    cx.run_until_parked();

    // The removed workspace should NOT appear in session restoration.
    let locations = db
        .last_session_workspace_locations(session_id, None, fs.as_ref())
        .await
        .unwrap();

    let restored_ids: Vec<WorkspaceId> = locations.iter().map(|sw| sw.workspace_id).collect();
    assert!(
        !restored_ids.contains(&ws2_id),
        "Removed workspace should not appear in session restoration list. Found: {:?}",
        restored_ids
    );
    assert!(
        restored_ids.contains(&ws1_id),
        "Remaining workspace should still appear in session restoration list"
    );
}

#[gpui::test]
async fn test_pending_removal_tasks_drained_on_flush(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir = unique_test_dir(&fs, "pending-removal").await;
    let project1 = Project::test(fs.clone(), [], cx).await;
    let project2 = Project::test(fs.clone(), [], cx).await;

    let db = cx.update(|cx| WorkspaceDb::global(cx));

    // Get a real DB id for workspace2 so the row actually exists.
    let workspace2_db_id = db.next_id().await.unwrap();

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project1.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.set_random_database_id(cx);
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = cx.new(|cx| crate::Workspace::test_new(project2.clone(), window, cx));
        workspace.update(cx, |ws: &mut crate::Workspace, _cx| {
            ws.set_database_id(workspace2_db_id)
        });
        mw.add(workspace.clone(), window, cx);
    });

    // Save a full workspace row to the DB directly and let it settle.
    let session_id = format!("pending-removal-session-{}", Uuid::new_v4());
    db.save_workspace(SerializedWorkspace {
        id: workspace2_db_id,
        paths: PathList::new(&[&dir]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some(session_id.clone()),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        window_id: Some(88),
        user_toolchains: Default::default(),
    })
    .await;
    cx.run_until_parked();

    // Remove workspace2 — this pushes a task to pending_removal_tasks.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let ws = mw.workspaces().nth(1).unwrap().clone();
        mw.remove([ws], |_, _, _| unreachable!(), window, cx)
            .detach_and_log_err(cx);
    });

    // Simulate the quit handler pattern: collect flush tasks + pending
    // removal tasks and await them all.
    let all_tasks = multi_workspace.update_in(cx, |mw, window, cx| {
        let mut tasks: Vec<Task<()>> = mw
            .workspaces()
            .map(|workspace| {
                workspace.update(cx, |workspace, cx| {
                    workspace.flush_serialization(window, cx)
                })
            })
            .collect();
        let mut removal_tasks = mw.take_pending_removal_tasks();
        // Note: removal_tasks may be empty if the background task already
        // completed (take_pending_removal_tasks filters out ready tasks).
        tasks.append(&mut removal_tasks);
        tasks.push(mw.flush_serialization());
        tasks
    });
    futures::future::join_all(all_tasks).await;

    // The row should still exist (for recent projects), but the session
    // binding should have been cleared by the pending removal task.
    assert!(
        db.workspace_for_id(workspace2_db_id).is_some(),
        "Workspace row should be preserved for recent projects"
    );

    let session_workspaces = db
        .last_session_workspace_locations("pending-removal-session", None, fs.as_ref())
        .await
        .unwrap();
    let restored_ids: Vec<WorkspaceId> = session_workspaces
        .iter()
        .map(|sw| sw.workspace_id)
        .collect();
    assert!(
        !restored_ids.contains(&workspace2_db_id),
        "Pending removal task should have cleared the session binding"
    );
}

#[gpui::test]
async fn test_create_workspace_bounds_observer_uses_fresh_id(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.set_random_database_id(cx);
    });

    let task = multi_workspace.update_in(cx, |mw, window, cx| mw.create_test_workspace(window, cx));
    task.await;

    let new_workspace_db_id =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).database_id());
    assert!(
        new_workspace_db_id.is_some(),
        "After run_until_parked, the workspace should have a database_id"
    );

    let workspace_id = new_workspace_db_id.unwrap();

    let db = cx.update(|_, cx| WorkspaceDb::global(cx));

    assert!(
        db.workspace_for_id(workspace_id).is_some(),
        "The workspace row should exist in the DB"
    );

    cx.simulate_resize(gpui::size(px(1024.0), px(768.0)));

    // Advance the clock past the 100ms debounce timer so the bounds
    // observer task fires
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let serialized = db
        .workspace_for_id(workspace_id)
        .expect("workspace row should still exist");
    assert!(
        serialized.window_bounds.is_some(),
        "The bounds observer should write bounds for the workspace's real DB ID, \
             even when the workspace was created via create_workspace (where the ID \
             is assigned asynchronously after construction)."
    );
}

#[gpui::test]
async fn test_flush_serialization_writes_bounds(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir = tempfile::TempDir::with_prefix("flush_bounds_test").unwrap();
    fs.insert_tree(dir.path(), json!({})).await;

    let project = Project::test(fs.clone(), [dir.path()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let db = cx.update(|_, cx| WorkspaceDb::global(cx));
    let workspace_id = db.next_id().await.unwrap();
    multi_workspace.update_in(cx, |mw, _, cx| {
        mw.workspace().update(cx, |ws, _cx| {
            ws.set_database_id(workspace_id);
        });
    });

    let task = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.workspace()
            .update(cx, |ws, cx| ws.flush_serialization(window, cx))
    });
    task.await;

    let after = db
        .workspace_for_id(workspace_id)
        .expect("workspace row should exist after flush_serialization");
    assert!(
        !after.paths.is_empty(),
        "flush_serialization should have written paths via save_workspace"
    );
    assert!(
        after.window_bounds.is_some(),
        "flush_serialization should ensure window bounds are persisted to the DB \
             before the process exits."
    );
}
