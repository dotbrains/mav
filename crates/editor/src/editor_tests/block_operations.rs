use super::*;

#[gpui::test]
fn test_transpose(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    _ = cx.add_window(|window, cx| {
        let mut editor = build_editor(MultiBuffer::build_simple("abc", cx), window, cx);
        editor.set_style(EditorStyle::default(), window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bac");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(2)..MultiBufferOffset(2)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bca");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(3)..MultiBufferOffset(3)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bac");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(3)..MultiBufferOffset(3)]
        );

        editor
    });

    _ = cx.add_window(|window, cx| {
        let mut editor = build_editor(MultiBuffer::build_simple("abc\nde", cx), window, cx);
        editor.set_style(EditorStyle::default(), window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(3)..MultiBufferOffset(3)])
        });
        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "acb\nde");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(3)..MultiBufferOffset(3)]
        );

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(4)..MultiBufferOffset(4)])
        });
        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "acbd\ne");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(5)..MultiBufferOffset(5)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "acbde\n");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(6)..MultiBufferOffset(6)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "acbd\ne");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(6)..MultiBufferOffset(6)]
        );

        editor
    });

    _ = cx.add_window(|window, cx| {
        let mut editor = build_editor(MultiBuffer::build_simple("abc\nde", cx), window, cx);
        editor.set_style(EditorStyle::default(), window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(1)..MultiBufferOffset(1),
                MultiBufferOffset(2)..MultiBufferOffset(2),
                MultiBufferOffset(4)..MultiBufferOffset(4),
            ])
        });
        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bacd\ne");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                MultiBufferOffset(2)..MultiBufferOffset(2),
                MultiBufferOffset(3)..MultiBufferOffset(3),
                MultiBufferOffset(5)..MultiBufferOffset(5)
            ]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bcade\n");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                MultiBufferOffset(3)..MultiBufferOffset(3),
                MultiBufferOffset(4)..MultiBufferOffset(4),
                MultiBufferOffset(6)..MultiBufferOffset(6)
            ]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bcda\ne");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                MultiBufferOffset(4)..MultiBufferOffset(4),
                MultiBufferOffset(6)..MultiBufferOffset(6)
            ]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bcade\n");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                MultiBufferOffset(4)..MultiBufferOffset(4),
                MultiBufferOffset(6)..MultiBufferOffset(6)
            ]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "bcaed\n");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                MultiBufferOffset(5)..MultiBufferOffset(5),
                MultiBufferOffset(6)..MultiBufferOffset(6)
            ]
        );

        editor
    });

    _ = cx.add_window(|window, cx| {
        let mut editor = build_editor(MultiBuffer::build_simple("🍐🏀✋", cx), window, cx);
        editor.set_style(EditorStyle::default(), window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(4)..MultiBufferOffset(4)])
        });
        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "🏀🍐✋");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(8)..MultiBufferOffset(8)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "🏀✋🍐");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(11)..MultiBufferOffset(11)]
        );

        editor.transpose(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "🏀🍐✋");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [MultiBufferOffset(11)..MultiBufferOffset(11)]
        );

        editor
    });
}
