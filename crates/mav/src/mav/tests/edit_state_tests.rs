use super::*;

#[gpui::test]
async fn test_window_edit_state_restoring_disabled(cx: &mut TestAppContext) {
    let executor = cx.executor();
    let app_state = init_test(cx);

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .session
                    .get_or_insert_default()
                    .restore_unsaved_buffers = Some(false)
            });
        });
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({"a": "hey"}))
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // When opening the workspace, the window is not in a edited state.
    let window = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());

    let window_is_edited = |window: WindowHandle<MultiWorkspace>, cx: &mut TestAppContext| {
        cx.update(|cx| window.read(cx).unwrap().workspace().read(cx).is_edited())
    };
    let pane = window
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace.workspace().read(cx).active_pane().clone()
        })
        .unwrap();
    let editor = window
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_item(cx)
                .unwrap()
                .downcast::<Editor>()
                .unwrap()
        })
        .unwrap();

    assert!(!window_is_edited(window, cx));

    // Editing a buffer marks the window as edited.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.insert("EDIT", window, cx));
        })
        .unwrap();

    assert!(window_is_edited(window, cx));

    // Undoing the edit restores the window's edited state.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.undo(&Default::default(), window, cx)
            });
        })
        .unwrap();
    assert!(!window_is_edited(window, cx));

    // Redoing the edit marks the window as edited again.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.redo(&Default::default(), window, cx)
            });
        })
        .unwrap();
    assert!(window_is_edited(window, cx));
    let weak = editor.downgrade();

    // Closing the item restores the window's edited state.
    let close = window
        .update(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                drop(editor);
                pane.close_active_item(&Default::default(), window, cx)
            })
        })
        .unwrap();
    executor.run_until_parked();

    cx.simulate_prompt_answer("Don't Save");
    close.await.unwrap();

    // Advance the clock to ensure that the item has been serialized and dropped from the queue
    cx.executor().advance_clock(Duration::from_secs(1));

    weak.assert_released();
    assert!(!window_is_edited(window, cx));
    // Opening the buffer again doesn't impact the window's edited state.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    executor.run_until_parked();

    window
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let editor = workspace
                    .active_item(cx)
                    .unwrap()
                    .downcast::<Editor>()
                    .unwrap();

                editor.update(cx, |editor, cx| {
                    assert_eq!(editor.text(cx), "hey");
                });
            });
        })
        .unwrap();

    let editor = window
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_item(cx)
                .unwrap()
                .downcast::<Editor>()
                .unwrap()
        })
        .unwrap();
    assert!(!window_is_edited(window, cx));

    // Editing the buffer marks the window as edited.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.insert("EDIT", window, cx));
        })
        .unwrap();
    executor.run_until_parked();
    assert!(window_is_edited(window, cx));

    // Ensure closing the window via the mouse gets preempted due to the
    // buffer having unsaved changes.
    assert!(!VisualTestContext::from_window(window.into(), cx).simulate_close());
    executor.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // The window is successfully closed after the user dismisses the prompt.
    cx.simulate_prompt_answer("Don't Save");
    executor.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 0);
}

#[ignore = "This test has timing issues across platforms."]
#[gpui::test]
async fn test_window_edit_state_restoring_enabled(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({"a": "hey"}))
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // When opening the workspace, the window is not in a edited state.
    let window = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());

    let window_is_edited = |window: WindowHandle<MultiWorkspace>, cx: &mut TestAppContext| {
        cx.update(|cx| window.read(cx).unwrap().workspace().read(cx).is_edited())
    };
    let workspace_database_id = |window: WindowHandle<MultiWorkspace>, cx: &mut TestAppContext| {
        cx.update(|cx| window.read(cx).unwrap().workspace().read(cx).database_id())
    };

    let editor = window
        .read_with(cx, |multi_workspace, cx| {
            multi_workspace
                .workspace()
                .read(cx)
                .active_item(cx)
                .unwrap()
                .downcast::<Editor>()
                .unwrap()
        })
        .unwrap();

    assert!(!window_is_edited(window, cx));
    let initial_database_id = workspace_database_id(window, cx);
    assert!(
        initial_database_id.is_some(),
        "a restored workspace must have a stable database id"
    );

    // Editing a buffer marks the window as edited.
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.insert("EDIT", window, cx));
        })
        .unwrap();
    cx.run_until_parked();

    assert!(window_is_edited(window, cx));

    // Advance the clock to make sure the workspace is serialized
    cx.executor().advance_clock(Duration::from_secs(1));

    // When closing the window, no prompt shows up and the window is closed.
    // buffer having unsaved changes.
    assert!(!VisualTestContext::from_window(window.into(), cx).simulate_close());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 0);

    // When we now reopen the window, the edited state and the edited buffer are back
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    assert!(cx.update(|cx| cx.active_window().is_some()));

    cx.run_until_parked();

    // When opening the workspace, the window is not in a edited state.
    let window = cx.update(|cx| {
        cx.active_window()
            .unwrap()
            .downcast::<MultiWorkspace>()
            .unwrap()
    });
    assert!(window_is_edited(window, cx));
    assert_eq!(
        workspace_database_id(window, cx),
        initial_database_id,
        "the workspace must keep the same database id across a close/reopen cycle"
    );

    window
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let editor = workspace
                    .active_item(cx)
                    .unwrap()
                    .downcast::<editor::Editor>()
                    .unwrap();
                editor.update(cx, |editor, cx| {
                    assert_eq!(editor.text(cx), "EDIThey");
                    assert!(editor.is_dirty(cx));
                });
            });
        })
        .unwrap();
}
