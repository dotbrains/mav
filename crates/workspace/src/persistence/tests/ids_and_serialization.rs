use super::*;

#[gpui::test]
async fn test_next_id_stability() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_next_id_stability").await;

    db.write(|conn| {
        conn.migrate(
            "test_table",
            &[sql!(
                CREATE TABLE test_table(
                    text TEXT,
                    workspace_id INTEGER,
                    FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                    ON DELETE CASCADE
                ) STRICT;
            )],
            &mut |_, _, _| false,
        )
        .unwrap();
    })
    .await;

    let id = db.next_id().await.unwrap();
    // Assert the empty row got inserted
    assert_eq!(
        Some(id),
        db.select_row_bound::<WorkspaceId, WorkspaceId>(sql!(
            SELECT workspace_id FROM workspaces WHERE workspace_id = ?
        ))
        .unwrap()(id)
        .unwrap()
    );

    db.write(move |conn| {
        conn.exec_bound(sql!(INSERT INTO test_table(text, workspace_id) VALUES (?, ?)))
            .unwrap()(("test-text-1", id))
        .unwrap()
    })
    .await;

    let test_text_1 = db
        .select_row_bound::<_, String>(sql!(SELECT text FROM test_table WHERE workspace_id = ?))
        .unwrap()(1)
    .unwrap()
    .unwrap();
    assert_eq!(test_text_1, "test-text-1");
}

#[gpui::test]
async fn test_workspace_id_stability() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_workspace_id_stability").await;

    db.write(|conn| {
        conn.migrate(
            "test_table",
            &[sql!(
                        CREATE TABLE test_table(
                            text TEXT,
                            workspace_id INTEGER,
                            FOREIGN KEY(workspace_id)
                                REFERENCES workspaces(workspace_id)
                            ON DELETE CASCADE
                        ) STRICT;)],
            &mut |_, _, _| false,
        )
    })
    .await
    .unwrap();

    let mut workspace_1 = SerializedWorkspace {
        id: WorkspaceId(1),
        paths: PathList::new(&["/tmp", "/tmp2"]),
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

    let workspace_2 = SerializedWorkspace {
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
        window_id: None,
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace_1.clone()).await;

    db.write(|conn| {
        conn.exec_bound(sql!(INSERT INTO test_table(text, workspace_id) VALUES (?, ?)))
            .unwrap()(("test-text-1", 1))
        .unwrap();
    })
    .await;

    db.save_workspace(workspace_2.clone()).await;

    db.write(|conn| {
        conn.exec_bound(sql!(INSERT INTO test_table(text, workspace_id) VALUES (?, ?)))
            .unwrap()(("test-text-2", 2))
        .unwrap();
    })
    .await;

    workspace_1.paths = PathList::new(&["/tmp", "/tmp3"]);
    db.save_workspace(workspace_1.clone()).await;
    db.save_workspace(workspace_1).await;
    db.save_workspace(workspace_2).await;

    let test_text_2 = db
        .select_row_bound::<_, String>(sql!(SELECT text FROM test_table WHERE workspace_id = ?))
        .unwrap()(2)
    .unwrap()
    .unwrap();
    assert_eq!(test_text_2, "test-text-2");

    let test_text_1 = db
        .select_row_bound::<_, String>(sql!(SELECT text FROM test_table WHERE workspace_id = ?))
        .unwrap()(1)
    .unwrap()
    .unwrap();
    assert_eq!(test_text_1, "test-text-1");
}

fn group(axis: Axis, children: Vec<SerializedPaneGroup>) -> SerializedPaneGroup {
    SerializedPaneGroup::Group {
        axis: SerializedAxis(axis),
        flexes: None,
        children,
    }
}

#[gpui::test]
async fn test_full_workspace_serialization() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_full_workspace_serialization").await;

    //  -----------------
    //  | 1,2   | 5,6   |
    //  | - - - |       |
    //  | 3,4   |       |
    //  -----------------
    let center_group = group(
        Axis::Horizontal,
        vec![
            group(
                Axis::Vertical,
                vec![
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 5, false, false),
                            SerializedItem::new("Terminal", 6, true, false),
                        ],
                        false,
                        0,
                    )),
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 7, true, false),
                            SerializedItem::new("Terminal", 8, false, false),
                        ],
                        false,
                        0,
                    )),
                ],
            ),
            SerializedPaneGroup::Pane(SerializedPane::new(
                vec![
                    SerializedItem::new("Terminal", 9, false, false),
                    SerializedItem::new("Terminal", 10, true, false),
                ],
                false,
                0,
            )),
        ],
    );

    let workspace = SerializedWorkspace {
        id: WorkspaceId(5),
        paths: PathList::new(&["/tmp", "/tmp2"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group,
        window_bounds: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: Some(999),
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace.clone()).await;

    let round_trip_workspace = db.workspace_for_roots(&["/tmp2", "/tmp"]);
    assert_eq!(workspace, round_trip_workspace.unwrap());

    // Test guaranteed duplicate IDs
    db.save_workspace(workspace.clone()).await;
    db.save_workspace(workspace.clone()).await;

    let round_trip_workspace = db.workspace_for_roots(&["/tmp", "/tmp2"]);
    assert_eq!(workspace, round_trip_workspace.unwrap());
}
