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
