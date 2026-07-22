use super::*;

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
