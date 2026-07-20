use super::*;

#[gpui::test]
async fn test_reopen_last_picker(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, cx) = cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

    // A non-reopenable modal is dropped on dismissal and cannot be revealed.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, TestModal::new);
    });
    cx.executor().run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.active_modal::<TestModal>(cx).is_some()
    }));
    workspace.update_in(cx, |workspace, window, cx| {
        let revealed = workspace.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.hide_modal(window, cx);
            modal_layer.reveal_stashed_modal(window, cx)
        });
        assert!(!revealed, "a non-reopenable modal should not be revealable");
    });

    // A reopenable modal is stashed on dismissal and revealed as the *same*
    // entity, so its prior state is preserved exactly.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, ReopenableTestModal::new);
    });
    cx.executor().run_until_parked();
    let original_id = workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<ReopenableTestModal>(cx)
            .unwrap()
            .entity_id()
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx));
    });
    cx.executor().run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.active_modal::<ReopenableTestModal>(cx).is_none()
    }));
    workspace.update_in(cx, |workspace, window, cx| {
        let revealed = workspace.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.reveal_stashed_modal(window, cx)
        });
        assert!(revealed, "a reopenable modal should be revealable");
    });
    cx.executor().run_until_parked();
    let revealed_id = workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<ReopenableTestModal>(cx)
            .unwrap()
            .entity_id()
    });
    assert_eq!(
        original_id, revealed_id,
        "reveal should restore the same modal entity rather than building a new one"
    );

    workspace.update_in(cx, |workspace, window, cx| {
        let revealed = workspace.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.reveal_stashed_modal(window, cx)
        });
        assert!(!revealed, "reveal should be a no-op while a modal is open");
    });

    // A non-reopenable modal must not discard the stash, which is what lets the
    // command palette be used to trigger the reopen.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx));
        workspace.toggle_modal(window, cx, TestModal::new);
    });
    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        let revealed = workspace.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.hide_modal(window, cx);
            modal_layer.reveal_stashed_modal(window, cx)
        });
        assert!(
            revealed,
            "a non-reopenable modal must not discard the stash"
        );
    });
    cx.executor().run_until_parked();

    // Reopen triggered from within a modal that dismisses asynchronously and
    // dispatches the action in the same cycle, as the command palette does.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx));
        workspace.toggle_modal(window, cx, TestModal::new);
    });
    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        // Mirror the command palette's confirm: emit DismissEvent on itself and
        // dispatch the reopen action within the same update.
        let palette = workspace.active_modal::<TestModal>(cx).unwrap();
        palette.update(cx, |_, cx| cx.emit(DismissEvent));
        workspace.reopen_last_picker(&ReopenLastPicker, window, cx);
    });
    cx.executor().run_until_parked();
    assert!(
        workspace.read_with(cx, |workspace, cx| workspace
            .active_modal::<ReopenableTestModal>(cx)
            .is_some()),
        "reopen triggered from within a dismissing modal should reveal the stash"
    );
}

#[gpui::test]
async fn test_panels(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let (panel_1, panel_2) = workspace.update_in(cx, |workspace, window, cx| {
        let panel_1 = cx.new(|cx| TestPanel::new(DockPosition::Left, 100, cx));
        workspace.add_panel(panel_1.clone(), window, cx);
        workspace.toggle_dock(DockPosition::Left, window, cx);
        let panel_2 = cx.new(|cx| TestPanel::new(DockPosition::Right, 101, cx));
        workspace.add_panel(panel_2.clone(), window, cx);
        workspace.toggle_dock(DockPosition::Right, window, cx);

        let left_dock = workspace.left_dock();
        assert_eq!(
            left_dock.read(cx).visible_panel().unwrap().panel_id(),
            panel_1.panel_id()
        );
        assert_eq!(
            workspace.dock_size(&left_dock.read(cx), window, cx),
            Some(px(300.))
        );

        workspace.resize_left_dock(px(1337.), window, cx);
        assert_eq!(
            workspace
                .right_dock()
                .read(cx)
                .visible_panel()
                .unwrap()
                .panel_id(),
            panel_2.panel_id(),
        );

        (panel_1, panel_2)
    });

    // Move panel_1 to the right
    panel_1.update_in(cx, |panel_1, window, cx| {
        panel_1.set_position(DockPosition::Right, window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        // Since panel_1 was visible on the left, it should now be visible now that it's been moved to the right.
        // Since it was the only panel on the left, the left dock should now be closed.
        assert!(!workspace.left_dock().read(cx).is_open());
        assert!(workspace.left_dock().read(cx).visible_panel().is_none());
        let right_dock = workspace.right_dock();
        assert_eq!(
            right_dock.read(cx).visible_panel().unwrap().panel_id(),
            panel_1.panel_id()
        );
        assert_eq!(
            right_dock
                .read(cx)
                .active_panel_size()
                .unwrap()
                .size
                .unwrap(),
            px(1337.)
        );

        // Now we move panel_2 to the left
        panel_2.set_position(DockPosition::Left, window, cx);
    });

    workspace.update(cx, |workspace, cx| {
        // Since panel_2 was not visible on the right, we don't open the left dock.
        assert!(!workspace.left_dock().read(cx).is_open());
        // And the right dock is unaffected in its displaying of panel_1
        assert!(workspace.right_dock().read(cx).is_open());
        assert_eq!(
            workspace
                .right_dock()
                .read(cx)
                .visible_panel()
                .unwrap()
                .panel_id(),
            panel_1.panel_id(),
        );
    });

    // Move panel_1 back to the left
    panel_1.update_in(cx, |panel_1, window, cx| {
        panel_1.set_position(DockPosition::Left, window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        // Since panel_1 was visible on the right, we open the left dock and make panel_1 active.
        let left_dock = workspace.left_dock();
        assert!(left_dock.read(cx).is_open());
        assert_eq!(
            left_dock.read(cx).visible_panel().unwrap().panel_id(),
            panel_1.panel_id()
        );
        assert_eq!(
            workspace.dock_size(&left_dock.read(cx), window, cx),
            Some(px(1337.))
        );
        // And the right dock should be closed as it no longer has any panels.
        assert!(!workspace.right_dock().read(cx).is_open());
    });

    // Emit activated event on panel 1
    panel_1.update(cx, |_, cx| cx.emit(PanelEvent::Activate));

    // Now the left dock is open and panel_1 is active and focused.
    workspace.update_in(cx, |workspace, window, cx| {
        let left_dock = workspace.left_dock();
        assert!(left_dock.read(cx).is_open());
        assert_eq!(
            left_dock.read(cx).visible_panel().unwrap().panel_id(),
            panel_1.panel_id(),
        );
        assert!(panel_1.focus_handle(cx).is_focused(window));
    });

    // Emit closed event on panel 2, which is not active
    panel_2.update(cx, |_, cx| cx.emit(PanelEvent::Close));

    // Wo don't close the left dock, because panel_2 wasn't the active panel
    workspace.update(cx, |workspace, cx| {
        let left_dock = workspace.left_dock();
        assert!(left_dock.read(cx).is_open());
        assert_eq!(
            left_dock.read(cx).visible_panel().unwrap().panel_id(),
            panel_1.panel_id(),
        );
    });

    // Emitting a ZoomIn event shows the panel as zoomed.
    panel_1.update(cx, |_, cx| cx.emit(PanelEvent::ZoomIn));
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, Some(panel_1.to_any().downgrade()));
        assert_eq!(workspace.zoomed_position, Some(DockPosition::Left));
    });

    // Move panel to another dock while it is zoomed
    panel_1.update_in(cx, |panel, window, cx| {
        panel.set_position(DockPosition::Right, window, cx)
    });
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, Some(panel_1.to_any().downgrade()));

        assert_eq!(workspace.zoomed_position, Some(DockPosition::Right));
    });

    // This is a helper for getting a:
    // - valid focus on an element,
    // - that isn't a part of the panes and panels system of the Workspace,
    // - and doesn't trigger the 'on_focus_lost' API.
    let focus_other_view = {
        let workspace = workspace.clone();
        move |cx: &mut VisualTestContext| {
            workspace.update_in(cx, |workspace, window, cx| {
                if workspace.active_modal::<TestModal>(cx).is_some() {
                    workspace.toggle_modal(window, cx, TestModal::new);
                    workspace.toggle_modal(window, cx, TestModal::new);
                } else {
                    workspace.toggle_modal(window, cx, TestModal::new);
                }
            })
        }
    };

    // If focus is transferred to another view that's not a panel or another pane, we still show
    // the panel as zoomed.
    focus_other_view(cx);
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, Some(panel_1.to_any().downgrade()));
        assert_eq!(workspace.zoomed_position, Some(DockPosition::Right));
    });

    // If focus is transferred elsewhere in the workspace, the panel is no longer zoomed.
    workspace.update_in(cx, |_workspace, window, cx| {
        cx.focus_self(window);
    });
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, None);
        assert_eq!(workspace.zoomed_position, None);
    });

    // If focus is transferred again to another view that's not a panel or a pane, we won't
    // show the panel as zoomed because it wasn't zoomed before.
    focus_other_view(cx);
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, None);
        assert_eq!(workspace.zoomed_position, None);
    });

    // When the panel is activated, it is zoomed again.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_dock(DockPosition::Right, window, cx);
    });
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, Some(panel_1.to_any().downgrade()));
        assert_eq!(workspace.zoomed_position, Some(DockPosition::Right));
    });

    // Emitting a ZoomOut event unzooms the panel.
    panel_1.update(cx, |_, cx| cx.emit(PanelEvent::ZoomOut));
    workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.zoomed, None);
        assert_eq!(workspace.zoomed_position, None);
    });

    // Emit closed event on panel 1, which is active
    panel_1.update(cx, |_, cx| cx.emit(PanelEvent::Close));

    // Now the left dock is closed, because panel_1 was the active panel
    workspace.update(cx, |workspace, cx| {
        let right_dock = workspace.right_dock();
        assert!(!right_dock.read(cx).is_open());
    });
}
