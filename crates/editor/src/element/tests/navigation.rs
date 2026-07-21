use super::*;

#[gpui::test]
fn test_navigation_overlay_covered_text_highlights_are_replaced(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("overlay replacement", cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let editor = window.root(cx).unwrap();

    editor.update(cx, |editor, cx| {
        let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let target_start = buffer_snapshot.anchor_after(Point::new(0, 0));
        let target_end = buffer_snapshot.anchor_after(Point::new(0, 7));
        let covered_text_end = buffer_snapshot.anchor_after(Point::new(0, 2));

        editor.set_navigation_overlays(
            PRIMARY_NAVIGATION_OVERLAY_KEY,
            vec![navigation_overlay(
                "ov",
                target_start..target_end,
                Some(target_start..covered_text_end),
            )],
            cx,
        );
        assert!(
            editor
                .text_highlights(
                    HighlightKey::NavigationOverlay(PRIMARY_NAVIGATION_OVERLAY_KEY),
                    cx,
                )
                .is_some()
        );

        editor.set_navigation_overlays(
            PRIMARY_NAVIGATION_OVERLAY_KEY,
            vec![navigation_overlay("ov", target_start..target_end, None)],
            cx,
        );
        assert!(
            editor
                .text_highlights(
                    HighlightKey::NavigationOverlay(PRIMARY_NAVIGATION_OVERLAY_KEY),
                    cx,
                )
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_navigation_overlay_repositions_when_editor_width_changes(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let text = "jump target overlay ".repeat(16);
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&text, cx);
        let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
        editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
        editor
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();

    editor.update(cx, |editor, cx| {
        let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let target_start = buffer_snapshot.anchor_after(Point::new(0, 30));
        let target_end = buffer_snapshot.anchor_after(Point::new(0, 40));

        editor.set_navigation_overlays(
            PRIMARY_NAVIGATION_OVERLAY_KEY,
            vec![navigation_overlay("jj", target_start..target_end, None)],
            cx,
        );
    });

    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
    let (_, wide_state) = cx.draw(Default::default(), size(px(520.), px(260.)), |_, _| {
        EditorElement::new(&editor, style.clone())
    });
    let (_, narrow_state) = cx.draw(Default::default(), size(px(140.), px(260.)), |_, _| {
        EditorElement::new(&editor, style.clone())
    });

    let wide_label_layouts = navigation_label_layouts(&wide_state);
    let narrow_label_layouts = navigation_label_layouts(&narrow_state);

    assert_eq!(wide_label_layouts.len(), 1);
    assert_eq!(narrow_label_layouts.len(), 1);

    let wide_label_origin = wide_label_layouts[0].origin;
    let narrow_label_origin = narrow_label_layouts[0].origin;

    assert!(
        narrow_label_origin.y > wide_label_origin.y,
        "expected inline label to move to a later wrapped row when the editor narrows"
    );
    assert!(
        narrow_label_origin.x < wide_label_origin.x,
        "expected inline label to recompute its horizontal position for the wrapped row"
    );
}
