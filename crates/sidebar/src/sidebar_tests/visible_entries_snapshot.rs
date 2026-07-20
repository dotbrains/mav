use super::*;

#[gpui::test]
async fn test_keyboard_select_next_and_previous(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Entries: [header, thread3, thread2, thread1]
    // Focusing the sidebar does not set a selection; select_next/select_previous
    // handle None gracefully by starting from the first or last entry.
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // First SelectNext from None starts at index 0
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // Move down through remaining entries
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));

    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // At the end, wraps back to first entry
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // Navigate back to the end
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // Move back up
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));

    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // At the top, selection clears (focus returns to editor)
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);
}

#[gpui::test]
async fn test_keyboard_select_first_and_last(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);

    // SelectLast jumps to the end
    cx.dispatch_action(SelectLast);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // SelectFirst jumps to the beginning
    cx.dispatch_action(SelectFirst);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_keyboard_focus_in_does_not_set_selection(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Initially no selection
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // Open the sidebar so it's rendered, then focus it to trigger focus_in.
    // focus_in no longer sets a default selection.
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // Manually set a selection, blur, then refocus — selection should be preserved
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    cx.update(|window, _cx| {
        window.blur();
    });
    cx.run_until_parked();

    sidebar.update_in(cx, |_, window, cx| {
        cx.focus_self(window);
    });
    cx.run_until_parked();
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_keyboard_confirm_on_project_header_toggles_collapse(cx: &mut TestAppContext) {
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

    // Focus the sidebar and select the header
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    // Confirm on project header collapses the group
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );

    // Confirm again expands the group
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]  <== selected",
            "  Thread 1",
        ]
    );
}
