use super::*;

#[gpui::test]
async fn test_close_window_with_worktrees_hot_exits(cx: &mut TestAppContext) {
    init_test(cx);

    // Register TestItem as a serializable item
    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "one": "" })).await;

    let project = Project::test(fs, ["root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // When there are dirty untitled items, but they can serialize, then there is no prompt.
    let item1 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    let item2 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item1.clone()), None, true, window, cx);
        w.add_item_to_active_pane(Box::new(item2.clone()), None, true, window, cx);
    });
    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::CloseWindow, window, cx)
    });
    assert!(task.await.unwrap());
}

// See https://github.com/mav-industries/mav/issues/55726.
//
// macOS only: on Linux/Windows, closing the last window sets
// `save_last_workspace`, which preserves the session (same as `Quit`),
// so hot-exit is safe there.
#[cfg(target_os = "macos")]
#[gpui::test]
async fn test_close_window_without_worktrees_prompts(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::CloseWindow, window, cx)
    });
    cx.executor().run_until_parked();

    assert!(
        cx.has_pending_prompt(),
        "closing a no-folder workspace with a dirty serializable item should prompt, \
         since the workspace will not be reachable after close"
    );
    cx.simulate_prompt_answer("Don't Save");
    cx.executor().run_until_parked();

    assert!(task.await.unwrap());
}

#[gpui::test]
async fn test_quit_without_worktrees_hot_exits(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::Quit, window, cx)
    });
    cx.executor().run_until_parked();

    assert!(
        !cx.has_pending_prompt(),
        "quitting should hot-exit silently; the session restore on next \
         launch will bring the dirty buffer back"
    );
    assert!(task.await.unwrap());
}

// See https://github.com/mav-industries/mav/issues/55726.
#[gpui::test]
async fn test_replace_window_without_worktrees_prompts(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
    });
    cx.executor().run_until_parked();

    assert!(
        cx.has_pending_prompt(),
        "replacing a workspace with a dirty serializable item should prompt, \
         since the workspace will be detached afterwards"
    );
    cx.simulate_prompt_answer("Don't Save");
    cx.executor().run_until_parked();

    assert!(task.await.unwrap());
}

#[gpui::test]
async fn test_replace_window_with_worktrees_hot_exits(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "one": "" })).await;

    let project = Project::test(fs, ["root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_serialize(|| Some(Task::ready(Ok(()))))
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
    });
    cx.executor().run_until_parked();

    assert!(
        !cx.has_pending_prompt(),
        "replacing a workspace with folder paths should hot-exit silently; \
         the buffer is recoverable by reopening the project"
    );
    assert!(task.await.unwrap());
}

#[gpui::test]
async fn test_close_window_with_failing_serialize_prompts(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        register_serializable_item::<TestItem>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let item = cx.new(|cx| {
        TestItem::new(cx).with_dirty(true).with_serialize(|| {
            Some(Task::ready(Err(anyhow::anyhow!(
                "FOREIGN KEY constraint failed"
            ))))
        })
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item.clone()), None, true, window, cx);
    });

    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::CloseWindow, window, cx)
    });
    cx.executor().run_until_parked();

    // The failing serialization must not short-circuit the close; a
    // save/discard prompt must be shown for the dirty scratch item.
    assert!(
        cx.has_pending_prompt(),
        "a save/discard prompt should be shown for the dirty scratch item \
         when its serialization fails"
    );
    cx.simulate_prompt_answer("Don't Save");
    cx.executor().run_until_parked();

    // Preparing to close succeeds, even though serialization failed.
    assert!(task.await.unwrap());
}

#[gpui::test]
async fn test_close_pane_items(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, None, cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    let item1 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_project_items(&[dirty_project_item(1, "1.txt", cx)])
    });
    let item2 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_conflict(true)
            .with_project_items(&[dirty_project_item(2, "2.txt", cx)])
    });
    let item3 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_conflict(true)
            .with_project_items(&[dirty_project_item(3, "3.txt", cx)])
    });
    let item4 = cx.new(|cx| {
        TestItem::new(cx).with_dirty(true).with_project_items(&[{
            let project_item = TestProjectItem::new_untitled(cx);
            project_item.update(cx, |project_item, _| project_item.is_dirty = true);
            project_item
        }])
    });
    let pane = workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item1.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(Box::new(item2.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(Box::new(item3.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(Box::new(item4.clone()), None, true, window, cx);
        workspace.active_pane().clone()
    });

    let close_items = pane.update_in(cx, |pane, window, cx| {
        pane.activate_item(1, true, true, window, cx);
        assert_eq!(pane.active_item().unwrap().item_id(), item2.item_id());
        let item1_id = item1.item_id();
        let item3_id = item3.item_id();
        let item4_id = item4.item_id();
        pane.close_items(window, cx, SaveIntent::Close, &move |id| {
            [item1_id, item3_id, item4_id].contains(&id)
        })
    });
    cx.executor().run_until_parked();

    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Save all");

    cx.executor().run_until_parked();

    // Item 1 is saved. There's a prompt to save item 3.
    pane.update(cx, |pane, cx| {
        assert_eq!(item1.read(cx).save_count, 1);
        assert_eq!(item1.read(cx).save_as_count, 0);
        assert_eq!(item1.read(cx).reload_count, 0);
        assert_eq!(pane.items_len(), 3);
        assert_eq!(pane.active_item().unwrap().item_id(), item3.item_id());
    });
    assert!(cx.has_pending_prompt());

    // Cancel saving item 3.
    cx.simulate_prompt_answer("Discard");
    cx.executor().run_until_parked();

    // Item 3 is reloaded. There's a prompt to save item 4.
    pane.update(cx, |pane, cx| {
        assert_eq!(item3.read(cx).save_count, 0);
        assert_eq!(item3.read(cx).save_as_count, 0);
        assert_eq!(item3.read(cx).reload_count, 1);
        assert_eq!(pane.items_len(), 2);
        assert_eq!(pane.active_item().unwrap().item_id(), item4.item_id());
    });

    // There's a prompt for a path for item 4.
    cx.simulate_new_path_selection(|_| Some(Default::default()));
    close_items.await.unwrap();

    // The requested items are closed.
    pane.update(cx, |pane, cx| {
        assert_eq!(item4.read(cx).save_count, 1);
        assert_eq!(item4.read(cx).save_as_count, 1);
        assert_eq!(item4.read(cx).reload_count, 0);
        assert_eq!(pane.items_len(), 1);
        assert_eq!(pane.active_item().unwrap().item_id(), item2.item_id());
    });
}

#[gpui::test]
async fn test_prompting_to_save_only_on_last_item_for_entry(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // Create several workspace items with single project entries, and two
    // workspace items with multiple project entries.
    let single_entry_items = (0..=4)
        .map(|project_entry_id| {
            cx.new(|cx| {
                TestItem::new(cx)
                    .with_dirty(true)
                    .with_project_items(&[dirty_project_item(
                        project_entry_id,
                        &format!("{project_entry_id}.txt"),
                        cx,
                    )])
            })
        })
        .collect::<Vec<_>>();
    let item_2_3 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_buffer_kind(ItemBufferKind::Multibuffer)
            .with_project_items(&[
                single_entry_items[2].read(cx).project_items[0].clone(),
                single_entry_items[3].read(cx).project_items[0].clone(),
            ])
    });
    let item_3_4 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_buffer_kind(ItemBufferKind::Multibuffer)
            .with_project_items(&[
                single_entry_items[3].read(cx).project_items[0].clone(),
                single_entry_items[4].read(cx).project_items[0].clone(),
            ])
    });

    // Create two panes that contain the following project entries:
    //   left pane:
    //     multi-entry items:   (2, 3)
    //     single-entry items:  0, 2, 3, 4
    //   right pane:
    //     single-entry items:  4, 1
    //     multi-entry items:   (3, 4)
    let (left_pane, right_pane) = workspace.update_in(cx, |workspace, window, cx| {
        let left_pane = workspace.active_pane().clone();
        workspace.add_item_to_active_pane(Box::new(item_2_3.clone()), None, true, window, cx);
        workspace.add_item_to_active_pane(
            single_entry_items[0].boxed_clone(),
            None,
            true,
            window,
            cx,
        );
        workspace.add_item_to_active_pane(
            single_entry_items[2].boxed_clone(),
            None,
            true,
            window,
            cx,
        );
        workspace.add_item_to_active_pane(
            single_entry_items[3].boxed_clone(),
            None,
            true,
            window,
            cx,
        );
        workspace.add_item_to_active_pane(
            single_entry_items[4].boxed_clone(),
            None,
            true,
            window,
            cx,
        );

        let right_pane =
            workspace.split_and_clone(left_pane.clone(), SplitDirection::Right, window, cx);

        let boxed_clone = single_entry_items[1].boxed_clone();
        let right_pane = window.spawn(cx, async move |cx| {
            right_pane.await.inspect(|right_pane| {
                right_pane
                    .update_in(cx, |pane, window, cx| {
                        pane.add_item(boxed_clone, true, true, None, window, cx);
                        pane.add_item(Box::new(item_3_4.clone()), true, true, None, window, cx);
                    })
                    .unwrap();
            })
        });

        (left_pane, right_pane)
    });
    let right_pane = right_pane.await.unwrap();
    cx.focus(&right_pane);

    let close = right_pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(&CloseAllItems::default(), window, cx)
            .unwrap()
    });
    cx.executor().run_until_parked();

    let msg = cx.pending_prompt().unwrap().0;
    assert!(msg.contains("1.txt"));
    assert!(!msg.contains("2.txt"));
    assert!(!msg.contains("3.txt"));
    assert!(!msg.contains("4.txt"));

    // With best-effort close, cancelling item 1 keeps it open but items 4
    // and (3,4) still close since their entries exist in left pane.
    cx.simulate_prompt_answer("Cancel");
    close.await;

    right_pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items_len(), 1);
    });

    // Remove item 3 from left pane, making (2,3) the only item with entry 3.
    left_pane
        .update_in(cx, |left_pane, window, cx| {
            left_pane.close_item_by_id(
                single_entry_items[3].entity_id(),
                SaveIntent::Skip,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    let close = left_pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(&CloseAllItems::default(), window, cx)
            .unwrap()
    });
    cx.executor().run_until_parked();

    let details = cx.pending_prompt().unwrap().1;
    assert!(details.contains("0.txt"));
    assert!(details.contains("3.txt"));
    assert!(details.contains("4.txt"));
    // Ideally 2.txt wouldn't appear since entry 2 still exists in item 2.
    // But we can only save whole items, so saving (2,3) for entry 3 includes 2.
    // assert!(!details.contains("2.txt"));

    cx.simulate_prompt_answer("Save all");
    cx.executor().run_until_parked();
    close.await;

    left_pane.read_with(cx, |pane, _| {
        assert_eq!(pane.items_len(), 0);
    });
}
