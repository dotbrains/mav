use super::*;

/// Returns column 0 of the document's first line, imitating `gg` in Helix.
///
/// With a count, Helix treats it as a (1-based) line number, so a count of `n`
/// targets buffer row `n - 1`, clamped to the last line.
fn start_of_document(map: &DisplaySnapshot, times: Option<usize>) -> DisplayPoint {
    let buffer_row = match times {
        None => 0,
        Some(times) => (times.saturating_sub(1) as u32).min(map.max_row().0),
    };
    map.point_to_display_point(Point::new(buffer_row, 0), Bias::Left)
}

/// Returns column 0 of the document's last line, imitating `ge` in Helix.
///
/// Helix ignores any count for `ge`, so it is not taken here.
fn end_of_document(map: &DisplaySnapshot) -> DisplayPoint {
    map.point_to_display_point(Point::new(map.max_row().0, 0), Bias::Left)
}
