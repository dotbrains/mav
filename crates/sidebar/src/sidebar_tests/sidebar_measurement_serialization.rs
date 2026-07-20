use super::*;

#[gpui::test]
async fn test_thread_metadata_update_preserves_sticky_header_measurements(cx: &mut TestAppContext) {
    let (fs, project_a) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    add_test_project("/project-b", &fs, &multi_workspace, cx).await;

    save_thread_metadata(
        acp::SessionId::new(Arc::from("project-a-thread")),
        Some("Project A Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project_a,
        cx,
    );
    save_thread_metadata_with_main_paths(
        "project-b-thread",
        "Project B Thread",
        PathList::new(&[PathBuf::from("/project-b")]),
        PathList::new(&[PathBuf::from("/project-b")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        cx,
    );

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(400.), px(240.)),
        |_, _| sidebar.clone().into_any_element(),
    );
    cx.run_until_parked();

    let next_header_ix = sidebar.read_with(cx, |sidebar, _| {
        assert!(
            sidebar.contents.project_header_indices.len() == 2,
            "test setup should render exctly two project headers"
        );
        sidebar.contents.project_header_indices[1]
    });

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.list_state.scroll_to(gpui::ListOffset {
            item_ix: next_header_ix - 1,
            offset_in_item: px(24.),
        });
        cx.notify();
    });
    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(400.), px(240.)),
        |_, _| sidebar.clone().into_any_element(),
    );
    cx.run_until_parked();

    let bounds_before = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .list_state
            .bounds_for_item(next_header_ix)
            .expect("next project header should be measured before metadata update")
    });

    save_thread_metadata(
        acp::SessionId::new(Arc::from("project-a-thread")),
        Some("Renamed Project A Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 1, 0).unwrap(),
        None,
        None,
        &project_a,
        cx,
    );

    let bounds_after = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .list_state
            .bounds_for_item(next_header_ix)
            .expect("same-shape metadata update should preserve next header measurements")
    });
    assert_eq!(bounds_before, bounds_after);
}

#[gpui::test]
async fn test_thread_status_update_does_not_reset_list_measurements(cx: &mut TestAppContext) {
    // When a thread's status changes (e.g. Running -> Completed after sending a message), the
    // shape sequence is unchanged, so `update_entries` should not reset the underlying
    // `ListState`. Resetting throws away measured item bounds for one frame, which makes the
    // sticky project header flicker between its pushed-off and fully-on-screen positions.
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    cx.run_until_parked();

    let before = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    let after = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });

    assert_eq!(
        before, after,
        "a no-op rebuild should produce an identical shape sequence"
    );
}

#[gpui::test]
async fn test_collapse_changes_entry_shape(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    cx.run_until_parked();

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    let before = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();
    let after = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });

    assert_ne!(
        before, after,
        "collapsing the project group should change the shape sequence so the list resets"
    );
}

#[gpui::test]
async fn test_serialization_round_trip(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    // Set a custom width and collapse the group.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.set_width(Some(px(420.0)), cx);
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    // Capture the serialized state from the first sidebar.
    let serialized = sidebar.read_with(cx, |sidebar, cx| sidebar.serialized_state(cx));
    let serialized = serialized.expect("serialized_state should return Some");

    // Create a fresh sidebar and restore into it.
    let sidebar2 =
        cx.update(|window, cx| cx.new(|cx| Sidebar::new(multi_workspace.clone(), window, cx)));
    cx.run_until_parked();

    sidebar2.update_in(cx, |sidebar, window, cx| {
        sidebar.restore_serialized_state(&serialized, window, cx);
    });
    cx.run_until_parked();

    // Assert all serialized fields match.
    let width1 = sidebar.read_with(cx, |s, _| s.width);
    let width2 = sidebar2.read_with(cx, |s, _| s.width);

    assert_eq!(width1, width2);
    assert_eq!(width1, px(420.0));
}

#[gpui::test]
async fn test_restore_serialized_archive_view_does_not_panic(cx: &mut TestAppContext) {
    // A regression test to ensure that restoring a serialized archive view does not panic.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.update(|_window, cx| {
        AgentRegistryStore::init_test_global(cx, vec![]);
    });

    let serialized = serde_json::to_string(&SerializedSidebar {
        width: Some(400.0),
        active_view: SerializedSidebarView::History,
    })
    .expect("serialization should succeed");

    multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        if let Some(sidebar) = multi_workspace.sidebar() {
            sidebar.restore_serialized_state(&serialized, window, cx);
        }
    });
    cx.run_until_parked();

    // After the deferred `show_archive` runs, the view should be Archive.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(sidebar.view, SidebarView::Archive(_)),
            "expected sidebar view to be Archive after restore, got ThreadList"
        );
    });
}

#[gpui::test]
async fn test_entities_released_on_window_close(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let weak_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().downgrade());
    let weak_sidebar = sidebar.downgrade();
    let weak_multi_workspace = multi_workspace.downgrade();

    drop(sidebar);
    drop(multi_workspace);
    cx.update(|window, _cx| window.remove_window());
    cx.run_until_parked();

    weak_multi_workspace.assert_released();
    weak_sidebar.assert_released();
    weak_workspace.assert_released();
}

#[gpui::test]
async fn test_single_workspace_no_threads(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (_sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    assert_eq!(
        visible_entries_as_strings(&_sidebar, cx),
        vec!["v [my-project]"]
    );
}

#[gpui::test]
async fn test_single_workspace_with_saved_threads(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-1")),
        Some("Fix crash in project panel".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-2")),
        Some("Add inline diff view".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Fix crash in project panel",
            "  Add inline diff view",
        ]
    );
}

#[gpui::test]
async fn test_workspace_lifecycle(cx: &mut TestAppContext) {
    let project = init_test_project("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Single workspace with a thread
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-a1")),
        Some("Thread A1".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Thread A1",
        ]
    );

    // Add a second workspace
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.create_test_workspace(window, cx).detach();
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Thread A1",
        ]
    );
}

#[gpui::test]
async fn test_collapse_and_expand_group(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );

    // Collapse
    sidebar.update_in(cx, |s, window, cx| {
        s.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]",
        ]
    );

    // Expand
    sidebar.update_in(cx, |s, window, cx| {
        s.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );
}
