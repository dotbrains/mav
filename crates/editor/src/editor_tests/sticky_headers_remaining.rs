use super::*;

#[gpui::test]
async fn test_no_duplicated_sticky_headers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        ˇimpl Foo { fn bar() {
            let x = 1;
            fn baz() {
                let y = 2;
            }
        } }
    "});

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|e, window, cx| {
            EditorElement::sticky_headers(&e, &e.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    let struct_foo = Point { row: 0, column: 0 };
    let fn_baz = Point { row: 2, column: 4 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(1.5), vec![(struct_foo, 0.0), (fn_baz, 1.0)]);
    assert_eq!(sticky_headers(2.0), vec![(struct_foo, 0.0), (fn_baz, 1.0)]);
    assert_eq!(sticky_headers(2.5), vec![(struct_foo, 0.0), (fn_baz, 0.5)]);
    assert_eq!(sticky_headers(3.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(3.5), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(4.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(4.5), vec![(struct_foo, -0.5)]);
    assert_eq!(sticky_headers(5.0), vec![]);
}

#[gpui::test]
async fn test_autoscroll_keeps_cursor_visible_below_sticky_headers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.vertical_scroll_margin = Some(0.0);
        settings.scroll_beyond_last_line = Some(ScrollBeyondLastLine::OnePage);
        settings.sticky_scroll = Some(settings::StickyScrollContent {
            enabled: Some(true),
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        impl Foo { fn bar() {
            let x = 1;
            fn baz() {
                let y = 2;
            }
        } }
        ˇ
    "});

    let mut previous_cursor_row = cx.update_editor(|editor, window, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| buffer.set_language(Some(rust_lang()), cx));
        let cursor_row = editor
            .selections
            .newest_display(&editor.display_snapshot(cx))
            .head()
            .row();
        editor.set_scroll_top_row(cursor_row, window, cx);
        cursor_row
    });

    for _ in 0..6 {
        cx.update_editor(|editor, window, cx| editor.move_up(&MoveUp, window, cx));
        cx.run_until_parked();

        cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            let scroll_top = snapshot.scroll_position().y;
            let sticky_header_count = EditorElement::sticky_headers(editor, &snapshot).len();
            let cursor_row = editor
                .selections
                .newest_display(&snapshot.display_snapshot)
                .head()
                .row();
            assert_eq!(
                cursor_row,
                previous_cursor_row
                    .previous_row()
                    .max(DisplayRow(scroll_top as u32) + DisplayRow(sticky_header_count as u32))
            );
            previous_cursor_row = cursor_row;
        });

        // The `ScrollCursorTop` action shouldn't change the scroll position, as the cursor is
        // already as high up as the sticky headers allow.
        let scroll_top_before =
            cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).scroll_position().y);
        cx.update_editor(|editor, window, cx| {
            editor.scroll_cursor_top(&ScrollCursorTop, window, cx)
        });
        cx.run_until_parked();
        let scroll_top_after =
            cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).scroll_position().y);
        assert_eq!(scroll_top_before, scroll_top_after);
    }
}

#[gpui::test]
fn test_relative_line_numbers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaaaaaaaaa\nbbb\n", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("cccccccccc\nddd\n", cx));
    let buffer_3 = cx.new(|cx| Buffer::local("eee\nffffffffff\n", cx));

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer
    });

    // wrapped contents of multibuffer:
    //    aaa
    //    aaa
    //    aaa
    //    a
    //    bbb
    //
    //    ccc
    //    ccc
    //    ccc
    //    c
    //    ddd
    //
    //    eee
    //    fff
    //    fff
    //    fff
    //    f

    let editor = cx.add_window(|window, cx| build_editor(multibuffer, window, cx));
    _ = editor.update(cx, |editor, window, cx| {
        editor.set_wrap_width(Some(30.0.into()), cx); // every 3 characters

        // includes trailing newlines.
        let expected_line_numbers = [2, 6, 7, 10, 14, 15, 18, 19, 23];
        let expected_wrapped_line_numbers = [
            2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 15, 18, 19, 20, 21, 22, 23,
        ];

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(7, 0)..Point::new(7, 1), // second row of `ccc`
            ]);
        });

        let snapshot = editor.snapshot(window, cx);

        // these are all 0-indexed
        let base_display_row = DisplayRow(11);
        let base_row = 3;
        let wrapped_base_row = 7;

        // test not counting wrapped lines
        let expected_relative_numbers = expected_line_numbers
            .into_iter()
            .enumerate()
            .map(|(i, row)| (DisplayRow(row), i.abs_diff(base_row) as u32))
            .filter(|(_, relative_line_number)| *relative_line_number != 0)
            .collect_vec();
        let actual_relative_numbers = snapshot
            .calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(24)),
                base_display_row,
                false,
            )
            .into_iter()
            .sorted()
            .collect_vec();
        assert_eq!(expected_relative_numbers, actual_relative_numbers);
        // check `calculate_relative_line_numbers()` against `relative_line_delta()` for each line
        for (display_row, relative_number) in expected_relative_numbers {
            assert_eq!(
                relative_number,
                snapshot
                    .relative_line_delta(display_row, base_display_row, false)
                    .unsigned_abs() as u32,
            );
        }

        // test counting wrapped lines
        let expected_wrapped_relative_numbers = expected_wrapped_line_numbers
            .into_iter()
            .enumerate()
            .map(|(i, row)| (DisplayRow(row), i.abs_diff(wrapped_base_row) as u32))
            .filter(|(row, _)| *row != base_display_row)
            .collect_vec();
        let actual_relative_numbers = snapshot
            .calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(24)),
                base_display_row,
                true,
            )
            .into_iter()
            .sorted()
            .collect_vec();
        assert_eq!(expected_wrapped_relative_numbers, actual_relative_numbers);
        // check `calculate_relative_line_numbers()` against `relative_wrapped_line_delta()` for each line
        for (display_row, relative_number) in expected_wrapped_relative_numbers {
            assert_eq!(
                relative_number,
                snapshot
                    .relative_line_delta(display_row, base_display_row, true)
                    .unsigned_abs() as u32,
            );
        }
    });
}
