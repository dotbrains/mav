
use super::*;
use crate::OpenMode;
use crate::PathList;
use crate::ProjectGroupKey;
use crate::{
    multi_workspace::MultiWorkspace,
    persistence::{
        model::{
            SerializedItem, SerializedPane, SerializedPaneGroup, SerializedWorkspace,
            SessionWorkspace,
        },
        read_multi_workspace_state,
    },
};
use gpui::TaskExt;

use gpui::AppContext as _;
use pretty_assertions::assert_eq;
use project::Project;
use remote::SshConnectionOptions;
use serde_json::json;
use std::{thread, time::Duration};

/// Creates a unique directory in a FakeFs, returning the path.
/// Uses a UUID suffix to avoid collisions with other tests sharing the global DB.
async fn unique_test_dir(fs: &fs::FakeFs, prefix: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/test-dirs/{}-{}", prefix, uuid::Uuid::new_v4()));
    fs.insert_tree(&dir, json!({})).await;
    dir
}

#[gpui::test]
async fn test_multi_workspace_serializes_on_add_and_remove(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
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

    let window_id =
        multi_workspace.update_in(cx, |_, window, _cx| window.window_handle().window_id());

    // --- Add a second workspace ---
    let workspace2 = multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = cx.new(|cx| crate::Workspace::test_new(project2.clone(), window, cx));
        workspace.update(cx, |ws, _cx| ws.set_random_database_id());
        mw.activate(workspace.clone(), None, window, cx);
        workspace
    });

    // Run background tasks so serialize has a chance to flush.
    cx.run_until_parked();

    // Read back the persisted state and check that the active workspace ID was written.
    let state_after_add = cx.update(|_, cx| read_multi_workspace_state(window_id, cx));
    let active_workspace2_db_id = workspace2.read_with(cx, |ws, _| ws.database_id());
    assert_eq!(
        state_after_add.active_workspace_id, active_workspace2_db_id,
        "After adding a second workspace, the serialized active_workspace_id should match \
             the newly activated workspace's database id"
    );

    // --- Remove the non-active workspace ---
    multi_workspace.update_in(cx, |mw, _window, cx| {
        let active = mw.workspace().clone();
        let ws = mw
            .workspaces()
            .find(|ws| *ws != &active)
            .expect("should have a non-active workspace");
        mw.remove([ws.clone()], |_, _, _| unreachable!(), _window, cx)
            .detach_and_log_err(cx);
    });

    cx.run_until_parked();

    let state_after_remove = cx.update(|_, cx| read_multi_workspace_state(window_id, cx));
    let remaining_db_id =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).database_id());
    assert_eq!(
        state_after_remove.active_workspace_id, remaining_db_id,
        "After removing a workspace, the serialized active_workspace_id should match \
             the remaining active workspace's database id"
    );
}

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

#[gpui::test]
async fn test_get_or_create_ssh_project() {
    let db = WorkspaceDb::open_test_db("test_get_or_create_ssh_project").await;

    let host = "example.com".to_string();
    let port = Some(22_u16);
    let user = Some("user".to_string());

    let connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    // Test that calling the function again with the same parameters returns the same project
    let same_connection = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_eq!(connection_id, same_connection);

    // Test with different parameters
    let host2 = "otherexample.com".to_string();
    let port2 = None;
    let user2 = Some("otheruser".to_string());

    let different_connection = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host2.clone().into(),
            port: port2,
            username: user2.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_ne!(connection_id, different_connection);
}

#[gpui::test]
async fn test_get_or_create_ssh_project_with_null_user() {
    let db = WorkspaceDb::open_test_db("test_get_or_create_ssh_project_with_null_user").await;

    let (host, port, user) = ("example.com".to_string(), None, None);

    let connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: None,
            ..Default::default()
        }))
        .await
        .unwrap();

    let same_connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_eq!(connection_id, same_connection_id);
}

#[gpui::test]
async fn test_get_remote_connections() {
    let db = WorkspaceDb::open_test_db("test_get_remote_connections").await;

    let connections = [
        ("example.com".to_string(), None, None),
        (
            "anotherexample.com".to_string(),
            Some(123_u16),
            Some("user2".to_string()),
        ),
        ("yetanother.com".to_string(), Some(345_u16), None),
    ];

    let mut ids = Vec::new();
    for (host, port, user) in connections.iter() {
        ids.push(
            db.get_or_create_remote_connection(RemoteConnectionOptions::Ssh(
                SshConnectionOptions {
                    host: host.clone().into(),
                    port: *port,
                    username: user.clone(),
                    ..Default::default()
                },
            ))
            .await
            .unwrap(),
        );
    }

    let stored_connections = db.remote_connections().unwrap();
    assert_eq!(
        stored_connections,
        [
            (
                ids[0],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "example.com".into(),
                    port: None,
                    username: None,
                    ..Default::default()
                }),
            ),
            (
                ids[1],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "anotherexample.com".into(),
                    port: Some(123),
                    username: Some("user2".into()),
                    ..Default::default()
                }),
            ),
            (
                ids[2],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "yetanother.com".into(),
                    port: Some(345),
                    username: None,
                    ..Default::default()
                }),
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>(),
    );
}

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

#[gpui::test]
async fn test_recent_workspace_identity_deduplication(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());

    // Main repo with a linked worktree entry
    fs.insert_tree(
        "/repo",
        json!({
            ".git": {
                "worktrees": {
                    "feature": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // Linked worktree checkout pointing back to /repo
    fs.insert_tree(
        "/worktree",
        json!({
            ".git": "gitdir: /repo/.git/worktrees/feature",
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // A plain non-git project
    fs.insert_tree(
        "/plain-project",
        json!({
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // Another normal git repo (used in mixed-path entry)
    fs.insert_tree(
        "/other-repo",
        json!({
            ".git": {},
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now() - chrono::Duration::hours(4);
    let t1 = Utc::now() - chrono::Duration::hours(3);
    let t2 = Utc::now() - chrono::Duration::hours(2);
    let t3 = Utc::now() - chrono::Duration::hours(1);

    let workspaces = vec![
        local_recent_workspace(WorkspaceId(1), PathList::new(&["/repo"]), t0, fs.as_ref()).await,
        local_recent_workspace(
            WorkspaceId(2),
            PathList::new(&["/worktree"]),
            t1,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(3),
            PathList::new(&["/other-repo", "/worktree"]),
            t2,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(4),
            PathList::new(&["/plain-project"]),
            t3,
            fs.as_ref(),
        )
        .await,
    ];

    let result = dedupe_recent_workspaces(workspaces);

    // Should have 3 entries: #1 and #2 deduped into one, plus #3 and #4.
    assert_eq!(result.len(), 3);

    // First entry: /repo — deduplicated from #1 and #2.
    // Keeps the position of #1 (first seen), but with #2's later timestamp.
    assert_eq!(result[0].identity_paths.paths(), &[PathBuf::from("/repo")]);
    assert_eq!(result[0].timestamp, t1);

    // Second entry: mixed-path workspace with worktree resolved.
    // /worktree → /repo, so paths become [/other-repo, /repo] (sorted).
    assert_eq!(
        result[1].identity_paths.paths(),
        &[PathBuf::from("/other-repo"), PathBuf::from("/repo")]
    );
    assert_eq!(result[1].workspace_id, WorkspaceId(3));

    // Third entry: non-git project, unchanged.
    assert_eq!(
        result[2].identity_paths.paths(),
        &[PathBuf::from("/plain-project")]
    );
    assert_eq!(result[2].workspace_id, WorkspaceId(4));
}

#[gpui::test]
async fn test_recent_workspace_identity_for_bare_repo(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());

    // Bare repo at /foo/.bare (commondir doesn't end with .git)
    fs.insert_tree(
        "/foo/.bare",
        json!({
            "worktrees": {
                "my-feature": {
                    "commondir": "../../",
                    "HEAD": "ref: refs/heads/my-feature"
                }
            }
        }),
    )
    .await;

    // Linked worktree whose commondir resolves to a bare repo (/foo/.bare)
    fs.insert_tree(
        "/foo/my-feature",
        json!({
            ".git": "gitdir: /foo/.bare/worktrees/my-feature",
            "src": { "main.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now();

    let result = local_recent_workspace(
        WorkspaceId(1),
        PathList::new(&["/foo/my-feature"]),
        t0,
        fs.as_ref(),
    )
    .await;

    // Bare-backed worktrees should resolve to the repo identity path, which
    // is the parent directory users think of as the project root.
    assert_eq!(result.identity_paths.paths(), &[PathBuf::from("/foo")]);
}

#[gpui::test]
async fn test_recent_workspace_identity_deduplicates_main_and_linked_worktree(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());

    fs.insert_tree(
        "/the-project",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-project/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now() - chrono::Duration::hours(1);
    let t1 = Utc::now();
    let workspaces = vec![
        local_recent_workspace(
            WorkspaceId(1),
            PathList::new(&["/the-project"]),
            t0,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(2),
            PathList::new(&["/the-project/feature-a"]),
            t1,
            fs.as_ref(),
        )
        .await,
    ];

    let result = dedupe_recent_workspaces(workspaces);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].identity_paths.paths(),
        &[PathBuf::from("/the-project")]
    );
    assert_eq!(result[0].workspace_id, WorkspaceId(2));
    assert_eq!(result[0].timestamp, t1);
}

#[gpui::test]
async fn test_recent_project_workspaces_preserve_reopen_paths(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_recent_project_workspaces_preserve_reopen_paths").await;

    fs.insert_tree(
        "/the-project",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-project/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(workspace_with(
        1,
        &[Path::new("/the-project")],
        empty_pane_group(),
        None,
    ))
    .await;
    db.save_workspace(workspace_with(
        2,
        &[Path::new("/the-project/feature-a")],
        empty_pane_group(),
        None,
    ))
    .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].workspace_id, WorkspaceId(2));
    assert_eq!(
        recents[0].paths.paths(),
        &[PathBuf::from("/the-project/feature-a")]
    );
    assert_eq!(
        recents[0].identity_paths.paths(),
        &[PathBuf::from("/the-project")]
    );
}

#[gpui::test]
async fn test_recent_project_workspaces_remote_identity_hint(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_recent_project_workspaces_remote_identity_hint").await;

    let workspace = remote_workspace_with(1, "example.com", &[Path::new("/repo/feature-a")]);
    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/repo"])),
        ..workspace
    })
    .await;

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(
        recents[0].paths.paths(),
        &[PathBuf::from("/repo/feature-a")]
    );
    assert_eq!(recents[0].identity_paths.paths(), &[PathBuf::from("/repo")]);
}

#[gpui::test]
async fn test_recent_project_workspaces_remote_paths_do_not_use_local_fs_identity(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db(
        "test_recent_project_workspaces_remote_paths_do_not_use_local_fs_identity",
    )
    .await;

    fs.insert_tree(
        "/repo",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;
    fs.insert_tree(
        "/repo/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(remote_workspace_with(
        1,
        "example.com",
        &[Path::new("/repo/feature-a")],
    ))
    .await;

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(
        recents[0].identity_paths.paths(),
        &[PathBuf::from("/repo/feature-a")]
    );
}

#[gpui::test]
async fn test_recent_project_workspaces_do_not_dedupe_remote_hosts(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_recent_project_workspaces_do_not_dedupe_remote_hosts")
        .await;

    db.save_workspace(remote_workspace_with(1, "host-a", &[Path::new("/repo")]))
        .await;
    db.save_workspace(remote_workspace_with(2, "host-b", &[Path::new("/repo")]))
        .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 2);
    assert_eq!(recents[0].workspace_id, WorkspaceId(2));
    assert_eq!(recents[1].workspace_id, WorkspaceId(1));
}

#[gpui::test]
async fn test_delete_recent_workspace_group_removes_all_matching_rows(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_delete_recent_workspace_group_removes_all_matching_rows")
            .await;

    fs.insert_tree(
        "/the-group",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-group/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/the-group"])),
        ..workspace_with(1, &[Path::new("/the-group")], empty_pane_group(), None)
    })
    .await;
    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/the-group"])),
        ..workspace_with(
            2,
            &[Path::new("/the-group/feature-a")],
            empty_pane_group(),
            None,
        )
    })
    .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();
    assert_eq!(recents.len(), 1);

    let deleted = db.delete_recent_workspace_group(&recents[0]).await.unwrap();
    assert_eq!(deleted, vec![WorkspaceId(2), WorkspaceId(1)]);

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();
    assert!(recents.is_empty());
}

#[gpui::test]
async fn test_restore_window_with_linked_worktree_and_multiple_project_groups(
    cx: &mut gpui::TestAppContext,
) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());

    // Main git repo at /repo
    fs.insert_tree(
        "/repo",
        json!({
            ".git": {
                "HEAD": "ref: refs/heads/main",
                "worktrees": {
                    "feature": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // Linked worktree checkout pointing back to /repo
    fs.insert_tree(
        "/worktree-feature",
        json!({
            ".git": "gitdir: /repo/.git/worktrees/feature",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    // --- Phase 1: Set up the original multi-workspace window ---

    let project_1 = Project::test(fs.clone(), ["/repo".as_ref()], cx).await;
    let project_1_linked_worktree =
        Project::test(fs.clone(), ["/worktree-feature".as_ref()], cx).await;

    // Wait for git discovery to finish.
    cx.run_until_parked();

    // Create a second, unrelated project so we have two distinct project groups.
    fs.insert_tree(
        "/other-project",
        json!({
            ".git": { "HEAD": "ref: refs/heads/main" },
            "readme.md": ""
        }),
    )
    .await;
    let project_2 = Project::test(fs.clone(), ["/other-project".as_ref()], cx).await;
    cx.run_until_parked();

    // Create the MultiWorkspace with project_2, then add the main repo
    // and its linked worktree. The linked worktree is added last and
    // becomes the active workspace.
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_2.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_1.clone(), window, cx);
    });

    let workspace_worktree = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_1_linked_worktree.clone(), window, cx)
    });

    let tasks =
        multi_workspace.update_in(cx, |mw, window, cx| mw.flush_all_serialization(window, cx));
    cx.run_until_parked();
    for task in tasks {
        task.await;
    }
    cx.run_until_parked();

    let active_db_id = workspace_worktree.read_with(cx, |ws, _| ws.database_id());
    assert!(
        active_db_id.is_some(),
        "Active workspace should have a database ID"
    );

    // --- Phase 2: Read back and verify the serialized state ---

    let session_id = multi_workspace
        .read_with(cx, |mw, cx| mw.workspace().read(cx).session_id())
        .unwrap();
    let db = cx.update(|_, cx| WorkspaceDb::global(cx));
    let session_workspaces = db
        .last_session_workspace_locations(&session_id, None, fs.as_ref())
        .await
        .expect("should load session workspaces");
    assert!(
        !session_workspaces.is_empty(),
        "Should have at least one session workspace"
    );

    let multi_workspaces =
        cx.update(|_, cx| read_serialized_multi_workspaces(session_workspaces, cx));
    assert_eq!(
        multi_workspaces.len(),
        1,
        "All workspaces share one window, so there should be exactly one multi-workspace"
    );

    let serialized = &multi_workspaces[0];
    assert_eq!(
        serialized.active_workspace.workspace_id,
        active_db_id.unwrap(),
    );
    assert_eq!(serialized.state.project_groups.len(), 2,);

    // Verify the serialized project group keys round-trip back to the
    // originals.
    let restored_keys: Vec<ProjectGroupKey> = serialized
        .state
        .project_groups
        .iter()
        .cloned()
        .map(Into::into)
        .collect();
    let expected_keys = vec![
        ProjectGroupKey::new(None, PathList::new(&["/repo"])),
        ProjectGroupKey::new(None, PathList::new(&["/other-project"])),
    ];
    assert_eq!(
        restored_keys, expected_keys,
        "Deserialized project group keys should match the originals"
    );

    // --- Phase 3: Restore the window and verify the result ---

    let app_state =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).app_state().clone());

    let serialized_mw = multi_workspaces.into_iter().next().unwrap();
    let restored_handle: gpui::WindowHandle<MultiWorkspace> = cx
        .update(|_, cx| {
            cx.spawn(async move |mut cx| {
                crate::restore_multiworkspace(serialized_mw, app_state, &mut cx).await
            })
        })
        .await
        .expect("restore_multiworkspace should succeed");

    cx.run_until_parked();

    // The restored window should have the same project group keys.
    let restored_keys: Vec<ProjectGroupKey> = restored_handle
        .read_with(cx, |mw: &MultiWorkspace, _cx| mw.project_group_keys())
        .unwrap();
    assert_eq!(
        restored_keys, expected_keys,
        "Restored window should have the same project group keys as the original"
    );

    // The active workspace in the restored window should have the linked
    // worktree paths.
    let active_paths: Vec<PathBuf> = restored_handle
        .read_with(cx, |mw: &MultiWorkspace, cx| {
            mw.workspace()
                .read(cx)
                .root_paths(cx)
                .into_iter()
                .map(|p: Arc<Path>| p.to_path_buf())
                .collect()
        })
        .unwrap();
    assert_eq!(
        active_paths,
        vec![PathBuf::from("/worktree-feature")],
        "The restored active workspace should be the linked worktree project"
    );
}

#[gpui::test]
async fn test_remove_project_group_falls_back_to_neighbor(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir_a = unique_test_dir(&fs, "group-a").await;
    let dir_b = unique_test_dir(&fs, "group-b").await;
    let dir_c = unique_test_dir(&fs, "group-c").await;

    let project_a = Project::test(fs.clone(), [dir_a.as_path()], cx).await;
    let project_b = Project::test(fs.clone(), [dir_b.as_path()], cx).await;
    let project_c = Project::test(fs.clone(), [dir_c.as_path()], cx).await;

    // Create a multi-workspace with project A, then add B and C.
    // project_groups stores newest first: [C, B, A].
    // Sidebar displays in the same order: C (top), B (middle), A (bottom).
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| mw.open_sidebar(cx));

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _workspace_c = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_c.clone(), window, cx)
    });
    cx.run_until_parked();

    let key_a = project_a.read_with(cx, |p, cx| p.project_group_key(cx));
    let key_b = project_b.read_with(cx, |p, cx| p.project_group_key(cx));
    let key_c = project_c.read_with(cx, |p, cx| p.project_group_key(cx));

    // Activate workspace B so removing its group exercises the fallback.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // --- Remove group B (the middle one). ---
    // In the sidebar [C, B, A], "below" B is A.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_b, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert_eq!(
        active_paths
            .iter()
            .map(|p| p.to_path_buf())
            .collect::<Vec<_>>(),
        vec![dir_a.clone()],
        "After removing the middle group, should fall back to the group below (A)"
    );

    // After removing B, keys = [A, C], sidebar = [C, A].
    // Activate workspace A (the bottom) so removing it tests the
    // "fall back upward" path.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _cx| mw.workspaces().next().unwrap().clone());
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // --- Remove group A (the bottom one in sidebar). ---
    // Nothing below A, so should fall back upward to C.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_a, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert_eq!(
        active_paths
            .iter()
            .map(|p| p.to_path_buf())
            .collect::<Vec<_>>(),
        vec![dir_c.clone()],
        "After removing the bottom group, should fall back to the group above (C)"
    );

    // --- Remove group C (the only one remaining). ---
    // Should create an empty workspace.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_c, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert!(
        active_paths.is_empty(),
        "After removing the only remaining group, should have an empty workspace"
    );
}

/// Regression test for a crash where `find_or_create_local_workspace`
/// returned a workspace that was about to be removed, hitting an assert
/// in `MultiWorkspace::remove`.
///
/// The scenario: two workspaces share the same root paths (e.g. due to
/// a provisional key mismatch). When the first is removed and the
/// fallback searches for the same paths, `workspace_for_paths` must
/// skip the doomed workspace so the assert in `remove` is satisfied.
#[gpui::test]
async fn test_remove_fallback_skips_excluded_workspaces(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir = unique_test_dir(&fs, "shared").await;

    // Two projects that open the same directory — this creates two
    // workspaces whose root_paths are identical.
    let project_a = Project::test(fs.clone(), [dir.as_path()], cx).await;
    let project_b = Project::test(fs.clone(), [dir.as_path()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| mw.open_sidebar(cx));

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    cx.run_until_parked();

    // workspace_a is first in the workspaces vec.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().cloned().unwrap());
    assert_ne!(workspace_a, workspace_b);

    // Activate workspace_a so removing it triggers the fallback path.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // Remove workspace_a. The fallback searches for the same paths.
    // Without the `excluding` parameter, `workspace_for_paths` would
    // return workspace_a (first match) and the assert in `remove`
    // would fire. With the fix, workspace_a is skipped and
    // workspace_b is found instead.
    let path_list = PathList::new(std::slice::from_ref(&dir));
    let excluded = vec![workspace_a.clone()];
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove(
            vec![workspace_a.clone()],
            move |this, window, cx| {
                this.find_or_create_local_workspace(
                    path_list,
                    None,
                    &excluded,
                    None,
                    OpenMode::Activate,
                    window,
                    cx,
                )
            },
            window,
            cx,
        )
        .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    // workspace_b should now be active — workspace_a was removed.
    multi_workspace.read_with(cx, |mw, _cx| {
        assert_eq!(
            mw.workspace(),
            &workspace_b,
            "fallback should have found workspace_b, not the excluded workspace_a"
        );
    });
}
