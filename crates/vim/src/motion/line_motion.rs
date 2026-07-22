use super::*;

pub(crate) fn first_non_whitespace(
    map: &DisplaySnapshot,
    display_lines: bool,
    from: DisplayPoint,
) -> DisplayPoint {
    let mut start_offset = start_of_line(map, display_lines, from).to_offset(map, Bias::Left);
    let classifier = map.buffer_snapshot().char_classifier_at(from.to_point(map));
    for (ch, offset) in map.buffer_chars_at(start_offset) {
        if ch == '\n' {
            return from;
        }

        start_offset = offset;

        if classifier.kind(ch) != CharKind::Whitespace {
            break;
        }
    }

    start_offset.to_display_point(map)
}

pub(crate) fn last_non_whitespace(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    count: usize,
) -> DisplayPoint {
    let mut end_of_line = end_of_line(map, false, from, count).to_offset(map, Bias::Left);
    let classifier = map.buffer_snapshot().char_classifier_at(from.to_point(map));

    // NOTE: depending on clip_at_line_end we may already be one char back from the end.
    if let Some((ch, _)) = map.buffer_chars_at(end_of_line).next()
        && classifier.kind(ch) != CharKind::Whitespace
    {
        return end_of_line.to_display_point(map);
    }

    for (ch, offset) in map.reverse_buffer_chars_at(end_of_line) {
        if ch == '\n' {
            break;
        }
        end_of_line = offset;
        if classifier.kind(ch) != CharKind::Whitespace || ch == '\n' {
            break;
        }
    }

    end_of_line.to_display_point(map)
}

pub(crate) fn start_of_line(
    map: &DisplaySnapshot,
    display_lines: bool,
    point: DisplayPoint,
) -> DisplayPoint {
    if display_lines {
        map.clip_point(DisplayPoint::new(point.row(), 0), Bias::Right)
    } else {
        map.prev_line_boundary(point.to_point(map)).1
    }
}

pub(crate) fn middle_of_line(
    map: &DisplaySnapshot,
    display_lines: bool,
    point: DisplayPoint,
    times: Option<usize>,
) -> DisplayPoint {
    let percent = if let Some(times) = times.filter(|&t| t <= 100) {
        times as f64 / 100.
    } else {
        0.5
    };
    if display_lines {
        map.clip_point(
            DisplayPoint::new(
                point.row(),
                (map.line_len(point.row()) as f64 * percent) as u32,
            ),
            Bias::Left,
        )
    } else {
        let mut buffer_point = point.to_point(map);
        buffer_point.column = (map
            .buffer_snapshot()
            .line_len(MultiBufferRow(buffer_point.row)) as f64
            * percent) as u32;

        map.clip_point(buffer_point.to_display_point(map), Bias::Left)
    }
}

pub(crate) fn end_of_line(
    map: &DisplaySnapshot,
    display_lines: bool,
    mut point: DisplayPoint,
    times: usize,
) -> DisplayPoint {
    if times > 1 {
        point = map.start_of_relative_buffer_row(point, times as isize - 1);
    }
    if display_lines {
        map.clip_point(
            DisplayPoint::new(point.row(), map.line_len(point.row())),
            Bias::Left,
        )
    } else {
        map.clip_point(map.next_line_boundary(point.to_point(map)).1, Bias::Left)
    }
}
