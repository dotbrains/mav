use super::*;

async fn test_middle_click_pinned_tab_does_not_close(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.pin_tab_at(
            pane.index_for_item_id(item_a.item_id()).unwrap(),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A!", "B*"], cx);
    cx.run_until_parked();

    let tab_a_bounds = cx
        .debug_bounds("TAB-0")
        .expect("Tab A (index 1) should have debug bounds");
    let tab_b_bounds = cx
        .debug_bounds("TAB-1")
        .expect("Tab B (index 2) should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: tab_a_bounds.center(),
        button: MouseButton::Middle,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });

    cx.run_until_parked();

    cx.simulate_event(MouseUpEvent {
        position: tab_a_bounds.center(),
        button: MouseButton::Middle,
        modifiers: Modifiers::default(),
        click_count: 1,
    });

    cx.run_until_parked();

    cx.simulate_event(MouseDownEvent {
        position: tab_b_bounds.center(),
        button: MouseButton::Middle,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });

    cx.run_until_parked();

    cx.simulate_event(MouseUpEvent {
        position: tab_b_bounds.center(),
        button: MouseButton::Middle,
        modifiers: Modifiers::default(),
        click_count: 1,
    });

    cx.run_until_parked();

    assert_item_labels(&pane, ["A*!"], cx);
}

#[gpui::test]
async fn test_double_click_pinned_tab_bar_empty_space_creates_new_tab(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // The real NewFile handler lives in editor::init, which isn't initialized
    // in workspace tests. Register a global action handler that sets a flag so
    // we can verify the action is dispatched without depending on the editor crate.
    // TODO: If editor::init is ever available in workspace tests, remove this
    // flag and assert the resulting tab bar state directly instead.
    let new_file_dispatched = Rc::new(Cell::new(false));
    cx.update(|_, cx| {
        let new_file_dispatched = new_file_dispatched.clone();
        cx.on_action(move |_: &NewFile, _cx| {
            new_file_dispatched.set(true);
        });
    });

    set_pinned_tabs_separate_row(cx, true);

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane
            .index_for_item_id(item_a.item_id())
            .expect("item A should exist");
        pane.pin_tab_at(ix, window, cx);
    });
    assert_item_labels(&pane, ["A!", "B*"], cx);
    cx.run_until_parked();

    let pinned_drop_target_bounds = cx
        .debug_bounds("pinned_tabs_border")
        .expect("pinned_tabs_border should have debug bounds");

    cx.simulate_event(MouseDownEvent {
        position: pinned_drop_target_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });

    cx.run_until_parked();

    cx.simulate_event(MouseUpEvent {
        position: pinned_drop_target_bounds.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
    });

    cx.run_until_parked();

    // TODO: If editor::init is ever available in workspace tests, replace this
    // with an assert_item_labels check that verifies a new tab is actually created.
    assert!(
        new_file_dispatched.get(),
        "Double-clicking pinned tab bar empty space should dispatch the new file action"
    );
}

#[gpui::test]
async fn test_add_item_with_new_item(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // 1. Add with a destination index
    //   a. Add before the active item
    set_labeled_items(&pane, ["A", "B*", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
            false,
            false,
            Some(0),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["D*", "A", "B", "C"], cx);

    //   b. Add after the active item
    set_labeled_items(&pane, ["A", "B*", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
            false,
            false,
            Some(2),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A", "B", "D*", "C"], cx);

    //   c. Add at the end of the item list (including off the length)
    set_labeled_items(&pane, ["A", "B*", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
            false,
            false,
            Some(5),
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

    // 2. Add without a destination index
    //   a. Add with active item at the start of the item list
    set_labeled_items(&pane, ["A*", "B", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    set_labeled_items(&pane, ["A", "D*", "B", "C"], cx);

    //   b. Add with active item at the end of the item list
    set_labeled_items(&pane, ["A", "B", "C*"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
}

#[gpui::test]
async fn test_add_item_with_existing_item(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // 1. Add with a destination index
    //   1a. Add before the active item
    let [_, _, _, d] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(d, false, false, Some(0), window, cx);
    });
    assert_item_labels(&pane, ["D*", "A", "B", "C"], cx);

    //   1b. Add after the active item
    let [_, _, _, d] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(d, false, false, Some(2), window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "D*", "C"], cx);

    //   1c. Add at the end of the item list (including off the length)
    let [a, _, _, _] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(a, false, false, Some(5), window, cx);
    });
    assert_item_labels(&pane, ["B", "C", "D", "A*"], cx);

    //   1d. Add same item to active index
    let [_, b, _] = set_labeled_items(&pane, ["A", "B*", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(b, false, false, Some(1), window, cx);
    });
    assert_item_labels(&pane, ["A", "B*", "C"], cx);

    //   1e. Add item to index after same item in last position
    let [_, _, c] = set_labeled_items(&pane, ["A", "B*", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(c, false, false, Some(2), window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    // 2. Add without a destination index
    //   2a. Add with active item at the start of the item list
    let [_, _, _, d] = set_labeled_items(&pane, ["A*", "B", "C", "D"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(d, false, false, None, window, cx);
    });
    assert_item_labels(&pane, ["A", "D*", "B", "C"], cx);

    //   2b. Add with active item at the end of the item list
    let [a, _, _, _] = set_labeled_items(&pane, ["A", "B", "C", "D*"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(a, false, false, None, window, cx);
    });
    assert_item_labels(&pane, ["B", "C", "D", "A*"], cx);

    //   2c. Add active item to active item at end of list
    let [_, _, c] = set_labeled_items(&pane, ["A", "B", "C*"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(c, false, false, None, window, cx);
    });
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    //   2d. Add active item to active item at start of list
    let [a, _, _] = set_labeled_items(&pane, ["A*", "B", "C"], cx);
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(a, false, false, None, window, cx);
    });
    assert_item_labels(&pane, ["A*", "B", "C"], cx);
}

#[gpui::test]
async fn test_add_item_with_same_project_entries(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    // singleton view
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_buffer_kind(ItemBufferKind::Singleton)
                    .with_label("buffer 1")
                    .with_project_items(&[TestProjectItem::new(1, "one.txt", cx)])
            })),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["buffer 1*"], cx);

    // new singleton view with the same project entry
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_buffer_kind(ItemBufferKind::Singleton)
                    .with_label("buffer 1")
                    .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
            })),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["buffer 1*"], cx);

    // new singleton view with different project entry
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_buffer_kind(ItemBufferKind::Singleton)
                    .with_label("buffer 2")
                    .with_project_items(&[TestProjectItem::new(2, "2.txt", cx)])
            })),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["buffer 1", "buffer 2*"], cx);

    // new multibuffer view with the same project entry
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_buffer_kind(ItemBufferKind::Multibuffer)
                    .with_label("multibuffer 1")
                    .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
            })),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(&pane, ["buffer 1", "buffer 2", "multibuffer 1*"], cx);

    // another multibuffer view with the same project entry
    pane.update_in(cx, |pane, window, cx| {
        pane.add_item(
            Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_buffer_kind(ItemBufferKind::Multibuffer)
                    .with_label("multibuffer 1b")
                    .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
            })),
            false,
            false,
            None,
            window,
            cx,
        );
    });
    assert_item_labels(
        &pane,
        ["buffer 1", "buffer 2", "multibuffer 1", "multibuffer 1b*"],
        cx,
    );
}
