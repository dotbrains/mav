use super::*;

#[gpui::test]
async fn test_workspace_assignment() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_basic_functionality").await;

    let workspace_1 = SerializedWorkspace {
        id: WorkspaceId(1),
        paths: PathList::new(&["/tmp", "/tmp2"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: Some(1),
        user_toolchains: Default::default(),
    };

    let mut workspace_2 = SerializedWorkspace {
        id: WorkspaceId(2),
        paths: PathList::new(&["/tmp"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: None,
        window_id: Some(2),
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace_1.clone()).await;
    db.save_workspace(workspace_2.clone()).await;

    // Test that paths are treated as a set
    assert_eq!(
        db.workspace_for_roots(&["/tmp", "/tmp2"]).unwrap(),
        workspace_1
    );
    assert_eq!(
        db.workspace_for_roots(&["/tmp2", "/tmp"]).unwrap(),
        workspace_1
    );

    // Make sure that other keys work
    assert_eq!(db.workspace_for_roots(&["/tmp"]).unwrap(), workspace_2);
    assert_eq!(db.workspace_for_roots(&["/tmp3", "/tmp2", "/tmp4"]), None);

    // Test 'mutate' case of updating a pre-existing id
    workspace_2.paths = PathList::new(&["/tmp", "/tmp2"]);

    db.save_workspace(workspace_2.clone()).await;
    assert_eq!(
        db.workspace_for_roots(&["/tmp", "/tmp2"]).unwrap(),
        workspace_2
    );

    // Test other mechanism for mutating
    let mut workspace_3 = SerializedWorkspace {
        id: WorkspaceId(3),
        paths: PathList::new(&["/tmp2", "/tmp"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: Some(3),
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace_3.clone()).await;
    assert_eq!(
        db.workspace_for_roots(&["/tmp", "/tmp2"]).unwrap(),
        workspace_3
    );

    // Make sure that updating paths differently also works
    workspace_3.paths = PathList::new(&["/tmp3", "/tmp4", "/tmp2"]);
    db.save_workspace(workspace_3.clone()).await;
    assert_eq!(db.workspace_for_roots(&["/tmp2", "tmp"]), None);
    assert_eq!(
        db.workspace_for_roots(&["/tmp2", "/tmp3", "/tmp4"])
            .unwrap(),
        workspace_3
    );
}

#[gpui::test]
async fn test_session_workspaces() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_serializing_workspaces_session_id").await;

    let workspace_1 = SerializedWorkspace {
        id: WorkspaceId(1),
        paths: PathList::new(&["/tmp1"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: Some("session-id-1".to_owned()),
        window_id: Some(10),
        user_toolchains: Default::default(),
    };

    let workspace_2 = SerializedWorkspace {
        id: WorkspaceId(2),
        paths: PathList::new(&["/tmp2"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: Some("session-id-1".to_owned()),
        window_id: Some(20),
        user_toolchains: Default::default(),
    };

    let workspace_3 = SerializedWorkspace {
        id: WorkspaceId(3),
        paths: PathList::new(&["/tmp3"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: Some("session-id-2".to_owned()),
        window_id: Some(30),
        user_toolchains: Default::default(),
    };

    let workspace_4 = SerializedWorkspace {
        id: WorkspaceId(4),
        paths: PathList::new(&["/tmp4"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    };

    let connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: "my-host".into(),
            port: Some(1234),
            ..Default::default()
        }))
        .await
        .unwrap();

    let workspace_5 = SerializedWorkspace {
        id: WorkspaceId(5),
        paths: PathList::default(),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Remote(db.remote_connection(connection_id).unwrap()),
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        session_id: Some("session-id-2".to_owned()),
        window_id: Some(50),
        user_toolchains: Default::default(),
    };

    let workspace_6 = SerializedWorkspace {
        id: WorkspaceId(6),
        paths: PathList::new(&["/tmp6c", "/tmp6b", "/tmp6a"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: Some("session-id-3".to_owned()),
        window_id: Some(60),
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace_1.clone()).await;
    thread::sleep(Duration::from_millis(1000)); // Force timestamps to increment
    db.save_workspace(workspace_2.clone()).await;
    db.save_workspace(workspace_3.clone()).await;
    thread::sleep(Duration::from_millis(1000)); // Force timestamps to increment
    db.save_workspace(workspace_4.clone()).await;
    db.save_workspace(workspace_5.clone()).await;
    db.save_workspace(workspace_6.clone()).await;

    let locations = db.session_workspaces("session-id-1".to_owned()).unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].0, WorkspaceId(2));
    assert_eq!(locations[0].1, PathList::new(&["/tmp2"]));
    assert_eq!(locations[0].2, Some(20));
    assert_eq!(locations[1].0, WorkspaceId(1));
    assert_eq!(locations[1].1, PathList::new(&["/tmp1"]));
    assert_eq!(locations[1].2, Some(10));

    let locations = db.session_workspaces("session-id-2".to_owned()).unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].0, WorkspaceId(5));
    assert_eq!(locations[0].1, PathList::default());
    assert_eq!(locations[0].2, Some(50));
    assert_eq!(locations[0].3, Some(connection_id));
    assert_eq!(locations[1].0, WorkspaceId(3));
    assert_eq!(locations[1].1, PathList::new(&["/tmp3"]));
    assert_eq!(locations[1].2, Some(30));

    let locations = db.session_workspaces("session-id-3".to_owned()).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].0, WorkspaceId(6));
    assert_eq!(
        locations[0].1,
        PathList::new(&["/tmp6c", "/tmp6b", "/tmp6a"]),
    );
    assert_eq!(locations[0].2, Some(60));
}

fn default_workspace<P: AsRef<Path>>(
    paths: &[P],
    center_group: &SerializedPaneGroup,
) -> SerializedWorkspace {
    SerializedWorkspace {
        id: WorkspaceId(4),
        paths: PathList::new(paths),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: center_group.clone(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    }
}

#[gpui::test]
async fn test_last_session_workspace_locations(cx: &mut gpui::TestAppContext) {
    let dir1 = tempfile::TempDir::with_prefix("dir1").unwrap();
    let dir2 = tempfile::TempDir::with_prefix("dir2").unwrap();
    let dir3 = tempfile::TempDir::with_prefix("dir3").unwrap();
    let dir4 = tempfile::TempDir::with_prefix("dir4").unwrap();

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree(dir1.path(), json!({})).await;
    fs.insert_tree(dir2.path(), json!({})).await;
    fs.insert_tree(dir3.path(), json!({})).await;
    fs.insert_tree(dir4.path(), json!({})).await;

    let db = WorkspaceDb::open_test_db("test_serializing_workspaces_last_session_workspaces").await;

    let workspaces = [
        (1, vec![dir1.path()], 9),
        (2, vec![dir2.path()], 5),
        (3, vec![dir3.path()], 8),
        (4, vec![dir4.path()], 2),
        (5, vec![dir1.path(), dir2.path(), dir3.path()], 3),
        (6, vec![dir4.path(), dir3.path(), dir2.path()], 4),
    ]
    .into_iter()
    .map(|(id, paths, window_id)| SerializedWorkspace {
        id: WorkspaceId(id),
        paths: PathList::new(paths.as_slice()),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
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
        WindowId::from(9),
        WindowId::from(3),
        WindowId::from(4), // Bottom
    ]));

    let locations = db
        .last_session_workspace_locations("one-session", stack, fs.as_ref())
        .await
        .unwrap();
    assert_eq!(
        locations,
        [
            SessionWorkspace {
                workspace_id: WorkspaceId(4),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir4.path()]),
                window_id: Some(WindowId::from(2u64)),
            },
            SessionWorkspace {
                workspace_id: WorkspaceId(3),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir3.path()]),
                window_id: Some(WindowId::from(8u64)),
            },
            SessionWorkspace {
                workspace_id: WorkspaceId(2),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir2.path()]),
                window_id: Some(WindowId::from(5u64)),
            },
            SessionWorkspace {
                workspace_id: WorkspaceId(1),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir1.path()]),
                window_id: Some(WindowId::from(9u64)),
            },
            SessionWorkspace {
                workspace_id: WorkspaceId(5),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir1.path(), dir2.path(), dir3.path()]),
                window_id: Some(WindowId::from(3u64)),
            },
            SessionWorkspace {
                workspace_id: WorkspaceId(6),
                location: SerializedWorkspaceLocation::Local,
                paths: PathList::new(&[dir4.path(), dir3.path(), dir2.path()]),
                window_id: Some(WindowId::from(4u64)),
            },
        ]
    );
}
