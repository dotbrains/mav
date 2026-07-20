use super::*;

#[gpui::test]
async fn test_pane_actions(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1": "contents 1",
                    "file2": "contents 2",
                    "file3": "contents 3",
                },
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let entries = cx.read(|cx| workspace.file_project_paths(cx));
    let file1 = entries[0].clone();

    let pane_1 = cx.read(|cx| workspace.read(cx).active_pane().clone());

    workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file1.clone(), None, true, window, cx)
        })
        .await
        .unwrap();

    let (editor_1, buffer) = workspace.update_in(cx, |_, window, cx| {
        pane_1.update(cx, |pane_1, cx| {
            let editor = pane_1.active_item().unwrap().downcast::<Editor>().unwrap();
            assert_eq!(editor.read(cx).active_project_path(cx), Some(file1.clone()));
            let buffer = editor.update(cx, |editor, cx| {
                editor.insert("dirt", window, cx);
                editor.buffer().downgrade()
            });
            (editor.downgrade(), buffer)
        })
    });

    cx.dispatch_action(pane::SplitRight::default());
    let editor_2 = cx.update(|_, cx| {
        let pane_2 = workspace.read(cx).active_pane().clone();
        assert_ne!(pane_1, pane_2);

        let pane2_item = pane_2.read(cx).active_item().unwrap();
        assert_eq!(pane2_item.project_path(cx), Some(file1.clone()));

        pane2_item.downcast::<Editor>().unwrap().downgrade()
    });
    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });

    cx.background_executor.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.panes().len(), 1);
        assert_eq!(workspace.active_pane(), &pane_1);
    });

    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });
    cx.background_executor.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.background_executor.run_until_parked();

    workspace.read_with(cx, |workspace, cx| {
        assert_eq!(workspace.panes().len(), 1);
        assert!(workspace.active_item(cx).is_none());
    });

    cx.background_executor
        .advance_clock(SERIALIZATION_THROTTLE_TIME);
    cx.update(|_, _| {});
    editor_1.assert_released();
    editor_2.assert_released();
    buffer.assert_released();
}

#[gpui::test]
async fn test_editor_zoom_with_scroll_wheel(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({ "file.txt": "hello\nworld\n" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let mouse_position = point(px(250.), px(250.));

    let event_modifiers = {
        #[cfg(target_os = "macos")]
        {
            Modifiers {
                platform: true,
                ..Modifiers::default()
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        }
    };

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/file.txt")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx.update(|window, cx| {
        window.draw(cx).clear();
    });

    // mouse_wheel_zoom is disabled by default — zoom should not work.
    let initial_font_size =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: mouse_position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(1.))),
        modifiers: event_modifiers,
        ..Default::default()
    });

    let font_size_after_disabled_zoom =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    assert_eq!(
        initial_font_size, font_size_after_disabled_zoom,
        "Editor buffer font-size should not change when mouse_wheel_zoom is disabled"
    );

    // Enable mouse_wheel_zoom and verify zoom works.
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.mouse_wheel_zoom = Some(true);
            });
        });
    });

    cx.update(|window, cx| {
        window.draw(cx).clear();
    });

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: mouse_position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(1.))),
        modifiers: event_modifiers,
        ..Default::default()
    });

    let increased_font_size =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    assert!(
        increased_font_size > initial_font_size,
        "Editor buffer font-size should have increased from scroll-zoom"
    );

    cx.update(|window, cx| {
        window.draw(cx).clear();
    });

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: mouse_position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-1.))),
        modifiers: event_modifiers,
        ..Default::default()
    });

    let decreased_font_size =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    assert!(
        decreased_font_size < increased_font_size,
        "Editor buffer font-size should have decreased from scroll-zoom"
    );

    // Disable mouse_wheel_zoom again and verify zoom stops working.
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.mouse_wheel_zoom = Some(false);
            });
        });
    });

    let font_size_before =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    cx.update(|window, cx| {
        window.draw(cx).clear();
    });

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: mouse_position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(1.))),
        modifiers: event_modifiers,
        ..Default::default()
    });

    let font_size_after =
        cx.update(|_, cx| ThemeSettings::get_global(cx).buffer_font_size(cx).as_f32());

    assert_eq!(
        font_size_before, font_size_after,
        "Editor buffer font-size should not change when mouse_wheel_zoom is re-disabled"
    );
}
