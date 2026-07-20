use super::*;

#[gpui::test]
async fn test_quit_checks_all_workspaces_for_dirty_items(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(init);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/"),
            json!({
                "dir1": {
                    "a.txt": "content a"
                },
                "dir2": {
                    "b.txt": "content b"
                },
                "dir3": {
                    "c.txt": "content c"
                }
            }),
        )
        .await;

    // === Setup Window 1 with two workspaces ===
    let project1 = Project::test(app_state.fs.clone(), [path!("/dir1").as_ref()], cx).await;
    let window1 = cx.add_window({
        let project = project1.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });
    window1
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    cx.run_until_parked();

    let project2 = Project::test(app_state.fs.clone(), [path!("/dir2").as_ref()], cx).await;
    let workspace1_1 = window1
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    let workspace1_2 = window1
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project2.clone(), window, cx)
        })
        .unwrap();

    window1
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.activate(workspace1_2.clone(), None, window, cx);
            multi_workspace.activate(workspace1_1.clone(), None, window, cx);
        })
        .unwrap();

    // === Setup Window 2 with one workspace ===
    let project3 = Project::test(app_state.fs.clone(), [path!("/dir3").as_ref()], cx).await;
    let window2 = cx.add_window({
        let project = project3.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });
    window2
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 2);

    // === Case 1: Active workspace has dirty item, quit can be cancelled ===
    let worktree1_id = project1.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let editor1 = window1
        .update(cx, |_, window, cx| {
            workspace1_1.update(cx, |workspace, cx| {
                workspace.open_path((worktree1_id, rel_path("a.txt")), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    window1
        .update(cx, |_, window, cx| {
            editor1.update(cx, |editor, cx| {
                editor.insert("dirty in active workspace", window, cx);
            });
        })
        .unwrap();

    cx.run_until_parked();

    // Verify workspace1_1 is active
    window1
        .read_with(cx, |multi_workspace, _| {
            assert_eq!(multi_workspace.workspace(), &workspace1_1);
        })
        .unwrap();

    cx.dispatch_action(*window1, Quit);
    cx.run_until_parked();

    assert!(
        cx.has_pending_prompt(),
        "Case 1: Should prompt to save dirty item in active workspace"
    );

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    assert_eq!(
        cx.windows().len(),
        2,
        "Case 1: Windows should still exist after cancelling quit"
    );

    // Clean up Case 1: Close the dirty item without saving
    let close_task = window1
        .update(cx, |_, window, cx| {
            workspace1_1.update(cx, |workspace, cx| {
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.close_active_item(&Default::default(), window, cx)
                })
            })
        })
        .unwrap();
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    close_task.await.ok();
    cx.run_until_parked();

    // === Case 2: Non-active workspace (same window) has dirty item ===
    let worktree2_id = project2.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let editor2 = window1
        .update(cx, |_, window, cx| {
            workspace1_2.update(cx, |workspace, cx| {
                workspace.open_path((worktree2_id, rel_path("b.txt")), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    window1
        .update(cx, |_, window, cx| {
            editor2.update(cx, |editor, cx| {
                editor.insert("dirty in non-active workspace", window, cx);
            });
        })
        .unwrap();

    cx.run_until_parked();

    // Verify workspace1_1 is still active (not workspace1_2 with dirty item)
    window1
        .read_with(cx, |multi_workspace, _| {
            assert_eq!(multi_workspace.workspace(), &workspace1_1);
        })
        .unwrap();

    cx.dispatch_action(*window1, Quit);
    cx.run_until_parked();

    // Verify the non-active workspace got activated to show the dirty item
    window1
        .read_with(cx, |multi_workspace, _| {
            assert_eq!(
                multi_workspace.workspace(),
                &workspace1_2,
                "Case 2: Non-active workspace should be activated when it has dirty item"
            );
        })
        .unwrap();

    assert!(
        cx.has_pending_prompt(),
        "Case 2: Should prompt to save dirty item in non-active workspace"
    );

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    assert_eq!(
        cx.windows().len(),
        2,
        "Case 2: Windows should still exist after cancelling quit"
    );

    // Clean up Case 2: Close the dirty item without saving
    let close_task = window1
        .update(cx, |_, window, cx| {
            workspace1_2.update(cx, |workspace, cx| {
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.close_active_item(&Default::default(), window, cx)
                })
            })
        })
        .unwrap();
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    close_task.await.ok();
    cx.run_until_parked();

    // === Case 3: Non-active window has dirty item ===
    let workspace3 = window2
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();

    let worktree3_id = project3.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let editor3 = window2
        .update(cx, |_, window, cx| {
            workspace3.update(cx, |workspace, cx| {
                workspace.open_path((worktree3_id, rel_path("c.txt")), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    window2
        .update(cx, |_, window, cx| {
            editor3.update(cx, |editor, cx| {
                editor.insert("dirty in other window", window, cx);
            });
        })
        .unwrap();

    cx.run_until_parked();

    // Activate window1 explicitly (editing in window2 may have activated it)
    window1
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();

    // Verify window2 is not active (window1 should still be active)
    assert_eq!(
        cx.update(|cx| window2.is_active(cx)),
        Some(false),
        "Case 3: window2 should not be active before quit"
    );

    // Dispatch quit from window1 (window2 has the dirty item)
    cx.dispatch_action(*window1, Quit);
    cx.run_until_parked();

    // Verify window2 is now active (quit handler activated it to show dirty item)
    assert_eq!(
        cx.update(|cx| window2.is_active(cx)),
        Some(true),
        "Case 3: window2 should be activated when it has dirty item"
    );

    assert!(
        cx.has_pending_prompt(),
        "Case 3: Should prompt to save dirty item in non-active window"
    );

    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();

    assert_eq!(
        cx.windows().len(),
        2,
        "Case 3: Windows should still exist after cancelling quit"
    );
}
