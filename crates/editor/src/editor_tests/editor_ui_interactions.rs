use super::*;

#[gpui::test]
fn test_crease_insertion_and_rendering(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\nddddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    let render_args = Arc::new(Mutex::new(None));
    let snapshot = editor
        .update(cx, |editor, window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let range =
                snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(2, 6));

            struct RenderArgs {
                row: MultiBufferRow,
                folded: bool,
                callback: Arc<dyn Fn(bool, &mut Window, &mut App) + Send + Sync>,
            }

            let crease = Crease::inline(
                range,
                FoldPlaceholder::test(),
                {
                    let toggle_callback = render_args.clone();
                    move |row, folded, callback, _window, _cx| {
                        *toggle_callback.lock() = Some(RenderArgs {
                            row,
                            folded,
                            callback,
                        });
                        div()
                    }
                },
                |_row, _folded, _window, _cx| div(),
            );

            editor.insert_creases(Some(crease), cx);
            let snapshot = editor.snapshot(window, cx);
            let _div =
                snapshot.render_crease_toggle(MultiBufferRow(1), false, cx.entity(), window, cx);
            snapshot
        })
        .unwrap();

    let render_args = render_args.lock().take().unwrap();
    assert_eq!(render_args.row, MultiBufferRow(1));
    assert!(!render_args.folded);
    assert!(!snapshot.is_line_folded(MultiBufferRow(1)));

    cx.update_window(*editor, |_, window, cx| {
        (render_args.callback)(true, window, cx)
    })
    .unwrap();
    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    assert!(snapshot.is_line_folded(MultiBufferRow(1)));

    cx.update_window(*editor, |_, window, cx| {
        (render_args.callback)(false, window, cx)
    })
    .unwrap();
    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    assert!(!snapshot.is_line_folded(MultiBufferRow(1)));
}

#[gpui::test]
async fn test_input_text(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(
        &r#"ˇone
        two

        three
        fourˇ
        five

        siˇx"#
            .unindent(),
    );

    cx.dispatch_action(HandleInput(String::new()));
    cx.assert_editor_state(
        &r#"ˇone
        two

        three
        fourˇ
        five

        siˇx"#
            .unindent(),
    );

    cx.dispatch_action(HandleInput("AAAA".to_string()));
    cx.assert_editor_state(
        &r#"AAAAˇone
        two

        three
        fourAAAAˇ
        five

        siAAAAˇx"#
            .unindent(),
    );
}

#[gpui::test]
async fn test_scroll_cursor_center_top_bottom(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        r#"let foo = 1;
let foo = 2;
let foo = 3;
let fooˇ = 4;
let foo = 5;
let foo = 6;
let foo = 7;
let foo = 8;
let foo = 9;
let foo = 10;
let foo = 11;
let foo = 12;
let foo = 13;
let foo = 14;
let foo = 15;"#,
    );

    cx.update_editor(|e, window, cx| {
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "Default next scroll direction is center",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Top,
            "After center, next scroll direction should be top",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Bottom,
            "After top, next scroll direction should be bottom",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "After bottom, scrolling should start over",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Top,
            "Scrolling continues if retriggered fast enough"
        );
    });

    cx.executor()
        .advance_clock(SCROLL_CENTER_TOP_BOTTOM_DEBOUNCE_TIMEOUT + Duration::from_millis(200));
    cx.executor().run_until_parked();
    cx.update_editor(|e, _, _| {
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "If scrolling is not triggered fast enough, it should reset"
        );
    });
}
