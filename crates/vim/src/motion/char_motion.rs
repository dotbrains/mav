use super::*;

fn left(map: &DisplaySnapshot, mut point: DisplayPoint, times: usize) -> DisplayPoint {
    for _ in 0..times {
        point = movement::saturating_left(map, point);
        if point.column() == 0 {
            break;
        }
    }
    point
}

pub(crate) fn wrapping_left(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    times: usize,
) -> DisplayPoint {
    for _ in 0..times {
        point = movement::left(map, point);
        if point.is_zero() {
            break;
        }
    }
    point
}

fn wrapping_right(map: &DisplaySnapshot, mut point: DisplayPoint, times: usize) -> DisplayPoint {
    for _ in 0..times {
        point = wrapping_right_single(map, point);
        if point == map.max_point() {
            break;
        }
    }
    point
}

fn wrapping_right_single(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let mut next_point = point;
    *next_point.column_mut() += 1;
    next_point = map.clip_point(next_point, Bias::Right);
    if next_point == point {
        if next_point.row() == map.max_point().row() {
            next_point
        } else {
            DisplayPoint::new(next_point.row().next_row(), 0)
        }
    } else {
        next_point
    }
}

fn up_down_buffer_rows(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    mut goal: SelectionGoal,
    mut times: isize,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    let bias = if times < 0 { Bias::Left } else { Bias::Right };

    while map.is_folded_buffer_header(point.row()) {
        if times < 0 {
            (point, _) = movement::up(map, point, goal, true, text_layout_details);
            times += 1;
        } else if times > 0 {
            (point, _) = movement::down(map, point, goal, true, text_layout_details);
            times -= 1;
        } else {
            break;
        }
    }

    let start = map.display_point_to_fold_point(point, Bias::Left);
    let begin_folded_line = map.fold_point_to_display_point(
        map.fold_snapshot()
            .clip_point(FoldPoint::new(start.row(), 0), Bias::Left),
    );
    let select_nth_wrapped_row = point.row().0 - begin_folded_line.row().0;

    let (goal_wrap, goal_x) = match goal {
        SelectionGoal::WrappedHorizontalPosition((row, x)) => (row, x),
        SelectionGoal::HorizontalRange { end, .. } => (select_nth_wrapped_row, end as f32),
        SelectionGoal::HorizontalPosition(x) => (select_nth_wrapped_row, x as f32),
        _ => {
            let x = map.x_for_display_point(point, text_layout_details);
            goal = SelectionGoal::WrappedHorizontalPosition((select_nth_wrapped_row, x.into()));
            (select_nth_wrapped_row, x.into())
        }
    };

    let target = start.row() as isize + times;
    let new_row = (target.max(0) as u32).min(map.fold_snapshot().max_point().row());

    let mut begin_folded_line = map.fold_point_to_display_point(
        map.fold_snapshot()
            .clip_point(FoldPoint::new(new_row, 0), bias),
    );

    let mut i = 0;
    while i < goal_wrap && begin_folded_line.row() < map.max_point().row() {
        let next_folded_line = DisplayPoint::new(begin_folded_line.row().next_row(), 0);
        if map
            .display_point_to_fold_point(next_folded_line, bias)
            .row()
            == new_row
        {
            i += 1;
            begin_folded_line = next_folded_line;
        } else {
            break;
        }
    }

    let new_col = if i == goal_wrap {
        map.display_column_for_x(begin_folded_line.row(), px(goal_x), text_layout_details)
    } else {
        map.line_len(begin_folded_line.row())
    };

    let point = DisplayPoint::new(begin_folded_line.row(), new_col);
    let mut clipped_point = map.clip_point(point, bias);

    // When navigating vertically in vim mode with inlay hints present,
    // we need to handle the case where clipping moves us to a different row.
    // This can happen when moving down (Bias::Right) and hitting an inlay hint.
    // Re-clip with opposite bias to stay on the intended line.
    //
    // See: https://github.com/mav-industries/mav/issues/29134
    if clipped_point.row() > point.row() {
        clipped_point = map.clip_point(point, Bias::Left);
    }

    (clipped_point, goal)
}

fn down_display(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    mut goal: SelectionGoal,
    times: usize,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    for _ in 0..times {
        (point, goal) = movement::down(map, point, goal, true, text_layout_details);
    }

    (point, goal)
}

fn up_display(
    map: &DisplaySnapshot,
    mut point: DisplayPoint,
    mut goal: SelectionGoal,
    times: usize,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    for _ in 0..times {
        (point, goal) = movement::up(map, point, goal, true, text_layout_details);
    }

    (point, goal)
}

pub(crate) fn right(map: &DisplaySnapshot, mut point: DisplayPoint, times: usize) -> DisplayPoint {
    for _ in 0..times {
        let new_point = movement::saturating_right(map, point);
        if point == new_point {
            break;
        }
        point = new_point;
    }
    point
}

pub(crate) fn next_char(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    allow_cross_newline: bool,
) -> DisplayPoint {
    let mut new_point = point;
    let mut max_column = map.line_len(new_point.row());
    if !allow_cross_newline {
        max_column -= 1;
    }
    if new_point.column() < max_column {
        *new_point.column_mut() += 1;
    } else if new_point < map.max_point() && allow_cross_newline {
        *new_point.row_mut() += 1;
        *new_point.column_mut() = 0;
    }
    map.clip_ignoring_line_ends(new_point, Bias::Right)
}
