use super::*;

async fn test_discard_all_reloads_from_disk(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item_a = add_labeled_item(&pane, "A", true, cx);
    item_a.update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "A.txt", cx))
    });
    let item_b = add_labeled_item(&pane, "B", true, cx);
    item_b.update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "B.txt", cx))
    });
    assert_item_labels(&pane, ["A^", "B*^"], cx);

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

    item_a.read_with(cx, |item, _| {
        assert_eq!(item.reload_count, 1, "item A should have been reloaded");
        assert!(
            !item.is_dirty,
            "item A should no longer be dirty after reload"
        );
    });
    item_b.read_with(cx, |item, _| {
        assert_eq!(item.reload_count, 1, "item B should have been reloaded");
        assert!(
            !item.is_dirty,
            "item B should no longer be dirty after reload"
        );
    });
}

#[gpui::test]
async fn test_dont_save_single_file_reloads_from_disk(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item = add_labeled_item(&pane, "Dirty", true, cx);
    item.update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "Dirty.txt", cx))
    });
    assert_item_labels(&pane, ["Dirty*^"], cx);

    let close_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(item.item_id(), SaveIntent::Close, window, cx)
    });

    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    close_task.await.unwrap();
    assert_item_labels(&pane, [], cx);

    item.read_with(cx, |item, _| {
        assert_eq!(item.reload_count, 1, "item should have been reloaded");
        assert!(
            !item.is_dirty,
            "item should no longer be dirty after reload"
        );
    });
}

#[gpui::test]
async fn test_format_runs_on_first_save_of_new_file(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item = add_labeled_item(&pane, "untitled", true, cx);
    item.update(cx, |item, cx| {
        item.project_items.push(TestProjectItem::new_untitled(cx));
    });
    assert_item_labels(&pane, ["untitled*^"], cx);

    let close_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(item.item_id(), SaveIntent::Save, window, cx)
    });

    cx.executor().run_until_parked();
    cx.simulate_new_path_selection(|_| Some(Default::default()));
    close_task.await.unwrap();

    item.read_with(cx, |item, _| {
        assert_eq!(item.save_as_count, 1);
        assert_eq!(
            item.save_count, 1,
            "formatter should run after the file is given a path on first save"
        );
    });
}

#[gpui::test]
async fn test_format_does_not_run_on_first_save_when_save_without_format(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let item = add_labeled_item(&pane, "untitled", true, cx);
    item.update(cx, |item, cx| {
        item.project_items.push(TestProjectItem::new_untitled(cx));
    });
    assert_item_labels(&pane, ["untitled*^"], cx);

    let close_task = pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(item.item_id(), SaveIntent::SaveWithoutFormat, window, cx)
    });

    cx.executor().run_until_parked();
    cx.simulate_new_path_selection(|_| Some(Default::default()));
    close_task.await.unwrap();

    item.read_with(cx, |item, _| {
        assert_eq!(item.save_as_count, 1);
        assert_eq!(
            item.save_count, 0,
            "formatter should not run when SaveWithoutFormat is used"
        );
    });
}

#[gpui::test]
async fn test_discard_does_not_reload_multibuffer(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let singleton_item = pane.update_in(cx, |pane, window, cx| {
        let item = Box::new(cx.new(|cx| {
            TestItem::new(cx)
                .with_label("Singleton")
                .with_dirty(true)
                .with_buffer_kind(ItemBufferKind::Singleton)
        }));
        pane.add_item(item.clone(), false, false, None, window, cx);
        item
    });
    singleton_item.update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "Singleton.txt", cx))
    });

    let multi_item = pane.update_in(cx, |pane, window, cx| {
        let item = Box::new(cx.new(|cx| {
            TestItem::new(cx)
                .with_label("Multi")
                .with_dirty(true)
                .with_buffer_kind(ItemBufferKind::Multibuffer)
        }));
        pane.add_item(item.clone(), false, false, None, window, cx);
        item
    });
    multi_item.update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "Multi.txt", cx))
    });

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

    singleton_item.read_with(cx, |item, _| {
        assert_eq!(item.reload_count, 1, "singleton should have been reloaded");
        assert!(
            !item.is_dirty,
            "singleton should no longer be dirty after reload"
        );
    });
    multi_item.read_with(cx, |item, _| {
        assert_eq!(
            item.reload_count, 0,
            "multibuffer should not have been reloaded"
        );
    });
}

#[gpui::test]
async fn test_close_multibuffer_items(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let add_labeled_item =
        |pane: &Entity<Pane>, label, is_dirty, kind: ItemBufferKind, cx: &mut VisualTestContext| {
            pane.update_in(cx, |pane, window, cx| {
                let labeled_item = Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_label(label)
                        .with_dirty(is_dirty)
                        .with_buffer_kind(kind)
                }));
                pane.add_item(labeled_item.clone(), false, false, None, window, cx);
                labeled_item
            })
        };

    let item_a = add_labeled_item(&pane, "A", false, ItemBufferKind::Multibuffer, cx);
    add_labeled_item(&pane, "B", false, ItemBufferKind::Multibuffer, cx);
    add_labeled_item(&pane, "C", false, ItemBufferKind::Singleton, cx);
    assert_item_labels(&pane, ["A", "B", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.pin_tab_at(ix, window, cx);
        pane.close_multibuffer_items(
            &CloseMultibufferItems {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();
    assert_item_labels(&pane, ["A!", "C*"], cx);

    pane.update_in(cx, |pane, window, cx| {
        let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
        pane.unpin_tab_at(ix, window, cx);
        pane.close_multibuffer_items(
            &CloseMultibufferItems {
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

    add_labeled_item(&pane, "A", true, ItemBufferKind::Singleton, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(1, "A.txt", cx))
    });
    add_labeled_item(&pane, "B", true, ItemBufferKind::Multibuffer, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "B.txt", cx))
    });
    add_labeled_item(&pane, "D", true, ItemBufferKind::Multibuffer, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(3, "D.txt", cx))
    });
    assert_item_labels(&pane, ["C", "A^", "B^", "D*^"], cx);

    let save = pane.update_in(cx, |pane, window, cx| {
        pane.close_multibuffer_items(
            &CloseMultibufferItems {
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
    assert_item_labels(&pane, ["C", "A*^"], cx);

    add_labeled_item(&pane, "B", true, ItemBufferKind::Multibuffer, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(2, "B.txt", cx))
    });
    add_labeled_item(&pane, "D", true, ItemBufferKind::Multibuffer, cx).update(cx, |item, cx| {
        item.project_items
            .push(TestProjectItem::new_dirty(3, "D.txt", cx))
    });
    assert_item_labels(&pane, ["C", "A^", "B^", "D*^"], cx);
    let save = pane.update_in(cx, |pane, window, cx| {
        pane.close_multibuffer_items(
            &CloseMultibufferItems {
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
    assert_item_labels(&pane, ["C", "A*^"], cx);
}

#[gpui::test]
async fn test_close_with_save_intent(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let a = cx.update(|_, cx| TestProjectItem::new_dirty(1, "A.txt", cx));
    let b = cx.update(|_, cx| TestProjectItem::new_dirty(1, "B.txt", cx));
    let c = cx.update(|_, cx| TestProjectItem::new_dirty(1, "C.txt", cx));

    add_labeled_item(&pane, "AB", true, cx).update(cx, |item, _| {
        item.project_items.push(a.clone());
        item.project_items.push(b.clone());
    });
    add_labeled_item(&pane, "C", true, cx).update(cx, |item, _| item.project_items.push(c.clone()));
    assert_item_labels(&pane, ["AB^", "C*^"], cx);

    pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(
            &CloseAllItems {
                save_intent: Some(SaveIntent::Save),
                close_pinned: false,
            },
            window,
            cx,
        )
    })
    .await
    .unwrap();

    assert_item_labels(&pane, [], cx);
    cx.update(|_, cx| {
        assert!(!a.read(cx).is_dirty);
        assert!(!b.read(cx).is_dirty);
        assert!(!c.read(cx).is_dirty);
    });
}
