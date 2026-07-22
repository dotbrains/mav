use super::*;

#[gpui::test]
async fn test_breakpoints() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_breakpoints").await;
    let id = db.next_id().await.unwrap();

    let path = Path::new("/tmp/test.rs");

    let breakpoint = Breakpoint {
        position: 123,
        message: None,
        state: BreakpointState::Enabled,
        condition: None,
        hit_condition: None,
    };

    let log_breakpoint = Breakpoint {
        position: 456,
        message: Some("Test log message".into()),
        state: BreakpointState::Enabled,
        condition: None,
        hit_condition: None,
    };

    let disable_breakpoint = Breakpoint {
        position: 578,
        message: None,
        state: BreakpointState::Disabled,
        condition: None,
        hit_condition: None,
    };

    let condition_breakpoint = Breakpoint {
        position: 789,
        message: None,
        state: BreakpointState::Enabled,
        condition: Some("x > 5".into()),
        hit_condition: None,
    };

    let hit_condition_breakpoint = Breakpoint {
        position: 999,
        message: None,
        state: BreakpointState::Enabled,
        condition: None,
        hit_condition: Some(">= 3".into()),
    };

    let workspace = SerializedWorkspace {
        id,
        paths: PathList::new(&["/tmp"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: {
            let mut map = collections::BTreeMap::default();
            map.insert(
                Arc::from(path),
                vec![
                    SourceBreakpoint {
                        row: breakpoint.position,
                        path: Arc::from(path),
                        message: breakpoint.message.clone(),
                        state: breakpoint.state,
                        condition: breakpoint.condition.clone(),
                        hit_condition: breakpoint.hit_condition.clone(),
                    },
                    SourceBreakpoint {
                        row: log_breakpoint.position,
                        path: Arc::from(path),
                        message: log_breakpoint.message.clone(),
                        state: log_breakpoint.state,
                        condition: log_breakpoint.condition.clone(),
                        hit_condition: log_breakpoint.hit_condition.clone(),
                    },
                    SourceBreakpoint {
                        row: disable_breakpoint.position,
                        path: Arc::from(path),
                        message: disable_breakpoint.message.clone(),
                        state: disable_breakpoint.state,
                        condition: disable_breakpoint.condition.clone(),
                        hit_condition: disable_breakpoint.hit_condition.clone(),
                    },
                    SourceBreakpoint {
                        row: condition_breakpoint.position,
                        path: Arc::from(path),
                        message: condition_breakpoint.message.clone(),
                        state: condition_breakpoint.state,
                        condition: condition_breakpoint.condition.clone(),
                        hit_condition: condition_breakpoint.hit_condition.clone(),
                    },
                    SourceBreakpoint {
                        row: hit_condition_breakpoint.position,
                        path: Arc::from(path),
                        message: hit_condition_breakpoint.message.clone(),
                        state: hit_condition_breakpoint.state,
                        condition: hit_condition_breakpoint.condition.clone(),
                        hit_condition: hit_condition_breakpoint.hit_condition.clone(),
                    },
                ],
            );
            map
        },
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace.clone()).await;

    let loaded = db.workspace_for_roots(&["/tmp"]).unwrap();
    let loaded_breakpoints = loaded.breakpoints.get(&Arc::from(path)).unwrap();

    assert_eq!(loaded_breakpoints.len(), 5);

    // normal breakpoint
    assert_eq!(loaded_breakpoints[0].row, breakpoint.position);
    assert_eq!(loaded_breakpoints[0].message, breakpoint.message);
    assert_eq!(loaded_breakpoints[0].condition, breakpoint.condition);
    assert_eq!(
        loaded_breakpoints[0].hit_condition,
        breakpoint.hit_condition
    );
    assert_eq!(loaded_breakpoints[0].state, breakpoint.state);
    assert_eq!(loaded_breakpoints[0].path, Arc::from(path));

    // enabled breakpoint
    assert_eq!(loaded_breakpoints[1].row, log_breakpoint.position);
    assert_eq!(loaded_breakpoints[1].message, log_breakpoint.message);
    assert_eq!(loaded_breakpoints[1].condition, log_breakpoint.condition);
    assert_eq!(
        loaded_breakpoints[1].hit_condition,
        log_breakpoint.hit_condition
    );
    assert_eq!(loaded_breakpoints[1].state, log_breakpoint.state);
    assert_eq!(loaded_breakpoints[1].path, Arc::from(path));

    // disable breakpoint
    assert_eq!(loaded_breakpoints[2].row, disable_breakpoint.position);
    assert_eq!(loaded_breakpoints[2].message, disable_breakpoint.message);
    assert_eq!(
        loaded_breakpoints[2].condition,
        disable_breakpoint.condition
    );
    assert_eq!(
        loaded_breakpoints[2].hit_condition,
        disable_breakpoint.hit_condition
    );
    assert_eq!(loaded_breakpoints[2].state, disable_breakpoint.state);
    assert_eq!(loaded_breakpoints[2].path, Arc::from(path));

    // condition breakpoint
    assert_eq!(loaded_breakpoints[3].row, condition_breakpoint.position);
    assert_eq!(loaded_breakpoints[3].message, condition_breakpoint.message);
    assert_eq!(
        loaded_breakpoints[3].condition,
        condition_breakpoint.condition
    );
    assert_eq!(
        loaded_breakpoints[3].hit_condition,
        condition_breakpoint.hit_condition
    );
    assert_eq!(loaded_breakpoints[3].state, condition_breakpoint.state);
    assert_eq!(loaded_breakpoints[3].path, Arc::from(path));

    // hit condition breakpoint
    assert_eq!(loaded_breakpoints[4].row, hit_condition_breakpoint.position);
    assert_eq!(
        loaded_breakpoints[4].message,
        hit_condition_breakpoint.message
    );
    assert_eq!(
        loaded_breakpoints[4].condition,
        hit_condition_breakpoint.condition
    );
    assert_eq!(
        loaded_breakpoints[4].hit_condition,
        hit_condition_breakpoint.hit_condition
    );
    assert_eq!(loaded_breakpoints[4].state, hit_condition_breakpoint.state);
    assert_eq!(loaded_breakpoints[4].path, Arc::from(path));
}

#[gpui::test]
async fn test_remove_last_breakpoint() {
    zlog::init_test();

    let db = WorkspaceDb::open_test_db("test_remove_last_breakpoint").await;
    let id = db.next_id().await.unwrap();

    let singular_path = Path::new("/tmp/test_remove_last_breakpoint.rs");

    let breakpoint_to_remove = Breakpoint {
        position: 100,
        message: None,
        state: BreakpointState::Enabled,
        condition: None,
        hit_condition: None,
    };

    let workspace = SerializedWorkspace {
        id,
        paths: PathList::new(&["/tmp"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: {
            let mut map = collections::BTreeMap::default();
            map.insert(
                Arc::from(singular_path),
                vec![SourceBreakpoint {
                    row: breakpoint_to_remove.position,
                    path: Arc::from(singular_path),
                    message: None,
                    state: BreakpointState::Enabled,
                    condition: None,
                    hit_condition: None,
                }],
            );
            map
        },
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace.clone()).await;

    let loaded = db.workspace_for_roots(&["/tmp"]).unwrap();
    let loaded_breakpoints = loaded.breakpoints.get(&Arc::from(singular_path)).unwrap();

    assert_eq!(loaded_breakpoints.len(), 1);
    assert_eq!(loaded_breakpoints[0].row, breakpoint_to_remove.position);
    assert_eq!(loaded_breakpoints[0].message, breakpoint_to_remove.message);
    assert_eq!(
        loaded_breakpoints[0].condition,
        breakpoint_to_remove.condition
    );
    assert_eq!(
        loaded_breakpoints[0].hit_condition,
        breakpoint_to_remove.hit_condition
    );
    assert_eq!(loaded_breakpoints[0].state, breakpoint_to_remove.state);
    assert_eq!(loaded_breakpoints[0].path, Arc::from(singular_path));

    let workspace_without_breakpoint = SerializedWorkspace {
        id,
        paths: PathList::new(&["/tmp"]),
        identity_paths: None,
        location: SerializedWorkspaceLocation::Local,
        center_group: Default::default(),
        window_bounds: Default::default(),
        display: Default::default(),
        docks: Default::default(),
        centered_layout: false,
        bookmarks: Default::default(),
        breakpoints: collections::BTreeMap::default(),
        session_id: None,
        window_id: None,
        user_toolchains: Default::default(),
    };

    db.save_workspace(workspace_without_breakpoint.clone())
        .await;

    let loaded_after_remove = db.workspace_for_roots(&["/tmp"]).unwrap();
    let empty_breakpoints = loaded_after_remove
        .breakpoints
        .get(&Arc::from(singular_path));

    assert!(empty_breakpoints.is_none());
}
