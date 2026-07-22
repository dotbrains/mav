use super::*;

fn go_to_line(map: &DisplaySnapshot, display_point: DisplayPoint, line: usize) -> DisplayPoint {
    let point = map.display_point_to_point(display_point, Bias::Left);
    let snapshot = map.buffer_snapshot();
    let Some((buffer_snapshot, _)) = snapshot.point_to_buffer_point(point) else {
        return display_point;
    };

    let Some(anchor) = snapshot.anchor_in_excerpt(buffer_snapshot.anchor_after(
        buffer_snapshot.clip_point(Point::new((line - 1) as u32, point.column), Bias::Left),
    )) else {
        return display_point;
    };

    map.clip_point(
        map.point_to_display_point(anchor.to_point(snapshot), Bias::Left),
        Bias::Left,
    )
}

fn start_of_document(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    maybe_times: Option<usize>,
) -> DisplayPoint {
    if let Some(times) = maybe_times {
        return go_to_line(map, display_point, times);
    }

    let point = map.display_point_to_point(display_point, Bias::Left);
    let mut first_point = Point::zero();
    first_point.column = point.column;

    map.clip_point(
        map.point_to_display_point(
            map.buffer_snapshot().clip_point(first_point, Bias::Left),
            Bias::Left,
        ),
        Bias::Left,
    )
}

fn end_of_document(
    map: &DisplaySnapshot,
    display_point: DisplayPoint,
    maybe_times: Option<usize>,
) -> DisplayPoint {
    if let Some(times) = maybe_times {
        return go_to_line(map, display_point, times);
    };
    let point = map.display_point_to_point(display_point, Bias::Left);
    let mut last_point = map.buffer_snapshot().max_point();
    last_point.column = point.column;

    map.clip_point(
        map.point_to_display_point(
            map.buffer_snapshot().clip_point(last_point, Bias::Left),
            Bias::Left,
        ),
        Bias::Left,
    )
}

// Go to {count} percentage in the file, on the first
// non-blank in the line linewise.  To compute the new
// line number this formula is used:
// ({count} * number-of-lines + 99) / 100
//
// https://neovim.io/doc/user/motion.html#N%25
fn go_to_percentage(map: &DisplaySnapshot, point: DisplayPoint, count: usize) -> DisplayPoint {
    let total_lines = map.buffer_snapshot().max_point().row + 1;
    let target_line = (count * total_lines as usize).div_ceil(100);
    let target_point = DisplayPoint::new(
        DisplayRow(target_line.saturating_sub(1) as u32),
        point.column(),
    );
    map.clip_point(target_point, Bias::Left)
}
