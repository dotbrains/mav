use super::*;

pub(super) fn build_excerpt_ranges(
    ranges: impl IntoIterator<Item = Range<Point>>,
    context_line_count: u32,
    buffer_snapshot: &BufferSnapshot,
) -> Vec<ExcerptRange<Point>> {
    ranges
        .into_iter()
        .map(|range| {
            let start_row = range.start.row.saturating_sub(context_line_count);
            let start = Point::new(start_row, 0);
            let end_row = (range.end.row + context_line_count).min(buffer_snapshot.max_point().row);
            let end = Point::new(end_row, buffer_snapshot.line_len(end_row));
            ExcerptRange {
                context: start..end,
                primary: range,
            }
        })
        .collect()
}
