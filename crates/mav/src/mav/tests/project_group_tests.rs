use super::*;

#[gpui::test]
async fn test_quit_preserves_focused_workspace_for_restore(cx: &mut TestAppContext) {
    use session::Session;
    use workspace::{OpenMode, Workspace};

    let app_state = init_test(cx);
    cx.update(init);

    let dir1 = path!("/dir1");
    let dir2 = path!("/dir2");

    let fs = app_state.fs.clone();
    let fake_fs = fs.as_fake();
    fake_fs.insert_tree(dir1, json!({})).await;
    fake_fs.insert_tree(dir2, json!({})).await;

    let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

    // Window with two retained workspaces: dir1 added first, dir2 second.
    let workspace::OpenResult { window, .. } = cx
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

    window
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.open_project(vec![dir2.into()], OpenMode::Activate, window, cx)
        })
        .unwrap()
        .await
        .expect("failed to open second workspace");
    cx.run_until_parked();

    // Focus dir1 (the first workspace). dir2 was activated last when it was
    // opened and is iterated last by the quit-time close-prompt loop, so
    // without the fix the persisted active workspace gets clobbered to dir2.
    window
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspaces().next().unwrap().clone();
            multi_workspace.activate(workspace, None, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .read_with(cx, |mw, cx| {
            assert!(
                mw.workspace()
                    .read(cx)
                    .root_paths(cx)
                    .iter()
                    .any(|p| p.as_ref() == Path::new(dir1)),
                "dir1 should be the focused workspace before quitting"
            );
        })
        .unwrap();

    // Quit. With no dirty items there are no save prompts, so the quit flow
    // runs the prepare_to_close loop (which activates every workspace in
    // turn to surface prompts) and then flushes serialization. cx.quit() is
    // a no-op in tests, so the window stays around for inspection.
    cx.dispatch_action(*window, Quit);
    cx.run_until_parked();

    // The fix re-activates the originally-focused workspace after the loop,
    // so the window must still be focused on dir1, not dir2.
    window
        .read_with(cx, |mw, cx| {
            let active = mw.workspace().read(cx).root_paths(cx);
            assert!(
                active.iter().any(|p| p.as_ref() == Path::new(dir1)),
                "quitting must not change which workspace is focused"
            );
            assert!(
                !active.iter().any(|p| p.as_ref() == Path::new(dir2)),
                "dir2 must not become the focused workspace after quitting"
            );
        })
        .unwrap();

    // Simulate a fresh launch and verify dir1 is restored as the active
    // workspace rather than dir2 (or an empty window).
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        app_state.session.update(cx, |app_session, _cx| {
            app_session
                .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
        });
    });

    let mut async_cx = cx.to_async();
    crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
        .await
        .expect("failed to restore workspaces");
    cx.run_until_parked();

    let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<MultiWorkspace>())
            .collect()
    });
    assert_eq!(restored_windows.len(), 1);

    restored_windows[0]
        .read_with(cx, |mw, cx| {
            let active = mw.workspace().read(cx).root_paths(cx);
            assert!(
                active.iter().any(|p| p.as_ref() == Path::new(dir1)),
                "the focused workspace (dir1) must be restored as active"
            );
            assert!(
                !active.iter().any(|p| p.as_ref() == Path::new(dir2)),
                "dir2 must not be restored as the active workspace"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_restored_project_groups_survive_workspace_key_change(cx: &mut TestAppContext) {
    use session::Session;
    use util::path_list::PathList;
    use workspace::{OpenMode, ProjectGroupKey};

    let app_state = init_test(cx);

    let fs = app_state.fs.clone();
    let fake_fs = fs.as_fake();
    fake_fs
        .insert_tree(path!("/root_a"), json!({ "file.txt": "" }))
        .await;
    fake_fs
        .insert_tree(path!("/root_b"), json!({ "file.txt": "" }))
        .await;
    fake_fs
        .insert_tree(path!("/root_c"), json!({ "file.txt": "" }))
        .await;
    fake_fs
        .insert_tree(path!("/root_d"), json!({ "other.txt": "" }))
        .await;

    let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());

    // --- Phase 1: Build a multi-workspace with 3 project groups ---

    let workspace::OpenResult { window, .. } = cx
        .update(|cx| {
            workspace::Workspace::new_local(
                vec![path!("/root_a").into()],
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .expect("failed to open workspace");

    window.update(cx, |mw, _, cx| mw.open_sidebar(cx)).unwrap();

    window
        .update(cx, |mw, window, cx| {
            mw.open_project(vec![path!("/root_b").into()], OpenMode::Add, window, cx)
        })
        .unwrap()
        .await
        .expect("failed to add root_b");

    window
        .update(cx, |mw, window, cx| {
            mw.open_project(vec![path!("/root_c").into()], OpenMode::Add, window, cx)
        })
        .unwrap()
        .await
        .expect("failed to add root_c");
    cx.run_until_parked();

    let key_b = ProjectGroupKey::new(None, PathList::new(&[path!("/root_b")]));
    let key_c = ProjectGroupKey::new(None, PathList::new(&[path!("/root_c")]));

    // Make root_a the active workspace so it's the one eagerly restored.
    window
        .update(cx, |mw, window, cx| {
            let workspace_a = mw
                .workspaces()
                .find(|ws| {
                    ws.read(cx)
                        .root_paths(cx)
                        .iter()
                        .any(|p| p.as_ref() == Path::new(path!("/root_a")))
                })
                .expect("workspace_a should exist")
                .clone();
            mw.activate(workspace_a, None, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // --- Phase 2: Serialize, close, and restore ---

    flush_workspace_serialization(&window, cx).await;
    cx.run_until_parked();

    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        app_state.session.update(cx, |app_session, _cx| {
            app_session
                .replace_session_for_test(Session::test_with_old_session(session_id.clone()));
        });
    });

    let mut async_cx = cx.to_async();
    crate::restore_or_create_workspace(app_state.clone(), &mut async_cx)
        .await
        .expect("failed to restore workspace");
    cx.run_until_parked();

    let restored_windows: Vec<WindowHandle<MultiWorkspace>> = cx.read(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|w| w.downcast::<MultiWorkspace>())
            .collect()
    });
    assert_eq!(restored_windows.len(), 1);
    let restored = &restored_windows[0];

    // Verify the restored window has all 3 project groups.
    restored
        .read_with(cx, |mw, _cx| {
            let keys = mw.project_group_keys();
            assert_eq!(
                keys.len(),
                3,
                "restored window should have 3 groups; got {keys:?}"
            );
            assert!(keys.contains(&key_b), "should contain key_b");
            assert!(keys.contains(&key_c), "should contain key_c");
        })
        .unwrap();

    // --- Phase 3: Trigger a workspace key change and verify survival ---

    let active_project = restored
        .read_with(cx, |mw, cx| mw.workspace().read(cx).project().clone())
        .unwrap();

    active_project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/root_d"), true, cx)
        })
        .await
        .expect("adding worktree should succeed");
    cx.run_until_parked();

    restored
        .read_with(cx, |mw, _cx| {
            let keys = mw.project_group_keys();
            assert!(
                keys.contains(&key_b),
                "restored group key_b should survive a workspace key change; got {keys:?}"
            );
            assert!(
                keys.contains(&key_c),
                "restored group key_c should survive a workspace key change; got {keys:?}"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_close_project_removes_project_group(cx: &mut TestAppContext) {
    use util::path_list::PathList;
    use workspace::{OpenMode, ProjectGroupKey};

    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/my-project"), json!({}))
        .await;

    let workspace::OpenResult { window, .. } = cx
        .update(|cx| {
            workspace::Workspace::new_local(
                vec![path!("/my-project").into()],
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .unwrap();

    window.update(cx, |mw, _, cx| mw.open_sidebar(cx)).unwrap();
    cx.background_executor.run_until_parked();

    let project_key = ProjectGroupKey::new(None, PathList::new(&[path!("/my-project")]));
    let keys = window
        .read_with(cx, |mw, _| mw.project_group_keys())
        .unwrap();
    assert_eq!(
        keys,
        vec![project_key],
        "project group should exist before CloseProject: {keys:?}"
    );

    cx.dispatch_action(window.into(), CloseProject);

    let keys = window
        .read_with(cx, |mw, _| mw.project_group_keys())
        .unwrap();
    assert!(
        keys.is_empty(),
        "project group should be removed after CloseProject: {keys:?}"
    );
}
