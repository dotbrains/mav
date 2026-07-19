use super::*;

#[gpui::test]
async fn test_open_in_active_pane_deduplicates_files_by_path(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "1.txt": "",
                "2.txt": "",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_buffer("1.txt", &workspace, cx).await;
    open_buffer("2.txt", &workspace, cx).await;

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            workspace::SplitDirection::Right,
            window,
            cx,
        );
    });
    open_buffer("1.txt", &workspace, cx).await;

    let tab_switcher = open_tab_switcher_for_active_pane(&workspace, cx);

    tab_switcher.read_with(cx, |picker, _cx| {
        assert_eq!(
            picker.delegate.matches.len(),
            2,
            "should show 2 unique files despite 3 tabs"
        );
    });
}

#[gpui::test]
async fn test_open_in_active_pane_clones_files_to_current_pane(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({"1.txt": ""}))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_buffer("1.txt", &workspace, cx).await;

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            workspace::SplitDirection::Right,
            window,
            cx,
        );
    });

    let panes = workspace.read_with(cx, |workspace, _| workspace.panes().to_vec());

    let tab_switcher = open_tab_switcher_for_active_pane(&workspace, cx);
    tab_switcher.update(cx, |picker, _| {
        picker.delegate.selected_index = 0;
    });

    cx.dispatch_action(menu::Confirm);
    cx.run_until_parked();

    let editor_1 = panes[0].read_with(cx, |pane, cx| {
        pane.active_item()
            .and_then(|item| item.act_as::<Editor>(cx))
            .expect("pane 1 should have editor")
    });

    let editor_2 = panes[1].read_with(cx, |pane, cx| {
        pane.active_item()
            .and_then(|item| item.act_as::<Editor>(cx))
            .expect("pane 2 should have editor")
    });

    assert_ne!(
        editor_1.entity_id(),
        editor_2.entity_id(),
        "should clone to new instance"
    );
}

#[gpui::test]
async fn test_open_in_active_pane_moves_terminals_to_current_pane(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let test_item = cx.new(|cx| TestItem::new(cx).with_label("terminal"));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(test_item.clone()), None, true, window, cx);
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            workspace::SplitDirection::Right,
            window,
            cx,
        );
    });

    let panes = workspace.read_with(cx, |workspace, _| workspace.panes().to_vec());

    let tab_switcher = open_tab_switcher_for_active_pane(&workspace, cx);
    tab_switcher.update(cx, |picker, _| {
        picker.delegate.selected_index = 0;
    });

    cx.dispatch_action(menu::Confirm);
    cx.run_until_parked();

    assert!(
        !panes[0].read_with(cx, |pane, _| {
            pane.items()
                .any(|item| item.item_id() == test_item.item_id())
        }),
        "should be removed from pane 1"
    );
    assert!(
        panes[1].read_with(cx, |pane, _| {
            pane.items()
                .any(|item| item.item_id() == test_item.item_id())
        }),
        "should be moved to pane 2"
    );
}

#[gpui::test]
async fn test_open_in_active_pane_closes_file_in_all_panes(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({"1.txt": ""}))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_buffer("1.txt", &workspace, cx).await;

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            workspace::SplitDirection::Right,
            window,
            cx,
        );
    });
    open_buffer("1.txt", &workspace, cx).await;

    let panes = workspace.read_with(cx, |workspace, _| workspace.panes().to_vec());

    let tab_switcher = open_tab_switcher_for_active_pane(&workspace, cx);
    tab_switcher.update(cx, |picker, _| {
        picker.delegate.selected_index = 0;
    });

    cx.dispatch_action(CloseSelectedItem);
    cx.run_until_parked();

    for pane in &panes {
        assert_eq!(
            pane.read_with(cx, |pane, _| pane.items_len()),
            0,
            "all panes should be empty"
        );
    }
}

#[gpui::test]
async fn test_toggle_all_stays_open_after_closing_last_tab_in_active_pane(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a.txt": "",
                "b.txt": "",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let tab_a = open_buffer("a.txt", &workspace, cx).await;
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(
            workspace.active_pane().clone(),
            workspace::SplitDirection::Right,
            window,
            cx,
        );
    });
    open_buffer("b.txt", &workspace, cx).await;

    cx.dispatch_action(ToggleAll);
    let tab_switcher = get_active_tab_switcher(&workspace, cx);

    tab_switcher.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 2);
        picker.delegate.selected_index = 0;
    });

    cx.dispatch_action(CloseSelectedItem);
    cx.run_until_parked();

    let tab_switcher = get_active_tab_switcher(&workspace, cx);
    tab_switcher.update(cx, |picker, cx| {
        assert_eq!(picker.delegate.matches.len(), 1);
        assert_match_at_position(picker, 0, tab_a.boxed_clone());
        let _ = cx;
    });
}
