use super::*;

#[gpui::test]
async fn test_keeps_file_finder_open_after_modifier_keys_release(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "1.txt": "// One",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;

    cx.simulate_modifiers_change(Modifiers::secondary_key());
    open_file_picker(&workspace, cx);

    cx.simulate_modifiers_change(Modifiers::none());
    active_file_picker(&workspace, cx);
}

#[gpui::test]
async fn test_opens_file_on_modifier_keys_release(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "1.txt": "// One",
                "2.txt": "// Two",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;
    open_queried_buffer("2", 1, "2.txt", &workspace, cx).await;

    cx.simulate_modifiers_change(Modifiers::secondary_key());
    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_selection(finder, 0, "2.txt");
        assert_match_at_position(finder, 1, "1.txt");
    });

    cx.dispatch_action(SelectNext);
    cx.simulate_modifiers_change(Modifiers::none());
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "1.txt");
    });
}

#[gpui::test]
async fn test_switches_between_release_norelease_modes_on_forward_nav(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "1.txt": "// One",
                "2.txt": "// Two",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;
    open_queried_buffer("2", 1, "2.txt", &workspace, cx).await;

    // Open with a shortcut
    cx.simulate_modifiers_change(Modifiers::secondary_key());
    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_selection(finder, 0, "2.txt");
        assert_match_at_position(finder, 1, "1.txt");
    });

    // Switch to navigating with other shortcuts
    // Don't open file on modifiers release
    cx.simulate_modifiers_change(Modifiers::control());
    cx.dispatch_action(SelectNext);
    cx.simulate_modifiers_change(Modifiers::none());
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_at_position(finder, 0, "2.txt");
        assert_match_selection(finder, 1, "1.txt");
    });

    // Back to navigation with initial shortcut
    // Open file on modifiers release
    cx.simulate_modifiers_change(Modifiers::secondary_key());
    cx.dispatch_action(ToggleFileFinder::default());
    cx.simulate_modifiers_change(Modifiers::none());
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "2.txt");
    });
}

#[gpui::test]
async fn test_switches_between_release_norelease_modes_on_backward_nav(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "1.txt": "// One",
                "2.txt": "// Two",
                "3.txt": "// Three"
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;
    open_queried_buffer("2", 1, "2.txt", &workspace, cx).await;
    open_queried_buffer("3", 1, "3.txt", &workspace, cx).await;

    // Open with a shortcut
    cx.simulate_modifiers_change(Modifiers::secondary_key());
    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "3.txt");
        assert_match_at_position(finder, 1, "2.txt");
        assert_match_at_position(finder, 2, "1.txt");
    });

    // Switch to navigating with other shortcuts
    // Don't open file on modifiers release
    cx.simulate_modifiers_change(Modifiers::control());
    cx.dispatch_action(menu::SelectPrevious);
    cx.simulate_modifiers_change(Modifiers::none());
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "3.txt");
        assert_match_at_position(finder, 1, "2.txt");
        assert_match_selection(finder, 2, "1.txt");
    });

    // Back to navigation with initial shortcut
    // Open file on modifiers release
    cx.simulate_modifiers_change(Modifiers::secondary_key());
    cx.dispatch_action(SelectPrevious); // <-- File Finder's SelectPrevious, not menu's
    cx.simulate_modifiers_change(Modifiers::none());
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "3.txt");
    });
}

#[gpui::test]
async fn test_extending_modifiers_does_not_confirm_selection(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "1.txt": "// One",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;

    cx.simulate_modifiers_change(Modifiers::secondary_key());
    open_file_picker(&workspace, cx);

    cx.simulate_modifiers_change(Modifiers::command_shift());
    active_file_picker(&workspace, cx);
}

#[gpui::test]
async fn test_repeat_toggle_action(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/test",
            json!({
                "00.txt": "",
                "01.txt": "",
                "02.txt": "",
                "03.txt": "",
                "04.txt": "",
                "05.txt": "",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/test".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    cx.dispatch_action(ToggleFileFinder::default());
    let picker = active_file_picker(&workspace, cx);

    picker.update_in(cx, |picker, window, cx| {
        picker.update_matches(".txt".to_string(), window, cx)
    });

    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();

    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 7);
        assert_eq!(picker.delegate.selected_index, 0);
    });

    // When toggling repeatedly, the picker scrolls to reveal the selected item.
    cx.dispatch_action(ToggleFileFinder::default());
    cx.dispatch_action(ToggleFileFinder::default());
    cx.dispatch_action(ToggleFileFinder::default());

    cx.run_until_parked();

    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 7);
        assert_eq!(picker.delegate.selected_index, 3);
    });
}

#[gpui::test]
async fn test_open_without_dismiss_keeps_finder_open(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1.txt": "content1",
                    "file2.txt": "content2",
                    "file3.txt": "content3",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "file");
    picker.update(cx, |picker, _| {
        assert!(
            picker.delegate.matches.len() >= 3,
            "Expected at least 3 matches for 'file', got {}",
            picker.delegate.matches.len()
        );
    });

    cx.dispatch_action(OpenWithoutDismiss);
    cx.run_until_parked();

    // Finder must still be visible after opening a file without dismiss.
    workspace.update(cx, |workspace, cx| {
        assert!(
            workspace.active_modal::<FileFinder>(cx).is_some(),
            "File finder should remain open after OpenWithoutDismiss"
        );
    });

    // Exactly one file was opened in the pane.
    cx.read(|cx| {
        let items: Vec<_> = workspace.read(cx).active_pane().read(cx).items().collect();
        assert_eq!(items.len(), 1, "One file should be open in the pane");
    });

    // The search query and results are preserved so the user can continue browsing.
    picker.update(cx, |picker, _| {
        assert!(
            picker.delegate.matches.len() >= 3,
            "Search results should remain unchanged after OpenWithoutDismiss"
        );
    });
}

#[gpui::test]
async fn test_open_without_dismiss_opens_multiple_files(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "alpha.txt": "alpha",
                    "beta.txt": "beta",
                    "gamma.txt": "gamma",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (_picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "a");

    // Open the first match and stay in the finder.
    cx.dispatch_action(OpenWithoutDismiss);
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        assert!(
            workspace.active_modal::<FileFinder>(cx).is_some(),
            "Finder should remain open after first OpenWithoutDismiss"
        );
    });
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.items().count(),
            1,
            "One file open after first OpenWithoutDismiss"
        );
    });

    // Navigate to the next result and open it too.
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(OpenWithoutDismiss);
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        assert!(
            workspace.active_modal::<FileFinder>(cx).is_some(),
            "Finder should remain open after second OpenWithoutDismiss"
        );
    });
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.items().count(),
            2,
            "Two files open after second OpenWithoutDismiss"
        );
        // The second opened file should now be the active tab.
        let active_index = pane.active_item_index();
        assert_eq!(active_index, 1, "Second file should be the active tab");
    });
}

#[gpui::test]
async fn test_open_without_dismiss_then_confirm_closes_finder(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "first.txt": "first",
                    "second.txt": "second",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "t");
    picker.update(cx, |picker, _| {
        assert!(picker.delegate.matches.len() >= 2);
    });

    // Open first file, keep finder open.
    cx.dispatch_action(OpenWithoutDismiss);
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        assert!(workspace.active_modal::<FileFinder>(cx).is_some());
    });

    // Navigate to the next match and confirm normally — this should close the finder.
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        assert!(
            workspace.active_modal::<FileFinder>(cx).is_none(),
            "Finder should be closed after regular Confirm"
        );
    });

    // Two files were opened in total, with the confirmed one now active.
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(pane.items().count(), 2, "Two files should be open total");
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        let title = active_editor.read(cx).title(cx);
        assert!(
            title == "second.txt" || title == "first.txt",
            "Active editor should be one of the opened files, got: {title}"
        );
    });
}

#[gpui::test]
async fn test_reopen_with_preview_keeps_results_width(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({ "a.txt": "", "b.txt": "" }))
        .await;
    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (picker, workspace, cx) = build_find_picker(project, cx);

    cx.dispatch_action(picker::SetPreviewRight);
    cx.run_until_parked();
    let in_session_width = picker.update_in(cx, |picker, window, _| picker.results_width(window));

    cx.dispatch_action(Cancel);
    cx.run_until_parked();

    let picker = open_file_picker(&workspace, cx);
    cx.run_until_parked();
    let reopened_width = picker.update_in(cx, |picker, window, _| picker.results_width(window));

    assert_eq!(
        in_session_width, reopened_width,
        "reopening with the side preview must keep the same results width as the in-session toggle"
    );

    // The preview layout is persisted in the key-value store, and tests in a
    // process share one in-memory fallback store (no per-App `AppDatabase` is
    // set in tests, so `AppDatabase::global` falls back to the shared static).
    // Reset to the default layout so this write doesn't leak into other tests.
    cx.dispatch_action(picker::SetPreviewHidden);
    cx.run_until_parked();
}
