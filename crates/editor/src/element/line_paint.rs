use super::*;

impl LineWithInvisibles {
    pub(super) fn prepaint(
        &mut self,
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        row: DisplayRow,
        content_origin: gpui::Point<Pixels>,
        line_elements: &mut SmallVec<[AnyElement; 1]>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_y = f32::from(line_height) * Pixels::from(row.as_f64() - scroll_position.y);
        self.prepaint_with_custom_offset(
            line_height,
            scroll_pixel_position,
            content_origin,
            line_y,
            line_elements,
            window,
            cx,
        );
    }

    pub(super) fn prepaint_with_custom_offset(
        &mut self,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        line_y: Pixels,
        line_elements: &mut SmallVec<[AnyElement; 1]>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut fragment_origin =
            content_origin + gpui::point(Pixels::from(-scroll_pixel_position.x), line_y);
        for fragment in &mut self.fragments {
            match fragment {
                LineFragment::Text(line) => {
                    fragment_origin.x += line.width;
                }
                LineFragment::Element { element, size, .. } => {
                    let mut element = element
                        .take()
                        .expect("you can't prepaint LineWithInvisibles twice");

                    let mut element_origin = fragment_origin;
                    element_origin.y += (line_height - size.height) / 2.;
                    element.prepaint_at(element_origin, window, cx);
                    line_elements.push(element);

                    fragment_origin.x += size.width;
                }
            }
        }
    }

    pub(super) fn draw(
        &self,
        layout: &EditorLayout,
        row: DisplayRow,
        content_origin: gpui::Point<Pixels>,
        whitespace_setting: ShowWhitespaceSetting,
        selection_ranges: &[Range<DisplayPoint>],
        window: &mut Window,
        cx: &mut App,
    ) {
        self.draw_with_custom_offset(
            layout,
            row,
            content_origin,
            layout.position_map.line_height
                * (row.as_f64() - layout.position_map.scroll_position.y) as f32,
            whitespace_setting,
            selection_ranges,
            window,
            cx,
        );
    }

    pub(super) fn draw_with_custom_offset(
        &self,
        layout: &EditorLayout,
        row: DisplayRow,
        content_origin: gpui::Point<Pixels>,
        line_y: Pixels,
        whitespace_setting: ShowWhitespaceSetting,
        selection_ranges: &[Range<DisplayPoint>],
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = layout.position_map.line_height;
        let mut fragment_origin = content_origin
            + gpui::point(
                Pixels::from(-layout.position_map.scroll_pixel_position.x),
                line_y,
            );

        for fragment in &self.fragments {
            match fragment {
                LineFragment::Text(line) => {
                    line.paint(
                        fragment_origin,
                        line_height,
                        layout.text_align,
                        Some(layout.content_width),
                        window,
                        cx,
                    )
                    .log_err();
                    fragment_origin.x += line.width;
                }
                LineFragment::Element { size, .. } => {
                    fragment_origin.x += size.width;
                }
            }
        }

        self.draw_invisibles(
            selection_ranges,
            layout,
            content_origin,
            line_y,
            row,
            line_height,
            whitespace_setting,
            window,
            cx,
        );
    }

    pub(super) fn draw_background(
        &self,
        layout: &EditorLayout,
        row: DisplayRow,
        content_origin: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = layout.position_map.line_height;
        let line_y = line_height * (row.as_f64() - layout.position_map.scroll_position.y) as f32;

        let mut fragment_origin = content_origin
            + gpui::point(
                Pixels::from(-layout.position_map.scroll_pixel_position.x),
                line_y,
            );

        for fragment in &self.fragments {
            match fragment {
                LineFragment::Text(line) => {
                    line.paint_background(
                        fragment_origin,
                        line_height,
                        layout.text_align,
                        Some(layout.content_width),
                        window,
                        cx,
                    )
                    .log_err();
                    fragment_origin.x += line.width;
                }
                LineFragment::Element { size, .. } => {
                    fragment_origin.x += size.width;
                }
            }
        }
    }

    fn draw_invisibles(
        &self,
        selection_ranges: &[Range<DisplayPoint>],
        layout: &EditorLayout,
        content_origin: gpui::Point<Pixels>,
        line_y: Pixels,
        row: DisplayRow,
        line_height: Pixels,
        whitespace_setting: ShowWhitespaceSetting,
        window: &mut Window,
        cx: &mut App,
    ) {
        let extract_whitespace_info = |invisible: &Invisible| {
            let (token_offset, token_end_offset, invisible_symbol) = match invisible {
                Invisible::Tab {
                    line_start_offset,
                    line_end_offset,
                } => (*line_start_offset, *line_end_offset, &layout.tab_invisible),
                Invisible::Whitespace {
                    line_start_offset,
                    line_end_offset,
                } => (
                    *line_start_offset,
                    *line_end_offset,
                    &layout.space_invisible,
                ),
            };

            let token_x = self.x_for_index(token_offset);
            let glyph_width = (self.x_for_index(token_end_offset) - token_x).max(Pixels::ZERO);
            let x_offset: ScrollPixelOffset = token_x.into();
            let invisible_offset: ScrollPixelOffset =
                ((glyph_width - invisible_symbol.width).max(Pixels::ZERO) / 2.0).into();
            let origin = content_origin
                + gpui::point(
                    Pixels::from(
                        x_offset + invisible_offset - layout.position_map.scroll_pixel_position.x,
                    ),
                    line_y,
                );

            (
                [token_offset, token_end_offset],
                Box::new(move |window: &mut Window, cx: &mut App| {
                    invisible_symbol
                        .paint(origin, line_height, TextAlign::Left, None, window, cx)
                        .log_err();
                }),
            )
        };

        let invisible_iter = self.invisibles.iter().map(extract_whitespace_info);
        match whitespace_setting {
            ShowWhitespaceSetting::None => (),
            ShowWhitespaceSetting::All => invisible_iter.for_each(|(_, paint)| paint(window, cx)),
            ShowWhitespaceSetting::Selection => invisible_iter.for_each(|([start, _], paint)| {
                let invisible_point = DisplayPoint::new(row, start as u32);
                if !selection_ranges
                    .iter()
                    .any(|region| region.start <= invisible_point && invisible_point < region.end)
                {
                    return;
                }

                paint(window, cx);
            }),

            ShowWhitespaceSetting::Trailing => {
                let mut previous_start = self.len;
                for ([start, end], paint) in invisible_iter.rev() {
                    if previous_start != end {
                        break;
                    }
                    previous_start = start;
                    paint(window, cx);
                }
            }

            ShowWhitespaceSetting::Boundary => {
                let mut last_seen: Option<(bool, usize, Box<dyn Fn(&mut Window, &mut App)>)> = None;
                for (([start, end], paint), invisible) in
                    invisible_iter.zip_eq(self.invisibles.iter())
                {
                    let should_render = match (&last_seen, invisible) {
                        (_, Invisible::Tab { .. }) => true,
                        (Some((_, last_end, _)), _) => *last_end == start,
                        _ => false,
                    };

                    if should_render || start == 0 || end == self.len {
                        paint(window, cx);

                        if let Some((should_render_last, last_end, paint_last)) = last_seen {
                            if !should_render_last && last_end == start {
                                paint_last(window, cx);
                            }
                        }
                    }

                    let invisible_point = DisplayPoint::new(row, start as u32);
                    if selection_ranges.iter().any(|region| {
                        region.start <= invisible_point && invisible_point < region.end
                    }) {
                        paint(window, cx);
                    }

                    last_seen = Some((should_render, end, paint));
                }
            }
        }
    }
}

impl EditorElement {
    pub(super) fn paint_text(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(
            Some(ContentMask::new(layout.position_map.text_hitbox.bounds)),
            |window| {
                let editor = self.editor.read(cx);
                if let SelectionDragState::ReadyToDrag {
                    mouse_down_time, ..
                } = &editor.selection_drag_state
                {
                    let drag_and_drop_delay = Duration::from_millis(
                        EditorSettings::get_global(cx)
                            .drag_and_drop_selection
                            .delay
                            .0,
                    );
                    if mouse_down_time.elapsed() >= drag_and_drop_delay {
                        window.set_cursor_style(
                            CursorStyle::DragCopy,
                            &layout.position_map.text_hitbox,
                        );
                    }
                } else if matches!(
                    editor.selection_drag_state,
                    SelectionDragState::Dragging { .. }
                ) {
                    window
                        .set_cursor_style(CursorStyle::DragCopy, &layout.position_map.text_hitbox);
                } else if editor
                    .hovered_link_state
                    .as_ref()
                    .is_some_and(|hovered_link_state| !hovered_link_state.links.is_empty())
                {
                    window.set_cursor_style(
                        CursorStyle::PointingHand,
                        &layout.position_map.text_hitbox,
                    );
                } else {
                    window.set_cursor_style(CursorStyle::IBeam, &layout.position_map.text_hitbox);
                };

                self.paint_lines_background(layout, window, cx);
                let invisible_display_ranges = self.paint_highlights(layout, window, cx);
                self.paint_document_colors(layout, window);
                self.paint_lines(&invisible_display_ranges, layout, window, cx);
                self.paint_redactions(layout, window);
                self.paint_navigation_overlays(layout, window, cx);
                self.paint_cursors(layout, window, cx);
                self.paint_inline_diagnostics(layout, window, cx);
                self.paint_inline_blame(layout, window, cx);
                self.paint_inline_code_actions(layout, window, cx);
                self.paint_diff_hunk_controls(layout, window, cx);
                window.with_element_namespace("crease_trailers", |window| {
                    for trailer in layout.crease_trailers.iter_mut().flatten() {
                        trailer.element.paint(window, cx);
                    }
                });
            },
        )
    }

    pub(super) fn paint_highlights(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) -> SmallVec<[Range<DisplayPoint>; 32]> {
        window.paint_layer(layout.position_map.text_hitbox.bounds, |window| {
            let mut invisible_display_ranges = SmallVec::<[Range<DisplayPoint>; 32]>::new();
            let line_end_overshoot = 0.15 * layout.position_map.line_height;
            for (range, color) in &layout.highlighted_ranges {
                self.paint_highlighted_range(
                    range.clone(),
                    true,
                    *color,
                    Pixels::ZERO,
                    line_end_overshoot,
                    layout,
                    window,
                );
            }

            let corner_radius = if EditorSettings::get_global(cx).rounded_selection {
                0.15 * layout.position_map.line_height
            } else {
                Pixels::ZERO
            };

            for (player_color, selections) in &layout.selections {
                for selection in selections.iter() {
                    self.paint_highlighted_range(
                        selection.range.clone(),
                        true,
                        player_color.selection,
                        corner_radius,
                        corner_radius * 2.,
                        layout,
                        window,
                    );

                    if selection.is_local && !selection.range.is_empty() {
                        invisible_display_ranges.push(selection.range.clone());
                    }
                }
            }
            invisible_display_ranges
        })
    }

    pub(super) fn paint_lines(
        &mut self,
        invisible_display_ranges: &[Range<DisplayPoint>],
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let whitespace_setting = self
            .editor
            .read(cx)
            .buffer
            .read(cx)
            .language_settings(cx)
            .show_whitespaces;

        for (ix, line_with_invisibles) in layout.position_map.line_layouts.iter().enumerate() {
            let row = DisplayRow(layout.visible_display_row_range.start.0 + ix as u32);
            line_with_invisibles.draw(
                layout,
                row,
                layout.content_origin,
                whitespace_setting,
                invisible_display_ranges,
                window,
                cx,
            )
        }

        for line_element in &mut layout.line_elements {
            line_element.paint(window, cx);
        }
    }

    pub(super) fn paint_lines_background(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (ix, line_with_invisibles) in layout.position_map.line_layouts.iter().enumerate() {
            let row = DisplayRow(layout.visible_display_row_range.start.0 + ix as u32);
            line_with_invisibles.draw_background(layout, row, layout.content_origin, window, cx);
        }
    }

    pub(super) fn paint_redactions(&mut self, layout: &EditorLayout, window: &mut Window) {
        if layout.redacted_ranges.is_empty() {
            return;
        }

        let line_end_overshoot = layout.line_end_overshoot();

        // A softer than perfect black
        let redaction_color = gpui::rgb(0x0e1111);

        window.paint_layer(layout.position_map.text_hitbox.bounds, |window| {
            for range in layout.redacted_ranges.iter() {
                self.paint_highlighted_range(
                    range.clone(),
                    true,
                    redaction_color.into(),
                    Pixels::ZERO,
                    line_end_overshoot,
                    layout,
                    window,
                );
            }
        });
    }
}
