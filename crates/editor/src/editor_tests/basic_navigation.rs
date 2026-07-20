use super::*;

#[gpui::test]
fn test_clone(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (text, selection_ranges) = marked_text_ranges(
        indoc! {"
            one
            two
            threeˇ
            four
            fiveˇ
        "},
        true,
    );

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&text, cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(
                selection_ranges
                    .iter()
                    .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end)),
            )
        });
        editor.fold_creases(
            vec![
                Crease::simple(Point::new(1, 0)..Point::new(2, 0), FoldPlaceholder::test()),
                Crease::simple(Point::new(3, 0)..Point::new(4, 0), FoldPlaceholder::test()),
            ],
            true,
            window,
            cx,
        );
    });

    let cloned_editor = editor
        .update(cx, |editor, _, cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| editor.clone(window, cx))
            })
        })
        .unwrap()
        .unwrap();

    let snapshot = editor
        .update(cx, |e, window, cx| e.snapshot(window, cx))
        .unwrap();
    let cloned_snapshot = cloned_editor
        .update(cx, |e, window, cx| e.snapshot(window, cx))
        .unwrap();

    assert_eq!(
        cloned_editor
            .update(cx, |e, _, cx| e.display_text(cx))
            .unwrap(),
        editor.update(cx, |e, _, cx| e.display_text(cx)).unwrap()
    );
    assert_eq!(
        cloned_snapshot
            .folds_in_range(MultiBufferOffset(0)..MultiBufferOffset(text.len()))
            .collect::<Vec<_>>(),
        snapshot
            .folds_in_range(MultiBufferOffset(0)..MultiBufferOffset(text.len()))
            .collect::<Vec<_>>(),
    );
    assert_set_eq!(
        cloned_editor
            .update(cx, |editor, _, cx| editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)))
            .unwrap(),
        editor
            .update(cx, |editor, _, cx| editor
                .selections
                .ranges(&editor.display_snapshot(cx)))
            .unwrap()
    );
    assert_set_eq!(
        cloned_editor
            .update(cx, |e, _window, cx| e
                .selections
                .display_ranges(&e.display_snapshot(cx)))
            .unwrap(),
        editor
            .update(cx, |e, _, cx| e
                .selections
                .display_ranges(&e.display_snapshot(cx)))
            .unwrap()
    );
}

#[gpui::test]
fn test_toggle_breadcrumb_does_not_change_settings(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.toolbar.get_or_insert_default().breadcrumbs = Some(true);
    });

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("hello", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        assert!(EditorSettings::get_global(cx).toolbar.breadcrumbs);
        assert_eq!(
            editor.breadcrumb_location(cx),
            ToolbarItemLocation::PrimaryLeft
        );

        editor.toggle_breadcrumb(&ToggleBreadcrumb, window, cx);
        assert!(EditorSettings::get_global(cx).toolbar.breadcrumbs);
        assert_eq!(editor.breadcrumb_location(cx), ToolbarItemLocation::Hidden);
    });

    // Changing unrelated settings should not affect breadcrumbs visibility.
    update_test_editor_settings(cx, &|settings| {
        settings.vertical_scroll_margin = Some(4.0);
    });
    cx.run_until_parked();

    _ = editor.update(cx, |editor, window, cx| {
        assert!(EditorSettings::get_global(cx).toolbar.breadcrumbs);
        assert_eq!(editor.breadcrumb_location(cx), ToolbarItemLocation::Hidden);

        editor.toggle_breadcrumb(&ToggleBreadcrumb, window, cx);
        assert!(EditorSettings::get_global(cx).toolbar.breadcrumbs);
        assert_eq!(
            editor.breadcrumb_location(cx),
            ToolbarItemLocation::PrimaryLeft
        );
    });
}

#[gpui::test]
async fn test_navigation_history(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    use workspace::item::Item;

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    _ = window.update(cx, |_mw, window, cx| {
        cx.new(|cx| {
            let buffer = MultiBuffer::build_simple(&sample_text(300, 5, 'a'), cx);
            let mut editor = build_editor(buffer, window, cx);
            let handle = cx.entity();
            editor.set_nav_history(Some(pane.read(cx).nav_history_for_item(&handle)));

            fn pop_history(editor: &mut Editor, cx: &mut App) -> Option<NavigationEntry> {
                editor.nav_history.as_mut().unwrap().pop_backward(cx)
            }

            // Move the cursor a small distance.
            // Nothing is added to the navigation history.
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
                ])
            });
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0)
                ])
            });
            assert!(pop_history(&mut editor, cx).is_none());

            // Move the cursor a large distance.
            // The history can jump back to the previous position.
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(13), 0)..DisplayPoint::new(DisplayRow(13), 3)
                ])
            });
            let nav_entry = pop_history(&mut editor, cx).unwrap();
            editor.navigate(nav_entry.data.unwrap(), window, cx);
            assert_eq!(nav_entry.item.id(), cx.entity_id());
            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                &[DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0)]
            );
            assert!(pop_history(&mut editor, cx).is_none());

            // Move the cursor a small distance via the mouse.
            // Nothing is added to the navigation history.
            editor.begin_selection(DisplayPoint::new(DisplayRow(5), 0), false, 1, window, cx);
            editor.end_selection(window, cx);
            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                &[DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 0)]
            );
            assert!(pop_history(&mut editor, cx).is_none());

            // Move the cursor a large distance via the mouse.
            // The history can jump back to the previous position.
            editor.begin_selection(DisplayPoint::new(DisplayRow(15), 0), false, 1, window, cx);
            editor.end_selection(window, cx);
            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                &[DisplayPoint::new(DisplayRow(15), 0)..DisplayPoint::new(DisplayRow(15), 0)]
            );
            let nav_entry = pop_history(&mut editor, cx).unwrap();
            editor.navigate(nav_entry.data.unwrap(), window, cx);
            assert_eq!(nav_entry.item.id(), cx.entity_id());
            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                &[DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 0)]
            );
            assert!(pop_history(&mut editor, cx).is_none());

            // Set scroll position to check later
            editor.set_scroll_position(gpui::Point::<f64>::new(5.5, 5.5), window, cx);
            let original_scroll_position = editor
                .scroll_manager
                .native_anchor(&editor.display_snapshot(cx), cx);

            // Jump to the end of the document and adjust scroll
            editor.move_to_end(&MoveToEnd, window, cx);
            editor.set_scroll_position(gpui::Point::<f64>::new(-2.5, -0.5), window, cx);
            assert_ne!(
                editor
                    .scroll_manager
                    .native_anchor(&editor.display_snapshot(cx), cx),
                original_scroll_position
            );

            let nav_entry = pop_history(&mut editor, cx).unwrap();
            editor.navigate(nav_entry.data.unwrap(), window, cx);
            assert_eq!(
                editor
                    .scroll_manager
                    .native_anchor(&editor.display_snapshot(cx), cx),
                original_scroll_position
            );

            let other_buffer =
                cx.new(|cx| MultiBuffer::singleton(cx.new(|cx| Buffer::local("test", cx)), cx));

            // Ensure we don't panic when navigation data contains invalid anchors *and* points.
            let invalid_anchor = other_buffer.update(cx, |buffer, cx| {
                buffer.snapshot(cx).anchor_after(MultiBufferOffset(3))
            });
            let invalid_point = Point::new(9999, 0);
            editor.navigate(
                Arc::new(NavigationData {
                    cursor_anchor: invalid_anchor,
                    cursor_position: invalid_point,
                    scroll_anchor: ScrollAnchor {
                        anchor: invalid_anchor,
                        offset: Default::default(),
                    },
                    scroll_top_row: invalid_point.row,
                }),
                window,
                cx,
            );
            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                &[editor.max_point(cx)..editor.max_point(cx)]
            );
            assert_eq!(
                editor.scroll_position(cx),
                gpui::Point::new(0., editor.max_point(cx).row().as_f64())
            );

            editor
        })
    });
}

#[gpui::test]
fn test_cancel(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\ndddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 4), false, 1, window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(1), 1),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        editor.end_selection(window, cx);

        editor.begin_selection(DisplayPoint::new(DisplayRow(0), 1), true, 1, window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(0), 3),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(3), 4)..DisplayPoint::new(DisplayRow(1), 1),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.cancel(&Cancel, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 4)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.cancel(&Cancel, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });
}
