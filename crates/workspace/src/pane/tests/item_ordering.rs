use super::*;

async fn test_remove_item_ordering_history(cx: &mut TestAppContext) {
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

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, false, false, window, cx)
    });
    add_labeled_item(&pane, "1", false, cx);
    assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B*", "C", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(3, false, false, window, cx)
    });
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B*", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A*"], cx);
}

#[gpui::test]
async fn test_remove_item_ordering_neighbour(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update_global::<SettingsStore, ()>(|s, cx| {
        s.update_user_settings(cx, |s| {
            s.tabs.get_or_insert_default().activate_on_close = Some(ActivateOnClose::Neighbour);
        });
    });
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

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, false, false, window, cx)
    });
    add_labeled_item(&pane, "1", false, cx);
    assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B", "C*", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(3, false, false, window, cx)
    });
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A*"], cx);
}

#[gpui::test]
async fn test_remove_item_ordering_left_neighbour(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update_global::<SettingsStore, ()>(|s, cx| {
        s.update_user_settings(cx, |s| {
            s.tabs.get_or_insert_default().activate_on_close = Some(ActivateOnClose::LeftNeighbour);
        });
    });
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

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, false, false, window, cx)
    });
    add_labeled_item(&pane, "1", false, cx);
    assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B*", "C", "D"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(3, false, false, window, cx)
    });
    assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(0, false, false, window, cx)
    });
    assert_item_labels(&pane, ["A*", "B", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["B*", "C"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["C*"], cx);
}
