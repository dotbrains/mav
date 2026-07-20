use super::*;

async fn test_drag_tab_to_middle_tab_with_mouse_events(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    add_labeled_item(&pane, "D", false, cx);
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
    cx.run_until_parked();

    let tab_a_bounds = cx
        .debug_bounds("TAB-0")
        .expect("Tab A (index 0) should have debug bounds");
    let tab_c_bounds = cx
        .debug_bounds("TAB-2")
        .expect("Tab C (index 2) should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: tab_a_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: tab_c_bounds.center(),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseUpEvent {
        position: tab_c_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();

    assert_item_labels(&pane, ["B", "C", "A*", "D"], cx);
}

#[gpui::test]
async fn test_drag_pinned_tab_when_show_pinned_tabs_in_separate_row_enabled(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    set_pinned_tabs_separate_row(cx, true);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    let item_b = add_labeled_item(&pane, "B", false, cx);
    let item_c = add_labeled_item(&pane, "C", false, cx);
    let item_d = add_labeled_item(&pane, "D", false, cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(
            pane.index_for_item_id(item_a.item_id()).unwrap(),
            window,
            cx,
        );
        pane.pin_tab_at(
            pane.index_for_item_id(item_b.item_id()).unwrap(),
            window,
            cx,
        );
        pane.pin_tab_at(
            pane.index_for_item_id(item_c.item_id()).unwrap(),
            window,
            cx,
        );
        pane.pin_tab_at(
            pane.index_for_item_id(item_d.item_id()).unwrap(),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A!", "B!", "C!", "D*!"], cx);
    cx.run_until_parked();

    let tab_a_bounds = cx
        .debug_bounds("TAB-0")
        .expect("Tab A (index 0) should have debug bounds");
    let tab_c_bounds = cx
        .debug_bounds("TAB-2")
        .expect("Tab C (index 2) should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: tab_a_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: tab_c_bounds.center(),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseUpEvent {
        position: tab_c_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();

    assert_item_labels(&pane, ["B!", "C!", "A*!", "D!"], cx);
}

#[gpui::test]
async fn test_drag_unpinned_tab_when_show_pinned_tabs_in_separate_row_enabled(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    set_pinned_tabs_separate_row(cx, true);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    add_labeled_item(&pane, "D", false, cx);
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
    cx.run_until_parked();

    let tab_a_bounds = cx
        .debug_bounds("TAB-0")
        .expect("Tab A (index 0) should have debug bounds");
    let tab_c_bounds = cx
        .debug_bounds("TAB-2")
        .expect("Tab C (index 2) should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: tab_a_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: tab_c_bounds.center(),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseUpEvent {
        position: tab_c_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();

    assert_item_labels(&pane, ["B", "C", "A*", "D"], cx);
}

#[gpui::test]
async fn test_drag_mixed_tabs_when_show_pinned_tabs_in_separate_row_enabled(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    set_pinned_tabs_separate_row(cx, true);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    let item_b = add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    add_labeled_item(&pane, "D", false, cx);
    add_labeled_item(&pane, "E", false, cx);
    add_labeled_item(&pane, "F", false, cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(
            pane.index_for_item_id(item_a.item_id()).unwrap(),
            window,
            cx,
        );
        pane.pin_tab_at(
            pane.index_for_item_id(item_b.item_id()).unwrap(),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A!", "B!", "C", "D", "E", "F*"], cx);
    cx.run_until_parked();

    let tab_c_bounds = cx
        .debug_bounds("TAB-2")
        .expect("Tab C (index 2) should have debug bounds");
    let tab_e_bounds = cx
        .debug_bounds("TAB-4")
        .expect("Tab E (index 4) should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: tab_c_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    cx.simulate_event(MouseMoveEvent {
        position: tab_e_bounds.center(),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseUpEvent {
        position: tab_e_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();

    assert_item_labels(&pane, ["A!", "B!", "D", "E", "C*", "F"], cx);
}
