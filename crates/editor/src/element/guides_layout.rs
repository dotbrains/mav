use super::*;

impl EditorElement {
    pub(super) fn layout_indent_guides(
        &self,
        content_origin: gpui::Point<Pixels>,
        text_origin: gpui::Point<Pixels>,
        visible_buffer_range: Range<MultiBufferRow>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        snapshot: &DisplaySnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Vec<IndentGuideLayout>> {
        let indent_guides = self.editor.update(cx, |editor, cx| {
            editor.indent_guides(visible_buffer_range, snapshot, cx)
        })?;

        let active_indent_guide_indices = self.editor.update(cx, |editor, cx| {
            editor
                .find_active_indent_guide_indices(&indent_guides, snapshot, window, cx)
                .unwrap_or_default()
        });

        Some(
            indent_guides
                .into_iter()
                .enumerate()
                .filter_map(|(i, indent_guide)| {
                    let single_indent_width =
                        column_pixels(&self.style, indent_guide.tab_size as usize, window);
                    let total_width = single_indent_width * indent_guide.depth as f32;
                    let start_x = Pixels::from(
                        ScrollOffset::from(content_origin.x + total_width)
                            - scroll_pixel_position.x,
                    );
                    if start_x >= text_origin.x {
                        let (offset_y, length, display_row_range) =
                            Self::calculate_indent_guide_bounds(
                                indent_guide.start_row..indent_guide.end_row,
                                line_height,
                                snapshot,
                            );

                        let start_y = Pixels::from(
                            ScrollOffset::from(content_origin.y) + offset_y
                                - scroll_pixel_position.y,
                        );

                        Some(IndentGuideLayout {
                            origin: point(start_x, start_y),
                            length,
                            single_indent_width,
                            display_row_range,
                            depth: indent_guide.depth,
                            active: active_indent_guide_indices.contains(&i),
                            settings: indent_guide.settings,
                        })
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }

    pub(super) fn depth_zero_indent_guide_padding_for_row(
        indent_guides: &[IndentGuideLayout],
        row: DisplayRow,
    ) -> Pixels {
        indent_guides
            .iter()
            .find(|guide| guide.depth == 0 && guide.display_row_range.contains(&row))
            .and_then(|guide| {
                guide
                    .settings
                    .visible_line_width(guide.active)
                    .map(|width| px(width as f32 * 2.0))
            })
            .unwrap_or(px(0.0))
    }

    pub(super) fn layout_wrap_guides(
        &self,
        em_advance: Pixels,
        scroll_position: gpui::Point<f64>,
        content_origin: gpui::Point<Pixels>,
        scrollbar_layout: Option<&EditorScrollbars>,
        vertical_scrollbar_width: Pixels,
        hitbox: &Hitbox,
        window: &Window,
        cx: &App,
    ) -> SmallVec<[(Pixels, bool); 2]> {
        let scroll_left = scroll_position.x as f32 * em_advance;
        let content_origin = content_origin.x;
        let horizontal_offset = content_origin - scroll_left;
        let vertical_scrollbar_width = scrollbar_layout
            .and_then(|layout| layout.visible.then_some(vertical_scrollbar_width))
            .unwrap_or_default();

        self.editor
            .read(cx)
            .wrap_guides(cx)
            .into_iter()
            .flat_map(|(guide, active)| {
                let wrap_position = column_pixels(&self.style, guide, window);
                let wrap_guide_x = wrap_position + horizontal_offset;
                let display_wrap_guide = wrap_guide_x >= content_origin
                    && wrap_guide_x <= hitbox.bounds.right() - vertical_scrollbar_width;

                display_wrap_guide.then_some((wrap_guide_x, active))
            })
            .collect()
    }

    fn calculate_indent_guide_bounds(
        row_range: Range<MultiBufferRow>,
        line_height: Pixels,
        snapshot: &DisplaySnapshot,
    ) -> (f64, gpui::Pixels, Range<DisplayRow>) {
        let start_point = Point::new(row_range.start.0, 0);
        let end_point = Point::new(row_range.end.0, 0);

        let mut row_range = start_point.to_display_point(snapshot).row()
            ..end_point.to_display_point(snapshot).row();

        let mut prev_line = start_point;
        prev_line.row = prev_line.row.saturating_sub(1);
        let prev_line = prev_line.to_display_point(snapshot).row();

        let mut cons_line = end_point;
        cons_line.row += 1;
        let cons_line = cons_line.to_display_point(snapshot).row();

        let mut offset_y = row_range.start.as_f64() * f64::from(line_height);
        let mut length = (cons_line.0.saturating_sub(row_range.start.0)) as f32 * line_height;

        // If we are at the end of the buffer, ensure that the indent guide extends to the end of the line.
        if row_range.end == cons_line {
            length += line_height;
        }

        // If there is a block (e.g. diagnostic) in between the start of the indent guide and the line above,
        // we want to extend the indent guide to the start of the block.
        let mut block_height = 0;
        let mut block_offset = 0;
        let mut found_excerpt_header = false;
        for (_, block) in snapshot.blocks_in_range(prev_line..row_range.start) {
            if matches!(
                block,
                Block::ExcerptBoundary { .. } | Block::BufferHeader { .. }
            ) {
                found_excerpt_header = true;
                break;
            }
            block_offset += block.height();
            block_height += block.height();
        }
        if !found_excerpt_header {
            offset_y -= block_offset as f64 * f64::from(line_height);
            length += block_height as f32 * line_height;
            row_range = DisplayRow(row_range.start.0.saturating_sub(block_offset))..row_range.end;
        }

        // If there is a block (e.g. diagnostic) at the end of an multibuffer excerpt,
        // we want to ensure that the indent guide stops before the excerpt header.
        let mut block_height = 0;
        let mut found_excerpt_header = false;
        for (_, block) in snapshot.blocks_in_range(row_range.end..cons_line) {
            if matches!(
                block,
                Block::ExcerptBoundary { .. } | Block::BufferHeader { .. }
            ) {
                found_excerpt_header = true;
            }
            block_height += block.height();
        }
        if found_excerpt_header {
            length -= block_height as f32 * line_height;
        } else {
            row_range = row_range.start..cons_line;
        }

        (offset_y, length, row_range)
    }
}

impl EditorElement {
    pub(super) fn paint_indent_guides(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(indent_guides) = &layout.indent_guides else {
            return;
        };

        let faded_color = |color: Hsla, alpha: f32| {
            let mut faded = color;
            faded.a = alpha;
            faded
        };

        for indent_guide in indent_guides {
            let indent_accent_colors = cx.theme().accents().color_for_index(indent_guide.depth);
            let settings = &indent_guide.settings;

            // TODO fixed for now, expose them through themes later
            const INDENT_AWARE_ALPHA: f32 = 0.2;
            const INDENT_AWARE_ACTIVE_ALPHA: f32 = 0.4;
            const INDENT_AWARE_BACKGROUND_ALPHA: f32 = 0.1;
            const INDENT_AWARE_BACKGROUND_ACTIVE_ALPHA: f32 = 0.2;

            let line_color = match (settings.coloring, indent_guide.active) {
                (IndentGuideColoring::Disabled, _) => None,
                (IndentGuideColoring::Fixed, false) => {
                    Some(cx.theme().colors().editor_indent_guide)
                }
                (IndentGuideColoring::Fixed, true) => {
                    Some(cx.theme().colors().editor_indent_guide_active)
                }
                (IndentGuideColoring::IndentAware, false) => {
                    Some(faded_color(indent_accent_colors, INDENT_AWARE_ALPHA))
                }
                (IndentGuideColoring::IndentAware, true) => {
                    Some(faded_color(indent_accent_colors, INDENT_AWARE_ACTIVE_ALPHA))
                }
            };

            let background_color = match (settings.background_coloring, indent_guide.active) {
                (IndentGuideBackgroundColoring::Disabled, _) => None,
                (IndentGuideBackgroundColoring::IndentAware, false) => Some(faded_color(
                    indent_accent_colors,
                    INDENT_AWARE_BACKGROUND_ALPHA,
                )),
                (IndentGuideBackgroundColoring::IndentAware, true) => Some(faded_color(
                    indent_accent_colors,
                    INDENT_AWARE_BACKGROUND_ACTIVE_ALPHA,
                )),
            };

            let mut line_indicator_width = 0.;
            if let Some(requested_line_width) = settings.visible_line_width(indent_guide.active) {
                if let Some(color) = line_color {
                    window.paint_quad(fill(
                        window.pixel_snap_bounds(Bounds {
                            origin: indent_guide.origin,
                            size: size(px(requested_line_width as f32), indent_guide.length),
                        }),
                        color,
                    ));
                    line_indicator_width = requested_line_width as f32;
                }
            }

            if let Some(color) = background_color {
                let width = indent_guide.single_indent_width - px(line_indicator_width);
                window.paint_quad(fill(
                    window.pixel_snap_bounds(Bounds {
                        origin: point(
                            indent_guide.origin.x + px(line_indicator_width),
                            indent_guide.origin.y,
                        ),
                        size: size(width, indent_guide.length),
                    }),
                    color,
                ));
            }
        }
    }
}
