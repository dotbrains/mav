use super::*;

impl EditorElement {
    pub(super) fn layout_diff_hunk_controls(
        &self,
        row_range: Range<DisplayRow>,
        row_infos: &[RowInfo],
        text_hitbox: &Hitbox,
        newest_cursor_row: Option<DisplayRow>,
        line_height: Pixels,
        right_margin: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        sticky_header_height: Pixels,
        display_hunks: &[(DisplayDiffHunk, Option<Hitbox>)],
        highlighted_rows: &BTreeMap<DisplayRow, LineHighlight>,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut App,
    ) -> (Vec<AnyElement>, Vec<(DisplayRow, Bounds<Pixels>)>) {
        let render_diff_hunk_controls = editor.read(cx).render_diff_hunk_controls.clone();
        let hovered_diff_hunk_row = editor.read(cx).hovered_diff_hunk_row;
        let sticky_top = text_hitbox.bounds.top() + sticky_header_height;

        let mut controls = vec![];
        let mut control_bounds = vec![];

        let active_rows = [hovered_diff_hunk_row, newest_cursor_row];

        for (hunk, _) in display_hunks {
            if let DisplayDiffHunk::Unfolded {
                display_row_range,
                multi_buffer_range,
                status,
                is_created_file,
                ..
            } = &hunk
            {
                if display_row_range.start >= row_range.end {
                    // hunk is fully below the viewport
                    continue;
                }
                if display_row_range.end <= row_range.start {
                    // hunk is fully above the viewport
                    continue;
                }
                let row_ix = display_row_range.start.0.saturating_sub(row_range.start.0);
                if row_infos
                    .get(row_ix as usize)
                    .and_then(|row_info| row_info.diff_status)
                    .is_none()
                {
                    continue;
                }
                if highlighted_rows
                    .get(&display_row_range.start)
                    .and_then(|highlight| highlight.type_id)
                    .is_some_and(|type_id| {
                        [
                            TypeId::of::<ConflictsOuter>(),
                            TypeId::of::<ConflictsOursMarker>(),
                            TypeId::of::<ConflictsOurs>(),
                            TypeId::of::<ConflictsTheirs>(),
                            TypeId::of::<ConflictsTheirsMarker>(),
                        ]
                        .contains(&type_id)
                    })
                {
                    continue;
                }

                if active_rows
                    .iter()
                    .any(|row| row.is_some_and(|row| display_row_range.contains(&row)))
                {
                    let hunk_start_y: Pixels = (display_row_range.start.as_f64()
                        * ScrollPixelOffset::from(line_height)
                        + ScrollPixelOffset::from(text_hitbox.bounds.top())
                        - scroll_pixel_position.y)
                        .into();

                    let y: Pixels = if hunk_start_y >= sticky_top {
                        hunk_start_y
                    } else {
                        let hunk_end_y: Pixels = hunk_start_y
                            + (display_row_range.len() as f64
                                * ScrollPixelOffset::from(line_height))
                            .into();
                        let max_y = hunk_end_y - line_height;
                        sticky_top.min(max_y)
                    };

                    let mut element = render_diff_hunk_controls(
                        display_row_range.start.0,
                        status,
                        multi_buffer_range.clone(),
                        *is_created_file,
                        line_height,
                        &editor,
                        window,
                        cx,
                    );
                    let size =
                        element.layout_as_root(size(px(100.0), line_height).into(), window, cx);

                    let x = text_hitbox.bounds.right() - right_margin - px(10.) - size.width;

                    if x < text_hitbox.bounds.left() {
                        continue;
                    }

                    let bounds = Bounds::new(gpui::Point::new(x, y), size);
                    control_bounds.push((display_row_range.start, bounds));

                    window.with_absolute_element_offset(gpui::Point::new(x, y), |window| {
                        element.prepaint(window, cx)
                    });
                    controls.push(element);
                }
            }
        }

        (controls, control_bounds)
    }
}
