use super::*;

#[gpui::test]
async fn test_vim_visual_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&(sample_text(6, 6, 'a') + "\n"), cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

    window
        .update(cx, |editor, window, cx| {
            editor.cursor_offset_on_selection = true;
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([
                    Point::new(0, 0)..Point::new(1, 0),
                    Point::new(3, 2)..Point::new(3, 3),
                    Point::new(5, 6)..Point::new(6, 0),
                ]);
            });
        })
        .unwrap();

    let (_, state) = cx.draw(
        point(px(500.), px(500.)),
        size(px(500.), px(500.)),
        |_, _| EditorElement::new(&editor, style),
    );

    assert_eq!(state.selections.len(), 1);
    let local_selections = &state.selections[0].1;
    assert_eq!(local_selections.len(), 3);
    // moves cursor back one line
    assert_eq!(
        local_selections[0].head,
        DisplayPoint::new(DisplayRow(0), 6)
    );
    assert_eq!(
        local_selections[0].range,
        DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(1), 0)
    );

    // moves cursor back one column
    assert_eq!(
        local_selections[1].range,
        DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 3)
    );
    assert_eq!(
        local_selections[1].head,
        DisplayPoint::new(DisplayRow(3), 2)
    );

    // leaves cursor on the max point
    assert_eq!(
        local_selections[2].range,
        DisplayPoint::new(DisplayRow(5), 6)..DisplayPoint::new(DisplayRow(6), 0)
    );
    assert_eq!(
        local_selections[2].head,
        DisplayPoint::new(DisplayRow(6), 0)
    );

    // active lines does not include 1 (even though the range of the selection does)
    assert_eq!(
        state.active_rows.keys().cloned().collect::<Vec<_>>(),
        vec![DisplayRow(0), DisplayRow(3), DisplayRow(5), DisplayRow(6)]
    );
}

#[gpui::test]
fn test_layout_with_placeholder_text_and_blocks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("", cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
    window
        .update(cx, |editor, window, cx| {
            editor.set_placeholder_text("hello", window, cx);
            editor.insert_blocks(
                [BlockProperties {
                    style: BlockStyle::Fixed,
                    placement: BlockPlacement::Above(Anchor::Min),
                    height: Some(3),
                    render: Arc::new(|cx| div().h(3. * cx.window.line_height()).into_any()),
                    priority: 0,
                }],
                None,
                cx,
            );

            // Blur the editor so that it displays placeholder text.
            window.blur();
        })
        .unwrap();

    let (_, state) = cx.draw(
        point(px(500.), px(500.)),
        size(px(500.), px(500.)),
        |_, _| EditorElement::new(&editor, style),
    );
    assert_eq!(state.position_map.line_layouts.len(), 4);
    assert_eq!(state.line_numbers.len(), 1);
    assert_eq!(
        state
            .line_numbers
            .get(&MultiBufferRow(0))
            .map(|line_number| line_number
                .segments
                .first()
                .unwrap()
                .shaped_line
                .text
                .as_ref()),
        Some("1")
    );
}
