use super::*;

async fn test_close_all_items(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", false, cx);
    add_labeled_item(&pane, "B", false, cx);
    add_labeled_item(&pane, "C", false, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A*!"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    assert_item_labels(&pane, [], cx);

    add_labeled_item(&pane, "A", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "A.txt", cx))
    });
    add_labeled_item(&pane, "B", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "B.txt", cx))
    });
    add_labeled_item(&pane, "C", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(3, "C.txt", cx))
    });
    assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

    let save = pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    });

    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Save all");
    save.await.unwrap();
    assert_item_labels(&pane, [], cx);

    add_labeled_item(&pane, "A", true, cx);
    add_labeled_item(&pane, "B", true, cx);
    add_labeled_item(&pane, "C", true, cx);
    assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);
    let save = pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    });

    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Discard all");
    save.await.unwrap();
    assert_item_labels(&pane, [], cx);

    add_labeled_item(&pane, "A", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "A.txt", cx))
    });
    add_labeled_item(&pane, "B", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "B.txt", cx))
    });
    add_labeled_item(&pane, "C", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(3, "C.txt", cx))
    });
    assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

    let close_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    });

    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Discard all");
    close_task.await.unwrap();
    assert_item_labels(&pane, [], cx);

    add_labeled_item(&pane, "Clean1", false, cx);
    add_labeled_item(&pane, "Dirty", true, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "Dirty.txt", cx))
    });
    add_labeled_item(&pane, "Clean2", false, cx);
    assert_item_labels(&pane, ["Clean1", "Dirty^", "Clean2*"], cx);

    let close_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    });

    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    close_task.await.unwrap();
    assert_item_labels(&pane, ["Dirty*^"], cx);
}
