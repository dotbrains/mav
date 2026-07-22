use super::*;

#[gpui::test(retries = 5)]
async fn test_soft_wraps(cx: &mut gpui::TestAppContext) {
    cx.background_executor
        .set_block_on_ticks(usize::MAX..=usize::MAX);
    cx.update(|cx| {
        init_test(cx, &|_| {});
    });

    let mut cx = crate::test::editor_test_context::EditorTestContext::new(cx).await;
    let editor = cx.editor.clone();
    let window = cx.window;

    _ = cx.update_window(window, |_, window, cx| {
        let text_layout_details =
            editor.update(cx, |editor, cx| editor.text_layout_details(window, cx));

        let font_size = px(12.0);
        let wrap_width = Some(px(96.));

        let text = "one two three four five\nsix seven eight";
        let buffer = MultiBuffer::build_simple(text, cx);
        let map = cx.new(|cx| {
            DisplayMap::new(
                buffer.clone(),
                font("Helvetica"),
                font_size,
                wrap_width,
                1,
                1,
                FoldPlaceholder::test(),
                DiagnosticSeverity::Warning,
                cx,
            )
        });

        let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
        assert_eq!(
            snapshot.text_chunks(DisplayRow(0)).collect::<String>(),
            "one two \nthree four \nfive\nsix seven \neight"
        );
        assert_eq!(
            snapshot.clip_point(DisplayPoint::new(DisplayRow(0), 8), Bias::Left),
            DisplayPoint::new(DisplayRow(0), 7)
        );
        assert_eq!(
            snapshot.clip_point(DisplayPoint::new(DisplayRow(0), 8), Bias::Right),
            DisplayPoint::new(DisplayRow(1), 0)
        );
        assert_eq!(
            movement::right(&snapshot, DisplayPoint::new(DisplayRow(0), 7)),
            DisplayPoint::new(DisplayRow(1), 0)
        );
        assert_eq!(
            movement::left(&snapshot, DisplayPoint::new(DisplayRow(1), 0)),
            DisplayPoint::new(DisplayRow(0), 7)
        );

        let x = snapshot
            .x_for_display_point(DisplayPoint::new(DisplayRow(1), 10), &text_layout_details);
        assert_eq!(
            movement::up(
                &snapshot,
                DisplayPoint::new(DisplayRow(1), 10),
                language::SelectionGoal::None,
                false,
                &text_layout_details,
            ),
            (
                DisplayPoint::new(DisplayRow(0), 7),
                language::SelectionGoal::HorizontalPosition(f64::from(x))
            )
        );
        assert_eq!(
            movement::down(
                &snapshot,
                DisplayPoint::new(DisplayRow(0), 7),
                language::SelectionGoal::HorizontalPosition(f64::from(x)),
                false,
                &text_layout_details
            ),
            (
                DisplayPoint::new(DisplayRow(1), 10),
                language::SelectionGoal::HorizontalPosition(f64::from(x))
            )
        );
        assert_eq!(
            movement::down(
                &snapshot,
                DisplayPoint::new(DisplayRow(1), 10),
                language::SelectionGoal::HorizontalPosition(f64::from(x)),
                false,
                &text_layout_details
            ),
            (
                DisplayPoint::new(DisplayRow(2), 4),
                language::SelectionGoal::HorizontalPosition(f64::from(x))
            )
        );

        let ix = MultiBufferOffset(snapshot.buffer_snapshot().text().find("seven").unwrap());
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(ix..ix, "and ")], None, cx);
        });

        let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
        assert_eq!(
            snapshot.text_chunks(DisplayRow(1)).collect::<String>(),
            "three four \nfive\nsix and \nseven eight"
        );

        // Re-wrap on font size changes
        map.update(cx, |map, cx| {
            map.set_font(font("Helvetica"), font_size + Pixels::from(3.), cx)
        });

        let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
        assert_eq!(
            snapshot.text_chunks(DisplayRow(1)).collect::<String>(),
            "three \nfour five\nsix and \nseven \neight"
        )
    });
}

#[gpui::test]
fn test_text_chunks(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    let text = sample_text(6, 6, 'a');
    let buffer = MultiBuffer::build_simple(&text, cx);

    let font_size = px(14.0);
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            vec![
                (
                    MultiBufferPoint::new(1, 0)..MultiBufferPoint::new(1, 0),
                    "\t",
                ),
                (
                    MultiBufferPoint::new(1, 1)..MultiBufferPoint::new(1, 1),
                    "\t",
                ),
                (
                    MultiBufferPoint::new(2, 1)..MultiBufferPoint::new(2, 1),
                    "\t",
                ),
            ],
            None,
            cx,
        )
    });

    assert_eq!(
        map.update(cx, |map, cx| map.snapshot(cx))
            .text_chunks(DisplayRow(1))
            .collect::<String>()
            .lines()
            .next(),
        Some("    b   bbbbb")
    );
    assert_eq!(
        map.update(cx, |map, cx| map.snapshot(cx))
            .text_chunks(DisplayRow(2))
            .collect::<String>()
            .lines()
            .next(),
        Some("c   ccccc")
    );
}

#[gpui::test]
fn test_inlays_with_newlines_after_blocks(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.new(|cx| Buffer::local("a", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));

    let font_size = px(14.0);
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    map.update(cx, |map, cx| {
        map.insert_blocks(
            [BlockProperties {
                placement: BlockPlacement::Above(buffer_snapshot.anchor_before(Point::new(0, 0))),
                height: Some(2),
                style: BlockStyle::Sticky,
                render: Arc::new(|_| div().into_any()),
                priority: 0,
            }],
            cx,
        );
    });
    map.update(cx, |m, cx| assert_eq!(m.snapshot(cx).text(), "\n\na"));

    map.update(cx, |map, cx| {
        map.splice_inlays(
            &[],
            vec![Inlay::edit_prediction(
                0,
                buffer_snapshot.anchor_after(MultiBufferOffset(0)),
                "\n",
            )],
            cx,
        );
    });
    map.update(cx, |m, cx| assert_eq!(m.snapshot(cx).text(), "\n\n\na"));

    // Regression test: updating the display map does not crash when a
    // block is immediately followed by a multi-line inlay.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(MultiBufferOffset(1)..MultiBufferOffset(1), "b")],
            None,
            cx,
        );
    });
    map.update(cx, |m, cx| assert_eq!(m.snapshot(cx).text(), "\n\n\nab"));
}
