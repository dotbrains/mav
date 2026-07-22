use super::*;

#[gpui::test]
async fn test_simple_split() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("simple_split").await;

    //  -----------------
    //  | 1,2   | 5,6   |
    //  | - - - |       |
    //  | 3,4   |       |
    //  -----------------
    let center_pane = group(
        Axis::Horizontal,
        vec![
            group(
                Axis::Vertical,
                vec![
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 1, false, false),
                            SerializedItem::new("Terminal", 2, true, false),
                        ],
                        false,
                        0,
                    )),
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 4, false, false),
                            SerializedItem::new("Terminal", 3, true, false),
                        ],
                        true,
                        0,
                    )),
                ],
            ),
            SerializedPaneGroup::Pane(SerializedPane::new(
                vec![
                    SerializedItem::new("Terminal", 5, true, false),
                    SerializedItem::new("Terminal", 6, false, false),
                ],
                false,
                0,
            )),
        ],
    );

    let workspace = default_workspace(&["/tmp"], &center_pane);

    db.save_workspace(workspace.clone()).await;

    let new_workspace = db.workspace_for_roots(&["/tmp"]).unwrap();

    assert_eq!(workspace.center_group, new_workspace.center_group);
}

#[gpui::test]
async fn test_cleanup_panes() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_cleanup_panes").await;

    let center_pane = group(
        Axis::Horizontal,
        vec![
            group(
                Axis::Vertical,
                vec![
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 1, false, false),
                            SerializedItem::new("Terminal", 2, true, false),
                        ],
                        false,
                        0,
                    )),
                    SerializedPaneGroup::Pane(SerializedPane::new(
                        vec![
                            SerializedItem::new("Terminal", 4, false, false),
                            SerializedItem::new("Terminal", 3, true, false),
                        ],
                        true,
                        0,
                    )),
                ],
            ),
            SerializedPaneGroup::Pane(SerializedPane::new(
                vec![
                    SerializedItem::new("Terminal", 5, false, false),
                    SerializedItem::new("Terminal", 6, true, false),
                ],
                false,
                0,
            )),
        ],
    );

    let id = &["/tmp"];

    let mut workspace = default_workspace(id, &center_pane);

    db.save_workspace(workspace.clone()).await;

    workspace.center_group = group(
        Axis::Vertical,
        vec![
            SerializedPaneGroup::Pane(SerializedPane::new(
                vec![
                    SerializedItem::new("Terminal", 1, false, false),
                    SerializedItem::new("Terminal", 2, true, false),
                ],
                false,
                0,
            )),
            SerializedPaneGroup::Pane(SerializedPane::new(
                vec![
                    SerializedItem::new("Terminal", 4, true, false),
                    SerializedItem::new("Terminal", 3, false, false),
                ],
                true,
                0,
            )),
        ],
    );

    db.save_workspace(workspace.clone()).await;

    let new_workspace = db.workspace_for_roots(id).unwrap();

    assert_eq!(workspace.center_group, new_workspace.center_group);
}

#[gpui::test]
async fn test_empty_workspace_window_bounds() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_empty_workspace_window_bounds").await;
    let id = db.next_id().await.unwrap();

    // Create a workspace with empty paths (empty workspace)
    let empty_paths: &[&str] = &[];
    let display_uuid = Uuid::new_v4();
    let window_bounds = SerializedWindowBounds(WindowBounds::Windowed(Bounds {
        origin: point(px(100.0), px(200.0)),
        size: size(px(800.0), px(600.0)),
    }));

    let workspace = SerializedWorkspace {
        id,
        paths: PathList::new(empty_paths),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: None,
        display: None,
        docks: Default::default(),
        bookmarks: Default::default(),
        breakpoints: Default::default(),
        centered_layout: false,
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    };

    // Save the workspace (this creates the record with empty paths)
    db.save_workspace(workspace.clone()).await;

    // Save window bounds separately (as the actual code does via set_window_open_status)
    db.set_window_open_status(id, window_bounds, display_uuid)
        .await
        .unwrap();

    // Empty workspaces cannot be retrieved by paths (they'd all match).
    // They must be retrieved by workspace_id.
    assert!(db.workspace_for_roots(empty_paths).is_none());

    // Retrieve using workspace_for_id instead
    let retrieved = db.workspace_for_id(id).unwrap();

    // Verify window bounds were persisted
    assert_eq!(retrieved.id, id);
    assert!(retrieved.window_bounds.is_some());
    assert_eq!(retrieved.window_bounds.unwrap().0, window_bounds.0);
    assert!(retrieved.display.is_some());
    assert_eq!(retrieved.display.unwrap(), display_uuid);
}
