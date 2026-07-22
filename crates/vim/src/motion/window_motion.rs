use super::*;

fn window_top(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    text_layout_details: &TextLayoutDetails,
    mut times: usize,
) -> (DisplayPoint, SelectionGoal) {
    let first_visible_line = text_layout_details
        .scroll_anchor
        .scroll_top_display_point(map);

    if first_visible_line.row() != DisplayRow(0)
        && text_layout_details.vertical_scroll_margin as usize > times
    {
        times = text_layout_details.vertical_scroll_margin.ceil() as usize;
    }

    if let Some(visible_rows) = text_layout_details.visible_rows {
        let bottom_row = first_visible_line.row().0 + visible_rows as u32;
        let new_row = (first_visible_line.row().0 + (times as u32))
            .min(bottom_row)
            .min(map.max_point().row().0);
        let new_col = point.column().min(map.line_len(first_visible_line.row()));

        let new_point = DisplayPoint::new(DisplayRow(new_row), new_col);
        (map.clip_point(new_point, Bias::Left), SelectionGoal::None)
    } else {
        let new_row =
            DisplayRow((first_visible_line.row().0 + (times as u32)).min(map.max_point().row().0));
        let new_col = point.column().min(map.line_len(first_visible_line.row()));

        let new_point = DisplayPoint::new(new_row, new_col);
        (map.clip_point(new_point, Bias::Left), SelectionGoal::None)
    }
}

fn window_middle(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    if let Some(visible_rows) = text_layout_details.visible_rows {
        let first_visible_line = text_layout_details
            .scroll_anchor
            .scroll_top_display_point(map);

        let max_visible_rows =
            (visible_rows as u32).min(map.max_point().row().0 - first_visible_line.row().0);

        let new_row =
            (first_visible_line.row().0 + (max_visible_rows / 2)).min(map.max_point().row().0);
        let new_row = DisplayRow(new_row);
        let new_col = point.column().min(map.line_len(new_row));
        let new_point = DisplayPoint::new(new_row, new_col);
        (map.clip_point(new_point, Bias::Left), SelectionGoal::None)
    } else {
        (point, SelectionGoal::None)
    }
}

fn window_bottom(
    map: &DisplaySnapshot,
    point: DisplayPoint,
    text_layout_details: &TextLayoutDetails,
    mut times: usize,
) -> (DisplayPoint, SelectionGoal) {
    if let Some(visible_rows) = text_layout_details.visible_rows {
        let first_visible_line = text_layout_details
            .scroll_anchor
            .scroll_top_display_point(map);
        let bottom_row = first_visible_line.row().0
            + (visible_rows + text_layout_details.scroll_anchor.scroll_anchor.offset.y - 1.).floor()
                as u32;
        if bottom_row < map.max_point().row().0
            && text_layout_details.vertical_scroll_margin as usize > times
        {
            times = text_layout_details.vertical_scroll_margin.ceil() as usize;
        }
        let bottom_row_capped = bottom_row.min(map.max_point().row().0);
        let new_row = if bottom_row_capped.saturating_sub(times as u32) < first_visible_line.row().0
        {
            first_visible_line.row()
        } else {
            DisplayRow(bottom_row_capped.saturating_sub(times as u32))
        };
        let new_col = point.column().min(map.line_len(new_row));
        let new_point = DisplayPoint::new(new_row, new_col);
        (map.clip_point(new_point, Bias::Left), SelectionGoal::None)
    } else {
        (point, SelectionGoal::None)
    }
}
