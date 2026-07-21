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

impl EditorElement {
    pub(super) fn bg_segments_per_row(
        rows: Range<DisplayRow>,
        selections: &[(PlayerColor, Vec<SelectionLayout>)],
        highlight_ranges: impl IntoIterator<Item = (Range<DisplayPoint>, Hsla)>,
        base_background: Hsla,
    ) -> Vec<Vec<(Range<DisplayPoint>, Hsla)>> {
        if rows.start >= rows.end {
            return Vec::new();
        }
        if !base_background.is_opaque() {
            // We don't actually know what color is behind this editor.
            return Vec::new();
        }
        let highlight_iter = highlight_ranges.into_iter();
        let selection_iter = selections.iter().flat_map(|(player_color, layouts)| {
            let color = player_color.selection;
            layouts.iter().filter_map(move |selection_layout| {
                if selection_layout.range.start != selection_layout.range.end {
                    Some((selection_layout.range.clone(), color))
                } else {
                    None
                }
            })
        });
        let mut per_row_map = vec![Vec::new(); rows.len()];
        for (range, color) in highlight_iter.chain(selection_iter) {
            let covered_rows = if range.end.column() == 0 {
                cmp::max(range.start.row(), rows.start)..cmp::min(range.end.row(), rows.end)
            } else {
                cmp::max(range.start.row(), rows.start)
                    ..cmp::min(range.end.row().next_row(), rows.end)
            };
            for row in covered_rows.iter_rows() {
                let seg_start = if row == range.start.row() {
                    range.start
                } else {
                    DisplayPoint::new(row, 0)
                };
                let seg_end = if row == range.end.row() && range.end.column() != 0 {
                    range.end
                } else {
                    DisplayPoint::new(row, u32::MAX)
                };
                let ix = row.minus(rows.start) as usize;
                debug_assert!(row >= rows.start && row < rows.end);
                debug_assert!(ix < per_row_map.len());
                per_row_map[ix].push((seg_start..seg_end, color));
            }
        }
        for row_segments in per_row_map.iter_mut() {
            if row_segments.is_empty() {
                continue;
            }
            let segments = mem::take(row_segments);
            let merged = Self::merge_overlapping_ranges(segments, base_background);
            *row_segments = merged;
        }
        per_row_map
    }

    /// Merge overlapping ranges by splitting at all range boundaries and blending colors where
    /// multiple ranges overlap. The result contains non-overlapping ranges ordered from left to right.
    ///
    /// Expects `start.row() == end.row()` for each range.
    pub(super) fn merge_overlapping_ranges(
        ranges: Vec<(Range<DisplayPoint>, Hsla)>,
        base_background: Hsla,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        struct Boundary {
            pos: DisplayPoint,
            is_start: bool,
            index: usize,
            color: Hsla,
        }

        let mut boundaries: SmallVec<[Boundary; 16]> = SmallVec::with_capacity(ranges.len() * 2);
        for (index, (range, color)) in ranges.iter().enumerate() {
            debug_assert!(
                range.start.row() == range.end.row(),
                "expects single-row ranges"
            );
            if range.start < range.end {
                boundaries.push(Boundary {
                    pos: range.start,
                    is_start: true,
                    index,
                    color: *color,
                });
                boundaries.push(Boundary {
                    pos: range.end,
                    is_start: false,
                    index,
                    color: *color,
                });
            }
        }

        if boundaries.is_empty() {
            return Vec::new();
        }

        boundaries
            .sort_unstable_by(|a, b| a.pos.cmp(&b.pos).then_with(|| a.is_start.cmp(&b.is_start)));

        let mut processed_ranges: Vec<(Range<DisplayPoint>, Hsla)> = Vec::new();
        let mut active_ranges: SmallVec<[(usize, Hsla); 8]> = SmallVec::new();

        let mut i = 0;
        let mut start_pos = boundaries[0].pos;

        let boundaries_len = boundaries.len();
        while i < boundaries_len {
            let current_boundary_pos = boundaries[i].pos;
            if start_pos < current_boundary_pos {
                if !active_ranges.is_empty() {
                    let mut color = base_background;
                    for &(_, c) in &active_ranges {
                        color = Hsla::blend(color, c);
                    }
                    if let Some((last_range, last_color)) = processed_ranges.last_mut() {
                        if *last_color == color && last_range.end == start_pos {
                            last_range.end = current_boundary_pos;
                        } else {
                            processed_ranges.push((start_pos..current_boundary_pos, color));
                        }
                    } else {
                        processed_ranges.push((start_pos..current_boundary_pos, color));
                    }
                }
            }
            while i < boundaries_len && boundaries[i].pos == current_boundary_pos {
                let active_range = &boundaries[i];
                if active_range.is_start {
                    let idx = active_range.index;
                    let pos = active_ranges
                        .binary_search_by_key(&idx, |(i, _)| *i)
                        .unwrap_or_else(|p| p);
                    active_ranges.insert(pos, (idx, active_range.color));
                } else {
                    let idx = active_range.index;
                    if let Ok(pos) = active_ranges.binary_search_by_key(&idx, |(i, _)| *i) {
                        active_ranges.remove(pos);
                    }
                }
                i += 1;
            }
            start_pos = current_boundary_pos;
        }

        processed_ranges
    }
}
