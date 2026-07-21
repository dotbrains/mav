use super::*;

#[gpui::test]
fn test_mappings(cx: &mut TestAppContext) {
    // Formatting.
    assert_mappings(
        &render_markdown("He*l*lo", cx),
        vec![vec![(0, 0), (1, 1), (2, 3), (3, 5), (4, 6), (5, 7)]],
    );

    // Multiple lines.
    assert_mappings(
        &render_markdown("Hello\n\nWorld", cx),
        vec![
            vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4), (5, 5)],
            vec![(0, 7), (1, 8), (2, 9), (3, 10), (4, 11), (5, 12)],
        ],
    );

    // Multi-byte characters.
    assert_mappings(
        &render_markdown("αβγ\n\nδεζ", cx),
        vec![
            vec![(0, 0), (2, 2), (4, 4), (6, 6)],
            vec![(0, 8), (2, 10), (4, 12), (6, 14)],
        ],
    );

    // Smart quotes.
    assert_mappings(&render_markdown("\"", cx), vec![vec![(0, 0), (3, 1)]]);
    assert_mappings(
        &render_markdown("\"hey\"", cx),
        vec![vec![(0, 0), (3, 1), (4, 2), (5, 3), (6, 4), (9, 5)]],
    );

    // HTML Comments are ignored
    assert_mappings(
        &render_markdown(
            "<!--\nrdoc-file=string.c\n- str.intern   -> symbol\n- str.to_sym   -> symbol\n-->\nReturns",
            cx,
        ),
        vec![vec![
            (0, 78),
            (1, 79),
            (2, 80),
            (3, 81),
            (4, 82),
            (5, 83),
            (6, 84),
        ]],
    );
}

#[gpui::test]
fn test_bounds_for_source_range_skips_gaps_between_rendered_lines(cx: &mut TestAppContext) {
    let source = "First\n\nSecond";
    let rendered = render_markdown(source, cx);
    let highlight_bounds = rendered.bounds_for_source_range(0..source.len());
    assert_eq!(highlight_bounds.len(), rendered.lines.len());

    for (line, highlight_bounds) in rendered.lines.iter().zip(highlight_bounds.iter()) {
        let line_bounds = line.layout.bounds();
        assert_eq!(highlight_bounds.top(), line_bounds.top());
        assert_eq!(
            highlight_bounds.bottom(),
            line_bounds.top() + line.layout.line_height()
        );
    }
}

#[gpui::test]
fn test_bounds_for_source_range_returns_one_bound_per_soft_wrap_row(cx: &mut TestAppContext) {
    let sentence = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, \
            sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
    let source = [sentence, sentence, sentence, sentence].join(" ");
    let rendered = render_markdown(&source, cx);
    let line = &rendered.lines[0];
    let line_bounds = line.layout.bounds();
    let line_height = line.layout.line_height();
    let wrapped_line = line.layout.line_layout_for_index(0).unwrap();
    let visual_row_count = wrapped_line.wrap_boundaries().len() + 1;

    let highlight_bounds = rendered.bounds_for_source_range(0..source.len());
    assert_eq!(highlight_bounds.len(), visual_row_count);

    let mut row_top = line_bounds.top();
    for (row_index, row_bounds) in highlight_bounds.iter().enumerate() {
        assert_eq!(row_bounds.top(), row_top);
        assert_eq!(row_bounds.bottom(), row_top + line_height);
        assert!(
            row_bounds.size.width > Pixels::ZERO,
            "row {row_index} should have a non-empty highlight"
        );
        row_top += line_height;
    }
}
