use super::*;

impl EditorElement {
    pub(super) fn paint_highlighted_range(
        &self,
        range: Range<DisplayPoint>,
        fill: bool,
        color: Hsla,
        corner_radius: Pixels,
        line_end_overshoot: Pixels,
        layout: &EditorLayout,
        window: &mut Window,
    ) {
        let start_row = layout.visible_display_row_range.start;
        let end_row = layout.visible_display_row_range.end;
        if range.start != range.end {
            let row_range = if range.end.column() == 0 {
                cmp::max(range.start.row(), start_row)..cmp::min(range.end.row(), end_row)
            } else {
                cmp::max(range.start.row(), start_row)
                    ..cmp::min(range.end.row().next_row(), end_row)
            };

            let highlighted_range = HighlightedRange {
                color,
                line_height: layout.position_map.line_height,
                corner_radius,
                start_y: layout.content_origin.y
                    + Pixels::from(
                        (row_range.start.as_f64() - layout.position_map.scroll_position.y)
                            * ScrollOffset::from(layout.position_map.line_height),
                    ),
                lines: row_range
                    .iter_rows()
                    .map(|row| {
                        let line_layout =
                            &layout.position_map.line_layouts[row.minus(start_row) as usize];
                        let alignment_offset =
                            line_layout.alignment_offset(layout.text_align, layout.content_width);
                        HighlightedRangeLine {
                            start_x: if row == range.start.row() {
                                layout.content_origin.x
                                    + Pixels::from(
                                        ScrollPixelOffset::from(
                                            line_layout.x_for_index(range.start.column() as usize)
                                                + alignment_offset,
                                        ) - layout.position_map.scroll_pixel_position.x,
                                    )
                            } else {
                                layout.content_origin.x + alignment_offset
                                    - Pixels::from(layout.position_map.scroll_pixel_position.x)
                            },
                            end_x: if row == range.end.row() {
                                layout.content_origin.x
                                    + Pixels::from(
                                        ScrollPixelOffset::from(
                                            line_layout.x_for_index(range.end.column() as usize)
                                                + alignment_offset,
                                        ) - layout.position_map.scroll_pixel_position.x,
                                    )
                            } else {
                                Pixels::from(
                                    ScrollPixelOffset::from(
                                        layout.content_origin.x
                                            + line_layout.width
                                            + alignment_offset
                                            + line_end_overshoot,
                                    ) - layout.position_map.scroll_pixel_position.x,
                                )
                            },
                        }
                    })
                    .collect(),
            };

            highlighted_range.paint(fill, layout.position_map.text_hitbox.bounds, window);
        }
    }

    pub(super) fn paint_inline_diagnostics(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for mut inline_diagnostic in layout.inline_diagnostics.drain() {
            inline_diagnostic.1.paint(window, cx);
        }
    }

    pub(super) fn paint_inline_blame(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mut blame_layout) = layout.inline_blame_layout.take() {
            window.paint_layer(layout.position_map.text_hitbox.bounds, |window| {
                blame_layout.element.paint(window, cx);
            })
        }
    }

    pub(super) fn paint_inline_code_actions(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mut inline_code_actions) = layout.inline_code_actions.take() {
            window.paint_layer(layout.position_map.text_hitbox.bounds, |window| {
                inline_code_actions.paint(window, cx);
            })
        }
    }

    pub(super) fn paint_diff_hunk_controls(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for mut diff_hunk_control in layout.diff_hunk_controls.drain(..) {
            diff_hunk_control.paint(window, cx);
        }
    }

    pub(super) fn paint_spacer_blocks(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for mut block in layout.spacer_blocks.drain(..) {
            let mut bounds = layout.hitbox.bounds;
            bounds.origin.x += layout.gutter_hitbox.bounds.size.width;
            window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
                block.element.paint(window, cx);
            })
        }
    }

    pub(super) fn paint_non_spacer_blocks(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for mut block in layout.blocks.drain(..) {
            if block.overlaps_gutter {
                block.element.paint(window, cx);
            } else {
                let mut bounds = layout.hitbox.bounds;
                bounds.origin.x += layout.gutter_hitbox.bounds.size.width;
                window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
                    block.element.paint(window, cx);
                })
            }
        }
    }

    pub(super) fn paint_edit_prediction_popover(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(edit_prediction_popover) = layout.edit_prediction_popover.as_mut() {
            edit_prediction_popover.paint(window, cx);
        }
    }

    pub(super) fn paint_mouse_context_menu(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mouse_context_menu) = layout.mouse_context_menu.as_mut() {
            mouse_context_menu.paint(window, cx);
        }
    }
}
