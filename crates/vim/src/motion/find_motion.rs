use super::*;

fn unmatched_forward(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    char: char,
    times: usize,
) -> DisplayPoint {
    for _ in 0..times {
        // https://github.com/vim/vim/blob/1d87e11a1ef201b26ed87585fba70182ad0c468a/runtime/doc/motion.txt#L1245
        let point = display_point.to_point(map);
        let offset = point.to_offset(&map.buffer_snapshot());

        let ranges = map.buffer_snapshot().enclosing_bracket_ranges(point..point);
        let Some(ranges) = ranges else { break };
        let mut closest_closing_destination = None;
        let mut closest_distance = usize::MAX;

        for (_, close_range) in ranges {
            if close_range.start > offset {
                let mut chars = map.buffer_snapshot().chars_at(close_range.start);
                if Some(char) == chars.next() {
                    let distance = close_range.start - offset;
                    if distance < closest_distance {
                        closest_closing_destination = Some(close_range.start);
                        closest_distance = distance;
                        continue;
                    }
                }
            }
        }

        let new_point = closest_closing_destination
            .map(|destination| destination.to_display_point(map))
            .unwrap_or(display_point);
        if new_point == display_point {
            break;
        }
        display_point = new_point;
    }
    display_point
}

fn unmatched_backward(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    char: char,
    times: usize,
) -> DisplayPoint {
    for _ in 0..times {
        // https://github.com/vim/vim/blob/1d87e11a1ef201b26ed87585fba70182ad0c468a/runtime/doc/motion.txt#L1239
        let point = display_point.to_point(map);
        let offset = point.to_offset(&map.buffer_snapshot());

        let ranges = map.buffer_snapshot().enclosing_bracket_ranges(point..point);
        let Some(ranges) = ranges else {
            break;
        };

        let mut closest_starting_destination = None;
        let mut closest_distance = usize::MAX;

        for (start_range, _) in ranges {
            if start_range.start < offset {
                let mut chars = map.buffer_snapshot().chars_at(start_range.start);
                if Some(char) == chars.next() {
                    let distance = offset - start_range.start;
                    if distance < closest_distance {
                        closest_starting_destination = Some(start_range.start);
                        closest_distance = distance;
                        continue;
                    }
                }
            }
        }

        let new_point = closest_starting_destination
            .map(|destination| destination.to_display_point(map))
            .unwrap_or(display_point);
        if new_point == display_point {
            break;
        } else {
            display_point = new_point;
        }
    }
    display_point
}

fn find_forward(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    before: bool,
    target: char,
    times: usize,
    mode: FindRange,
    smartcase: bool,
) -> Option<DisplayPoint> {
    let mut to = from;
    let mut found = false;

    for _ in 0..times {
        found = false;
        let new_to = find_boundary(map, to, mode, &mut |_, right| {
            found = is_character_match(target, right, smartcase);
            found
        });
        if to == new_to {
            break;
        }
        to = new_to;
    }

    if found {
        if before && to.column() > 0 {
            *to.column_mut() -= 1;
            Some(map.clip_point(to, Bias::Left))
        } else if before && to.row().0 > 0 {
            *to.row_mut() -= 1;
            *to.column_mut() = map.line(to.row()).len() as u32;
            Some(map.clip_point(to, Bias::Left))
        } else {
            Some(to)
        }
    } else {
        None
    }
}

fn find_backward(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    after: bool,
    target: char,
    times: usize,
    mode: FindRange,
    smartcase: bool,
) -> DisplayPoint {
    let mut to = from;

    for _ in 0..times {
        let new_to = find_preceding_boundary_display_point(map, to, mode, &mut |_, right| {
            is_character_match(target, right, smartcase)
        });
        if to == new_to {
            break;
        }
        to = new_to;
    }

    let next = map.buffer_snapshot().chars_at(to.to_point(map)).next();
    if next.is_some() && is_character_match(target, next.unwrap(), smartcase) {
        if after {
            *to.column_mut() += 1;
            map.clip_point(to, Bias::Right)
        } else {
            to
        }
    } else {
        from
    }
}

/// Returns true if one char is equal to the other or its uppercase variant (if smartcase is true).
pub fn is_character_match(target: char, other: char, smartcase: bool) -> bool {
    if smartcase {
        if target.is_uppercase() {
            target == other
        } else {
            target == other.to_ascii_lowercase()
        }
    } else {
        target == other
    }
}

fn sneak(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    first_target: char,
    second_target: char,
    times: usize,
    smartcase: bool,
) -> Option<DisplayPoint> {
    let mut to = from;
    let mut found = false;

    for _ in 0..times {
        found = false;
        let new_to = find_boundary(
            map,
            movement::right(map, to),
            FindRange::MultiLine,
            &mut |left, right| {
                found = is_character_match(first_target, left, smartcase)
                    && is_character_match(second_target, right, smartcase);
                found
            },
        );
        if to == new_to {
            break;
        }
        to = new_to;
    }

    if found {
        Some(movement::left(map, to))
    } else {
        None
    }
}

fn sneak_backward(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    first_target: char,
    second_target: char,
    times: usize,
    smartcase: bool,
) -> Option<DisplayPoint> {
    let mut to = from;
    let mut found = false;

    for _ in 0..times {
        found = false;
        let new_to = find_preceding_boundary_display_point(
            map,
            to,
            FindRange::MultiLine,
            &mut |left, right| {
                found = is_character_match(first_target, left, smartcase)
                    && is_character_match(second_target, right, smartcase);
                found
            },
        );
        if to == new_to {
            break;
        }
        to = new_to;
    }

    if found {
        Some(movement::left(map, to))
    } else {
        None
    }
}

fn next_line_start(map: &DisplaySnapshot, point: DisplayPoint, times: usize) -> DisplayPoint {
    let correct_line = map.start_of_relative_buffer_row(point, times as isize);
    first_non_whitespace(map, false, correct_line)
}

fn previous_line_start(map: &DisplaySnapshot, point: DisplayPoint, times: usize) -> DisplayPoint {
    let correct_line = map.start_of_relative_buffer_row(point, -(times as isize));
    first_non_whitespace(map, false, correct_line)
}

fn go_to_column(map: &DisplaySnapshot, point: DisplayPoint, times: usize) -> DisplayPoint {
    let correct_line = map.start_of_relative_buffer_row(point, 0);
    right(map, correct_line, times.saturating_sub(1))
}

pub(crate) fn next_line_end(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    times: usize,
) -> DisplayPoint {
    if times > 1 {
        point = map.start_of_relative_buffer_row(point, times as isize - 1);
    }
    end_of_line(map, false, point, 1)
}
