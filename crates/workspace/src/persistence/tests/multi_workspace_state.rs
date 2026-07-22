use super::*;

#[gpui::test]
async fn test_last_session_workspace_locations_groups_by_window_id(cx: &mut gpui::TestAppContext) {
    let dir1 = tempfile::TempDir::with_prefix("dir1").unwrap();
    let dir2 = tempfile::TempDir::with_prefix("dir2").unwrap();
    let dir3 = tempfile::TempDir::with_prefix("dir3").unwrap();
    let dir4 = tempfile::TempDir::with_prefix("dir4").unwrap();
    let dir5 = tempfile::TempDir::with_prefix("dir5").unwrap();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(dir1.path(), json!({})).await;
    fs.insert_tree(dir2.path(), json!({})).await;
    fs.insert_tree(dir3.path(), json!({})).await;
    fs.insert_tree(dir4.path(), json!({})).await;
    fs.insert_tree(dir5.path(), json!({})).await;

    let db = WorkspaceDb::open_test_db("test_last_session_workspace_locations_groups_by_window_id")
        .await;

    // Simulate two MultiWorkspace windows each containing two workspaces,
    // plus one single-workspace window:
    //   Window 10: workspace 1, workspace 2
    //   Window 20: workspace 3, workspace 4
    //   Window 30: workspace 5 (only one)
    //
    // On session restore, the caller should be able to group these by
    // window_id to reconstruct the MultiWorkspace windows.
    let workspaces_data: Vec<(i64, &Path, u64)> = vec![
        (1, dir1.path(), 10),
        (2, dir2.path(), 10),
        (3, dir3.path(), 20),
        (4, dir4.path(), 20),
        (5, dir5.path(), 30),
    ];

    for (id, dir, window_id) in &workspaces_data {
        db.save_workspace(SerializedWorkspace {
            id: WorkspaceId(*id),
            paths: PathList::new(&[*dir]),
            identity_paths: None,
            location: SerializedWorkspaceLocation::Local,
            center_group: Default::default(),
            window_bounds: Default::default(),
            display: Default::default(),
            docks: Default::default(),
            centered_layout: false,
            session_id: Some("test-session".to_owned()),
            bookmarks: Default::default(),
            breakpoints: Default::default(),
            window_id: Some(*window_id),
            user_toolchains: Default::default(),
        })
        .await;
    }

    let locations = db
        .last_session_workspace_locations("test-session", None, fs.as_ref())
        .await
        .unwrap();

    // All 5 workspaces should be returned with their window_ids.
    assert_eq!(locations.len(), 5);

    // Every entry should have a window_id so the caller can group them.
    for session_workspace in &locations {
        assert!(
            session_workspace.window_id.is_some(),
            "workspace {:?} missing window_id",
            session_workspace.workspace_id
        );
    }

    // Group by window_id, simulating what the restoration code should do.
    let mut by_window: HashMap<WindowId, Vec<WorkspaceId>> = HashMap::default();
    for session_workspace in &locations {
        if let Some(window_id) = session_workspace.window_id {
            by_window
                .entry(window_id)
                .or_default()
                .push(session_workspace.workspace_id);
        }
    }

    // Should produce 3 windows, not 5.
    assert_eq!(
        by_window.len(),
        3,
        "Expected 3 window groups, got {}: {:?}",
        by_window.len(),
        by_window
    );

    // Window 10 should contain workspaces 1 and 2.
    let window_10 = by_window.get(&WindowId::from(10u64)).unwrap();
    assert_eq!(window_10.len(), 2);
    assert!(window_10.contains(&WorkspaceId(1)));
    assert!(window_10.contains(&WorkspaceId(2)));

    // Window 20 should contain workspaces 3 and 4.
    let window_20 = by_window.get(&WindowId::from(20u64)).unwrap();
    assert_eq!(window_20.len(), 2);
    assert!(window_20.contains(&WorkspaceId(3)));
    assert!(window_20.contains(&WorkspaceId(4)));

    // Window 30 should contain only workspace 5.
    let window_30 = by_window.get(&WindowId::from(30u64)).unwrap();
    assert_eq!(window_30.len(), 1);
    assert!(window_30.contains(&WorkspaceId(5)));
}

#[gpui::test]
async fn test_read_serialized_multi_workspaces_with_state(cx: &mut gpui::TestAppContext) {
    use crate::persistence::model::MultiWorkspaceState;

    // Write multi-workspace state for two windows via the scoped KVP.
    let window_10 = WindowId::from(10u64);
    let window_20 = WindowId::from(20u64);

    let kvp = cx.update(|cx| KeyValueStore::global(cx));

    write_multi_workspace_state(
        &kvp,
        window_10,
        MultiWorkspaceState {
            active_workspace_id: Some(WorkspaceId(2)),
            project_groups: vec![],
            sidebar_open: true,
            sidebar_state: None,
        },
    )
    .await;

    write_multi_workspace_state(
        &kvp,
        window_20,
        MultiWorkspaceState {
            active_workspace_id: Some(WorkspaceId(3)),
            project_groups: vec![],
            sidebar_open: false,
            sidebar_state: None,
        },
    )
    .await;

    // Build session workspaces: two in window 10, one in window 20, one with no window.
    let session_workspaces = vec![
        SessionWorkspace {
            workspace_id: WorkspaceId(1),
            location: SerializedWorkspaceLocation::Local,
            paths: PathList::new(&["/a"]),
            window_id: Some(window_10),
        },
        SessionWorkspace {
            workspace_id: WorkspaceId(2),
            location: SerializedWorkspaceLocation::Local,
            paths: PathList::new(&["/b"]),
            window_id: Some(window_10),
        },
        SessionWorkspace {
            workspace_id: WorkspaceId(3),
            location: SerializedWorkspaceLocation::Local,
            paths: PathList::new(&["/c"]),
            window_id: Some(window_20),
        },
        SessionWorkspace {
            workspace_id: WorkspaceId(4),
            location: SerializedWorkspaceLocation::Local,
            paths: PathList::new(&["/d"]),
            window_id: None,
        },
    ];

    let results = cx.update(|cx| read_serialized_multi_workspaces(session_workspaces, cx));

    // Should produce 3 results: window 10, window 20, and the orphan.
    assert_eq!(results.len(), 3);

    // Window 10: active_workspace_id = 2 picks workspace 2 (paths /b), sidebar open.
    let group_10 = &results[0];
    assert_eq!(group_10.active_workspace.workspace_id, WorkspaceId(2));
    assert_eq!(group_10.state.active_workspace_id, Some(WorkspaceId(2)));
    assert_eq!(group_10.state.sidebar_open, true);

    // Window 20: active_workspace_id = 3 picks workspace 3 (paths /c), sidebar closed.
    let group_20 = &results[1];
    assert_eq!(group_20.active_workspace.workspace_id, WorkspaceId(3));
    assert_eq!(group_20.state.active_workspace_id, Some(WorkspaceId(3)));
    assert_eq!(group_20.state.sidebar_open, false);

    // Orphan: no active_workspace_id, falls back to first workspace (id 4).
    let group_none = &results[2];
    assert_eq!(group_none.active_workspace.workspace_id, WorkspaceId(4));
    assert_eq!(group_none.state.active_workspace_id, None);
    assert_eq!(group_none.state.sidebar_open, false);
}
