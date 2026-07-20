use super::*;

#[gpui::test]
async fn test_tab_disambiguation(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // Adding an item with no ambiguity renders the tab without detail.
    let item1 = cx.new(|cx| {
        let mut item = TestItem::new(cx);
        item.tab_descriptions = Some(vec!["c", "b1/c", "a/b1/c"]);
        item
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item1.clone()), None, true, window, cx);
    });
    item1.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(0)));

    // Adding an item that creates ambiguity increases the level of detail on
    // both tabs.
    let item2 = cx.new_window_entity(|_window, cx| {
        let mut item = TestItem::new(cx);
        item.tab_descriptions = Some(vec!["c", "b2/c", "a/b2/c"]);
        item
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item2.clone()), None, true, window, cx);
    });
    item1.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(1)));
    item2.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(1)));

    // Adding an item that creates ambiguity increases the level of detail only
    // on the ambiguous tabs. In this case, the ambiguity can't be resolved so
    // we stop at the highest detail available.
    let item3 = cx.new(|cx| {
        let mut item = TestItem::new(cx);
        item.tab_descriptions = Some(vec!["c", "b2/c", "a/b2/c"]);
        item
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item3.clone()), None, true, window, cx);
    });
    item1.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(1)));
    item2.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(3)));
    item3.read_with(cx, |item, _| assert_eq!(item.tab_detail.get(), Some(3)));
}

#[gpui::test]
async fn test_tracking_active_path(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "one.txt": "",
            "two.txt": "",
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "three.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs, ["root1".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let item1 = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "one.txt", cx)]));
    let item2 = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(2, "two.txt", cx)]));

    // Add an item to an empty pane
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item1), None, true, window, cx)
    });
    project.update(cx, |project, cx| {
        assert_eq!(
            project.active_entry(),
            project
                .entry_for_path(&(worktree_id, rel_path("one.txt")).into(), cx)
                .map(|e| e.id)
        );
    });
    assert_eq!(cx.window_title().as_deref(), Some("root1 — one.txt"));

    // Add a second item to a non-empty pane
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item2), None, true, window, cx)
    });
    assert_eq!(cx.window_title().as_deref(), Some("root1 — two.txt"));
    project.update(cx, |project, cx| {
        assert_eq!(
            project.active_entry(),
            project
                .entry_for_path(&(worktree_id, rel_path("two.txt")).into(), cx)
                .map(|e| e.id)
        );
    });

    // Close the active item
    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(&Default::default(), window, cx)
    })
    .await
    .unwrap();
    assert_eq!(cx.window_title().as_deref(), Some("root1 — one.txt"));
    project.update(cx, |project, cx| {
        assert_eq!(
            project.active_entry(),
            project
                .entry_for_path(&(worktree_id, rel_path("one.txt")).into(), cx)
                .map(|e| e.id)
        );
    });

    // Add a project folder
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("root2", true, cx)
        })
        .await
        .unwrap();
    assert_eq!(cx.window_title().as_deref(), Some("root1, root2 — one.txt"));

    // Remove a project folder
    project.update(cx, |project, cx| project.remove_worktree(worktree_id, cx));
    assert_eq!(cx.window_title().as_deref(), Some("root2 — one.txt"));
}

#[gpui::test]
async fn test_document_path_updates_with_active_item(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "one.txt": "",
            "two.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs, ["root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let item1 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[new_test_project_item(
            1,
            "one.txt",
            worktree_id,
            cx,
        )])
    });
    let item2 = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[new_test_project_item(
            2,
            "two.txt",
            worktree_id,
            cx,
        )])
    });

    // Initially no document path
    assert_eq!(cx.document_path(), None);

    // Add an item - document path should be set
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item1), None, true, window, cx)
    });
    assert_eq!(
        cx.document_path(),
        Some(std::path::PathBuf::from("root/one.txt"))
    );

    // Add a second item - document path should update
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item2), None, true, window, cx)
    });
    assert_eq!(
        cx.document_path(),
        Some(std::path::PathBuf::from("root/two.txt"))
    );

    // Close the active item - document path should revert to first item
    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(&Default::default(), window, cx)
    })
    .await
    .unwrap();
    assert_eq!(
        cx.document_path(),
        Some(std::path::PathBuf::from("root/one.txt"))
    );

    // Close all items - document path should be cleared
    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(&Default::default(), window, cx)
    })
    .await
    .unwrap();
    assert_eq!(cx.document_path(), None);
}

#[gpui::test]
async fn test_close_window(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "one": "" })).await;

    let project = Project::test(fs, ["root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // When there are no dirty items, there's nothing to do.
    let item1 = cx.new(TestItem::new);
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item1.clone()), None, true, window, cx)
    });
    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::CloseWindow, window, cx)
    });
    assert!(task.await.unwrap());

    // When there are dirty untitled items, prompt to save each one. If the user
    // cancels any prompt, then abort.
    let item2 = cx.new(|cx| TestItem::new(cx).with_dirty(true));
    let item3 = cx.new(|cx| {
        TestItem::new(cx)
            .with_dirty(true)
            .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
    });
    workspace.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item2.clone()), None, true, window, cx);
        w.add_item_to_active_pane(Box::new(item3.clone()), None, true, window, cx);
    });
    let task = workspace.update_in(cx, |w, window, cx| {
        w.prepare_to_close(CloseIntent::CloseWindow, window, cx)
    });
    cx.executor().run_until_parked();
    cx.simulate_prompt_answer("Cancel"); // cancel save all
    cx.executor().run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(!task.await.unwrap());
}

#[gpui::test]
async fn test_multi_workspace_close_window_multiple_workspaces_cancel(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "one": "" })).await;

    let project_a = Project::test(fs.clone(), ["root".as_ref()], cx).await;
    let project_b = Project::test(fs, ["root".as_ref()], cx).await;
    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    cx.run_until_parked();

    multi_workspace_handle
        .update(cx, |mw, _window, cx| {
            mw.open_sidebar(cx);
        })
        .unwrap();

    let workspace_a = multi_workspace_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project_b, window, cx)
        })
        .unwrap();

    // Activate workspace A
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.activate(workspace_a.clone(), None, window, cx);
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

    // Workspace A has a clean item
    let item_a = cx.new(TestItem::new);
    workspace_a.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item_a.clone()), None, true, window, cx)
    });

    // Workspace B has a dirty item
    let item_b = cx.new(|cx| TestItem::new(cx).with_dirty(true));
    workspace_b.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item_b.clone()), None, true, window, cx)
    });

    // Verify workspace A is active
    multi_workspace_handle
        .read_with(cx, |mw, _| {
            assert_eq!(mw.workspace(), &workspace_a);
        })
        .unwrap();

    // Dispatch CloseWindow — workspace A will pass, workspace B will prompt
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.close_window(&CloseWindow, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // Workspace B should now be active since it has dirty items that need attention
    multi_workspace_handle
        .read_with(cx, |mw, _| {
            assert_eq!(
                mw.workspace(),
                &workspace_b,
                "workspace B should be activated when it prompts"
            );
        })
        .unwrap();

    // User cancels the save prompt from workspace B
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    // Window should still exist because workspace B's close was cancelled
    assert!(
        multi_workspace_handle.update(cx, |_, _, _| ()).is_ok(),
        "window should still exist after cancelling one workspace's close"
    );
}

#[gpui::test]
async fn test_remove_workspace_prompts_for_unsaved_changes(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "one": "" })).await;

    let project_a = Project::test(fs.clone(), ["root".as_ref()], cx).await;
    let project_b = Project::test(fs.clone(), ["root".as_ref()], cx).await;
    let multi_workspace_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    cx.run_until_parked();

    multi_workspace_handle
        .update(cx, |mw, _window, cx| mw.open_sidebar(cx))
        .unwrap();

    let workspace_a = multi_workspace_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.test_add_workspace(project_b, window, cx)
        })
        .unwrap();

    // Activate workspace A.
    multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.activate(workspace_a.clone(), None, window, cx);
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

    // Workspace B has a dirty item.
    let item_b = cx.new(|cx| TestItem::new(cx).with_dirty(true));
    workspace_b.update_in(cx, |w, window, cx| {
        w.add_item_to_active_pane(Box::new(item_b.clone()), None, true, window, cx)
    });

    // Try to remove workspace B. It should prompt because of the dirty item.
    let remove_task = multi_workspace_handle
        .update(cx, |mw, window, cx| {
            mw.remove([workspace_b.clone()], |_, _, _| unreachable!(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();

    // The prompt should have activated workspace B.
    multi_workspace_handle
        .read_with(cx, |mw, _| {
            assert_eq!(
                mw.workspace(),
                &workspace_b,
                "workspace B should be active while prompting"
            );
        })
        .unwrap();

    // Cancel the prompt — user stays on workspace B.
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    let removed = remove_task.await.unwrap();
    assert!(!removed, "removal should have been cancelled");

    multi_workspace_handle
        .read_with(cx, |mw, _cx| {
            assert_eq!(
                mw.workspace(),
                &workspace_b,
                "user should stay on workspace B after cancelling"
            );
            assert_eq!(mw.workspaces().count(), 2, "both workspaces should remain");
        })
        .unwrap();

    // Try again. This time accept the prompt.
    let remove_task = multi_workspace_handle
        .update(cx, |mw, window, cx| {
            // First switch back to A.
            mw.activate(workspace_a.clone(), None, window, cx);
            mw.remove([workspace_b.clone()], |_, _, _| unreachable!(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Accept the save prompt.
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    let removed = remove_task.await.unwrap();
    assert!(removed, "removal should have succeeded");

    // Should be back on workspace A, and B should be gone.
    multi_workspace_handle
        .read_with(cx, |mw, _cx| {
            assert_eq!(
                mw.workspace(),
                &workspace_a,
                "should be back on workspace A after removing B"
            );
            assert_eq!(mw.workspaces().count(), 1, "only workspace A should remain");
        })
        .unwrap();
}
