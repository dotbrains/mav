use super::*;

#[gpui::test]
async fn test_keyboard_expand_and_collapse_selected_entry(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
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

    // Focus sidebar and manually select the header (index 0). Press left to collapse.
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );

    // Press right to expand
    cx.dispatch_action(SelectChild);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]  <== selected",
            "  Thread 1",
        ]
    );

    // Press right again on already-expanded header moves selection down
    cx.dispatch_action(SelectChild);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));
}

#[gpui::test]
async fn test_keyboard_collapse_from_child_selects_parent(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Focus sidebar (selection starts at None), then navigate down to the thread (child)
    focus_sidebar(&sidebar, cx);
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1  <== selected",
        ]
    );

    // Pressing left on a child collapses the parent group and selects it
    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );
}

#[gpui::test]
async fn test_keyboard_navigation_on_empty_list(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/empty-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // An empty project has only the header (no auto-created draft).
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [empty-project]"]
    );

    // Focus sidebar — focus_in does not set a selection
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // First SelectNext from None starts at index 0 (header)
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // SelectNext with only one entry stays at index 0
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // SelectPrevious from first entry clears selection (returns to editor)
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // SelectPrevious from None selects the last entry
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_new_entry_noops_without_open_project(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));
    let project = project::Project::test(fs, [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let workspace = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });

    assert!(
        !sidebar.read_with(cx, |sidebar, _cx| sidebar.contents.has_open_projects),
        "empty workspaces should be treated as having no open projects"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_entry(&workspace, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(
            panel.active_conversation_view().is_none(),
            "sidebar should not create an agent thread without an open project"
        );
    });
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        Vec::<String>::new()
    );
}

#[gpui::test]
async fn test_selection_clamps_after_entry_removal(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Focus sidebar (selection starts at None), navigate down to the thread (index 1)
    focus_sidebar(&sidebar, cx);
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    // Collapse the group, which removes the thread from the list
    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    // Selection should be clamped to the last valid index (0 = header)
    let selection = sidebar.read_with(cx, |s, _| s.selection);
    let entry_count = sidebar.read_with(cx, |s, _| s.contents.entries.len());
    assert!(
        selection.unwrap_or(0) < entry_count,
        "selection {} should be within bounds (entries: {})",
        selection.unwrap_or(0),
        entry_count,
    );
}
