use super::*;

#[gpui::test]
async fn test_multi_workspace_session_restore(cx: &mut TestAppContext) {
    use collections::HashMap;
    use session::Session;
    use util::path_list::PathList;
    use workspace::{OpenMode, ProjectGroupKey, Workspace, WorkspaceId};

    let app_state = init_test(cx);

    let dir1 = path!("/dir1");
    let dir2 = path!("/dir2");
    let dir3 = path!("/dir3");

    let fs = app_state.fs.clone();
    let fake_fs = fs.as_fake();
    fake_fs.insert_tree(dir1, json!({})).await;
    fake_fs.insert_tree(dir2, json!({})).await;
    fake_fs.insert_tree(dir3, json!({})).await;

    let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

    // --- Create 3 workspaces in 2 windows ---
    //
    //   Window A: workspace for dir1, workspace for dir2
    //   Window B: workspace for dir3
    let workspace::OpenResult {
        window: window_a, ..
    } = cx
        .update(|cx| {
            Workspace::new_local(
                vec![dir1.into()],
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .expect("failed to open first workspace");

    window_a
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    window_a
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.open_project(vec![dir2.into()], OpenMode::Activate, window, cx)
        })
        .unwrap()
        .await
        .expect("failed to open second workspace into window A");
    cx.run_until_parked();

    let workspace::OpenResult {
        window: window_b, ..
    } = cx
        .update(|cx| {
            Workspace::new_local(
                vec![dir3.into()],
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .expect("failed to open third workspace");

    window_b
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    // Currently dir2 is active because it was added last.
    // So, switch window_a's active workspace to dir1 (index 0).
    // This sets up a non-trivial assertion: after restore, dir1 should
    // still be active rather than whichever workspace happened to restore last.
    window_a
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspaces().next().unwrap().clone();
            multi_workspace.activate(workspace, None, window, cx);
        })
        .unwrap();

    cx.run_until_parked();
    flush_workspace_serialization(&window_a, cx).await;
    flush_workspace_serialization(&window_b, cx).await;
    cx.run_until_parked();

    // Verify all workspaces retained their session_ids.
    let db = cx.update(|cx| workspace::WorkspaceDb::global(cx));
    let locations =
        workspace::last_session_workspace_locations(&db, &session_id, None, fs.as_ref())
            .await
            .expect("expected session workspace locations");
    assert_eq!(
        locations.len(),
        3,
        "all 3 workspaces should have session_ids in the DB"
    );

    // Close the original windows.
    window_a
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    window_b
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();

    // Simulate a new session launch: replace the session so that
    // `last_session_id()` returns the ID used during workspace creation.
    // `restore_on_startup` defaults to `LastSession`, which is what we need.
    cx.update(|cx| {
        app_state.session.update(cx, |app_session, _cx| {
            app_session
                .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
        });
    });

    // --- Read back from DB and verify grouping ---
    let locations =
        workspace::last_session_workspace_locations(&db, &session_id, None, fs.as_ref())
            .await
            .expect("expected session workspace locations");

    assert_eq!(locations.len(), 3, "expected 3 session workspaces");

    let mut groups_by_window: HashMap<gpui::WindowId, Vec<WorkspaceId>> = HashMap::default();
    for session_workspace in &locations {
        if let Some(window_id) = session_workspace.window_id {
            groups_by_window
                .entry(window_id)
                .or_default()
                .push(session_workspace.workspace_id);
        }
    }
    assert_eq!(
        groups_by_window.len(),
        2,
        "expected 2 window groups, got {groups_by_window:?}"
    );
    assert!(
        groups_by_window.values().any(|g| g.len() == 2),
        "expected one group with 2 workspaces"
    );
    assert!(
        groups_by_window.values().any(|g| g.len() == 1),
        "expected one group with 1 workspace"
    );

    let mut async_cx = cx.to_async();
    crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
        .await
        .expect("failed to restore workspaces");
    cx.run_until_parked();

    // --- Verify the restored windows ---
    let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<MultiWorkspace>())
            .collect()
    });
    assert_eq!(restored_windows.len(), 2,);

    // Identify restored windows by their active workspace root paths.
    let (restored_a, restored_b) = {
        let (mut with_dir1, mut with_dir3) = (None, None);
        for window in &restored_windows {
            let active_paths = window
                .read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx))
                .unwrap();
            if active_paths.iter().any(|p| p.as_ref() == Path::new(dir1)) {
                with_dir1 = Some(window);
            } else {
                with_dir3 = Some(window);
            }
        }
        (
            with_dir1.expect("expected a window with dir1 active"),
            with_dir3.expect("expected a window with dir3 active"),
        )
    };

    // Window A (dir1+dir2): 1 workspace restored, but 2 project group keys.
    restored_a
        .read_with(cx, |mw, _| {
            assert_eq!(
                mw.project_group_keys(),
                vec![
                    ProjectGroupKey::new(None, PathList::new(&[dir2])),
                    ProjectGroupKey::new(None, PathList::new(&[dir1])),
                ]
            );
            assert_eq!(mw.workspaces().count(), 1);
        })
        .unwrap();

    // Window B (dir3): 1 workspace, 1 project group key.
    restored_b
        .read_with(cx, |mw, _| {
            assert_eq!(
                mw.project_group_keys(),
                vec![ProjectGroupKey::new(None, PathList::new(&[dir3]))]
            );
            assert_eq!(mw.workspaces().count(), 1);
        })
        .unwrap();
}
