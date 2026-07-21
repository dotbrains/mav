use super::*;

impl EditorElement {
    pub(super) fn layout_diff_hunk_control_phase(
        &self,
        is_read_only: bool,
        sticky_headers: &Option<header::StickyHeaders>,
        has_sticky_buffer_header: bool,
        blocks: &[BlockLayout],
        scroll_position: gpui::Point<ScrollOffset>,
        row_range: Range<DisplayRow>,
        row_infos: &[RowInfo],
        text_hitbox: &Hitbox,
        current_selection_head: Option<DisplayRow>,
        line_height: Pixels,
        right_margin: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        display_hunks: &[(DisplayDiffHunk, Option<Hitbox>)],
        highlighted_rows: &BTreeMap<DisplayRow, LineHighlight>,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::DiffHunkControlLayouts {
        let sticky_scroll_header_height = sticky_headers
            .as_ref()
            .and_then(|headers| headers.lines.last())
            .map_or(Pixels::ZERO, |last| last.offset + line_height);

        let sticky_header_height = if has_sticky_buffer_header {
            let full_height = FILE_HEADER_HEIGHT as f32 * line_height;
            let display_row = blocks
                .iter()
                .filter(|block| block.is_buffer_header)
                .find_map(|block| block.row.filter(|row| row.0 > scroll_position.y as u32));
            let offset = match display_row {
                Some(display_row) => {
                    let max_row = display_row.0.saturating_sub(FILE_HEADER_HEIGHT);
                    let offset = (scroll_position.y - max_row as f64).max(0.0);
                    let slide_up = Pixels::from(offset * ScrollPixelOffset::from(line_height));

                    (full_height - slide_up).max(Pixels::ZERO)
                }
                None => full_height,
            };
            let header_bottom_padding = BUFFER_HEADER_PADDING.to_pixels(window.rem_size());
            sticky_scroll_header_height + offset - header_bottom_padding
        } else {
            sticky_scroll_header_height
        };

        let (diff_hunk_controls, diff_hunk_control_bounds) =
            if is_read_only && !self.editor.read(cx).delegate_stage_and_restore {
                (vec![], vec![])
            } else {
                self.layout_diff_hunk_controls(
                    row_range,
                    row_infos,
                    text_hitbox,
                    current_selection_head,
                    line_height,
                    right_margin,
                    scroll_pixel_position,
                    sticky_header_height,
                    display_hunks,
                    highlighted_rows,
                    self.editor.clone(),
                    window,
                    cx,
                )
            };

        layout_data::DiffHunkControlLayouts {
            diff_hunk_controls,
            diff_hunk_control_bounds,
        }
    }

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
