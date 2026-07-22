use super::*;

fn pane_with_items(item_ids: &[ItemId]) -> SerializedPaneGroup {
    SerializedPaneGroup::Pane(SerializedPane::new(
        item_ids
            .iter()
            .map(|id| SerializedItem::new("Terminal", *id, true, false))
            .collect(),
        true,
        0,
    ))
}

fn empty_pane_group() -> SerializedPaneGroup {
    SerializedPaneGroup::Pane(SerializedPane::default())
}

fn workspace_with(
    id: u64,
    paths: &[&Path],
    center_group: SerializedPaneGroup,
    session_id: Option<&str>,
) -> SerializedWorkspace {
    SerializedWorkspace {
        id: WorkspaceId(id as i64),
        paths: PathList::new(paths),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group,
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        centered_layout: false,
        session_id: session_id.map(|s| s.to_owned()),
        window_id: Some(id),
        user_toolchains: Default::default(),
    }
}

fn remote_workspace_with(id: u64, host: &str, paths: &[&Path]) -> SerializedWorkspace {
    SerializedWorkspace {
        id: WorkspaceId(id as i64),
        paths: PathList::new(paths),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Remote(RemoteConnectionOptions::Ssh(
            SshConnectionOptions {
                host: host.into(),
                ..Default::default()
            },
        )),
        center_group: empty_pane_group(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: Some(id),
        user_toolchains: Default::default(),
    }
}

async fn local_recent_workspace(
    workspace_id: WorkspaceId,
    paths: PathList,
    timestamp: DateTime<Utc>,
    fs: &dyn Fs,
) -> RecentWorkspace {
    let identity_paths = resolve_local_workspace_identity(fs, &paths)
        .await
        .unwrap_or_else(|| paths.clone());
    RecentWorkspace {
        workspace_id,
        location: SerializedWorkspaceLocation::Local,
        paths,
        identity_paths,
        timestamp,
    }
}

#[gpui::test]
async fn test_scratch_only_workspace_restores_from_last_session(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_scratch_only_workspace_restores_from_last_session").await;

    db.save_workspace(workspace_with(1, &[], pane_with_items(&[100]), Some("s1")))
        .await;

    let sessions = db
        .last_session_workspace_locations("s1", None, fs.as_ref())
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].workspace_id, WorkspaceId(1));
    assert!(sessions[0].paths.is_empty());

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();
    assert!(
        recents
            .iter()
            .all(|workspace| workspace.workspace_id != WorkspaceId(1)),
        "scratch-only workspace must not appear in the recent-projects UI"
    );
}

#[gpui::test]
async fn test_gc_preserves_scratch_inside_window(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_gc_preserves_scratch_inside_window").await;

    db.save_workspace(workspace_with(1, &[], empty_pane_group(), None))
        .await;

    db.garbage_collect_workspaces(fs.as_ref(), "current", None)
        .await
        .unwrap();
    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_some(),
        "fresh stale workspace must not be deleted before the 7-day window"
    );
}

#[gpui::test]
async fn test_gc_deletes_stale_outside_window(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_gc_deletes_stale_outside_window").await;

    db.save_workspace(workspace_with(1, &[], empty_pane_group(), None))
        .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2000-01-01 00:00:00".to_owned())
        .await
        .unwrap();

    db.garbage_collect_workspaces(fs.as_ref(), "current", None)
        .await
        .unwrap();
    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_none(),
        "stale empty workspace older than the retention window must be deleted"
    );
}

#[gpui::test]
async fn test_gc_preserves_directory_workspace_with_missing_path(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_gc_preserves_directory_workspace_with_missing_path").await;

    let missing_dir = PathBuf::from("/missing-project-dir");
    db.save_workspace(workspace_with(
        1,
        &[missing_dir.as_path()],
        empty_pane_group(),
        None,
    ))
    .await;

    db.garbage_collect_workspaces(fs.as_ref(), "current", None)
        .await
        .unwrap();
    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_some(),
        "a stale workspace within the retention window must be kept"
    );

    db.set_timestamp_for_tests(WorkspaceId(1), "2000-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.garbage_collect_workspaces(fs.as_ref(), "current", None)
        .await
        .unwrap();
    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_none(),
        "a stale workspace past the retention window must be deleted"
    );
}

#[gpui::test]
async fn test_gc_preserves_current_and_last_sessions(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_gc_preserves_current_and_last_sessions").await;

    db.save_workspace(workspace_with(1, &[], empty_pane_group(), Some("current")))
        .await;
    db.save_workspace(workspace_with(2, &[], empty_pane_group(), Some("last")))
        .await;
    db.save_workspace(workspace_with(3, &[], empty_pane_group(), Some("stale")))
        .await;

    for id in [1, 2, 3] {
        db.set_timestamp_for_tests(WorkspaceId(id), "2000-01-01 00:00:00".to_owned())
            .await
            .unwrap();
    }

    db.garbage_collect_workspaces(fs.as_ref(), "current", Some("last"))
        .await
        .unwrap();

    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_some(),
        "GC must not delete workspaces belonging to the current session"
    );
    assert!(
        db.workspace_for_id(WorkspaceId(2)).is_some(),
        "GC must not delete workspaces belonging to the last session"
    );
    assert!(
        db.workspace_for_id(WorkspaceId(3)).is_none(),
        "GC should still delete stale workspaces from other sessions"
    );
}

#[gpui::test]
async fn test_gc_deletes_empty_workspace_with_items(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_gc_deletes_empty_workspace_with_items").await;

    db.save_workspace(workspace_with(1, &[], pane_with_items(&[100]), None))
        .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2000-01-01 00:00:00".to_owned())
        .await
        .unwrap();

    db.garbage_collect_workspaces(fs.as_ref(), "current", None)
        .await
        .unwrap();
    assert!(
        db.workspace_for_id(WorkspaceId(1)).is_none(),
        "a stale empty-path workspace must be deleted regardless of its items"
    );
}

#[gpui::test]
async fn test_last_session_restores_workspace_with_missing_paths(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_last_session_restores_workspace_with_missing_paths").await;

    let missing = PathBuf::from("/gone/file.rs");
    db.save_workspace(workspace_with(
        1,
        &[missing.as_path()],
        empty_pane_group(),
        Some("s"),
    ))
    .await;

    let sessions = db
        .last_session_workspace_locations("s", None, fs.as_ref())
        .await
        .unwrap();
    assert!(
        sessions.is_empty(),
        "workspaces whose paths no longer exist on disk must not restore"
    );
}

#[gpui::test]
async fn test_last_session_workspace_locations_remote(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_serializing_workspaces_last_session_workspaces_remote")
            .await;

    let remote_connections = [
        ("host-1", "my-user-1"),
        ("host-2", "my-user-2"),
        ("host-3", "my-user-3"),
        ("host-4", "my-user-4"),
    ]
    .into_iter()
    .map(|(host, user)| async {
        let options = RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.into(),
            username: Some(user.to_string()),
            ..Default::default()
        });
        db.get_or_create_remote_connection(options.clone())
            .await
            .unwrap();
        options
    })
    .collect::<Vec<_>>();

    let remote_connections = futures::future::join_all(remote_connections).await;

    let workspaces = [
        (1, remote_connections[0].clone(), 9),
        (2, remote_connections[1].clone(), 5),
        (3, remote_connections[2].clone(), 8),
        (4, remote_connections[3].clone(), 2),
    ]
    .into_iter()
    .map(|(id, remote_connection, window_id)| SerializedWorkspace {
        id: WorkspaceId(id),
        paths: PathList::default(),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Remote(remote_connection),
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some("one-session".to_owned()),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        window_id: Some(window_id),
        user_toolchains: Default::default(),
    })
    .collect::<Vec<_>>();

    for workspace in workspaces.iter() {
        db.save_workspace(workspace.clone()).await;
    }

    let stack = Some(Vec::from([
        WindowId::from(2), // Top
        WindowId::from(8),
        WindowId::from(5),
        WindowId::from(9), // Bottom
    ]));

    let have = db
        .last_session_workspace_locations("one-session", stack, fs.as_ref())
        .await
        .unwrap();
    assert_eq!(have.len(), 4);
    assert_eq!(
        have[0],
        SessionWorkspace {
            workspace_id: WorkspaceId(4),
            location: SerializedWorkspaceLocation::Remote(remote_connections[3].clone()),
            paths: PathList::default(),
            window_id: Some(WindowId::from(2u64)),
        }
    );
    assert_eq!(
        have[1],
        SessionWorkspace {
            workspace_id: WorkspaceId(3),
            location: SerializedWorkspaceLocation::Remote(remote_connections[2].clone()),
            paths: PathList::default(),
            window_id: Some(WindowId::from(8u64)),
        }
    );
    assert_eq!(
        have[2],
        SessionWorkspace {
            workspace_id: WorkspaceId(2),
            location: SerializedWorkspaceLocation::Remote(remote_connections[1].clone()),
            paths: PathList::default(),
            window_id: Some(WindowId::from(5u64)),
        }
    );
    assert_eq!(
        have[3],
        SessionWorkspace {
            workspace_id: WorkspaceId(1),
            location: SerializedWorkspaceLocation::Remote(remote_connections[0].clone()),
            paths: PathList::default(),
            window_id: Some(WindowId::from(9u64)),
        }
    );
}
