use super::*;

#[gpui::test]
async fn test_soft_wrap_editor_width_auto_height_editor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&"a ".to_string().repeat(100), cx);
        let mut editor = Editor::new(
            EditorMode::AutoHeight {
                min_lines: 1,
                max_lines: None,
            },
            buffer,
            None,
            window,
            cx,
        );
        editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
        editor
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

    for x in 1..=100 {
        let (_, state) = cx.draw(
            Default::default(),
            size(px(200. + 0.13 * x as f32), px(500.)),
            |_, _| EditorElement::new(&editor, style.clone()),
        );

        assert!(
            state.position_map.scroll_max.x == 0.,
            "Soft wrapped editor should have no horizontal scrolling!"
        );
    }
}

#[gpui::test]
async fn test_soft_wrap_editor_width_full_editor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&"a ".to_string().repeat(100), cx);
        let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
        editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
        editor
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));

    for x in 1..=100 {
        let (_, state) = cx.draw(
            Default::default(),
            size(px(200. + 0.13 * x as f32), px(500.)),
            |_, _| EditorElement::new(&editor, style.clone()),
        );

        assert!(
            state.position_map.scroll_max.x == 0.,
            "Soft wrapped editor should have no horizontal scrolling!"
        );
    }
}

#[gpui::test]
async fn test_point_for_position_clipped_rows(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let text = "aaa\nbbb";
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(text, cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });

    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
    let line_height = window
        .update(cx, |_, window, _| {
            style.text.line_height_in_pixels(window.rem_size())
        })
        .unwrap();

    // the first line is clipped
    let (_, state) = cx.draw(
        point(Pixels::ZERO, Pixels::ZERO - line_height * 1.5),
        size(px(500.), px(500.)),
        |_, _| EditorElement::new(&editor, style),
    );

    // click at the end of the second line
    let target_point = DisplayPoint::new(DisplayRow(1), 3);
    let click_x = state.content_origin.x
        + editor.update_in(cx, |editor, window, cx| {
            editor
                .snapshot(window, cx)
                .x_for_display_point(target_point, &editor.text_layout_details(window, cx))
        });

    let point = state
        .position_map
        .point_for_position(point(click_x, px(0.)));
    assert_eq!(point.nearest_valid, target_point);
}
