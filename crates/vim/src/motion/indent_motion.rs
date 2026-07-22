use super::*;

fn matches_indent_type(
    target_indent: &text::LineIndent,
    current_indent: &text::LineIndent,
    indent_type: IndentType,
) -> bool {
    match indent_type {
        IndentType::Lesser => {
            target_indent.spaces < current_indent.spaces || target_indent.tabs < current_indent.tabs
        }
        IndentType::Greater => {
            target_indent.spaces > current_indent.spaces || target_indent.tabs > current_indent.tabs
        }
        IndentType::Same => {
            target_indent.spaces == current_indent.spaces
                && target_indent.tabs == current_indent.tabs
        }
    }
}

fn indent_motion(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    times: usize,
    direction: Direction,
    indent_type: IndentType,
) -> DisplayPoint {
    let buffer_point = map.display_point_to_point(display_point, Bias::Left);
    let current_row = MultiBufferRow(buffer_point.row);
    let current_indent = map.line_indent_for_buffer_row(current_row);
    if current_indent.is_line_empty() {
        return display_point;
    }
    let max_row = map.max_point().to_point(map).row;

    for _ in 0..times {
        let current_buffer_row = map.display_point_to_point(display_point, Bias::Left).row;

        let target_row = match direction {
            Direction::Next => (current_buffer_row + 1..=max_row).find(|&row| {
                let indent = map.line_indent_for_buffer_row(MultiBufferRow(row));
                !indent.is_line_empty()
                    && matches_indent_type(&indent, &current_indent, indent_type)
            }),
            Direction::Prev => (0..current_buffer_row).rev().find(|&row| {
                let indent = map.line_indent_for_buffer_row(MultiBufferRow(row));
                !indent.is_line_empty()
                    && matches_indent_type(&indent, &current_indent, indent_type)
            }),
        }
        .unwrap_or(current_buffer_row);

        let new_point = map.point_to_display_point(Point::new(target_row, 0), Bias::Right);
        let new_point = first_non_whitespace(map, false, new_point);
        if new_point == display_point {
            break;
        }
        display_point = new_point;
    }
    display_point
}
