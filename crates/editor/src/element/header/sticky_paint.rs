use super::*;

impl StickyHeaders {
    pub(super) fn paint(
        &mut self,
        layout: &mut EditorLayout,
        whitespace_setting: ShowWhitespaceSetting,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = layout.position_map.line_height;

        for line in self.lines.iter_mut().rev() {
            window.paint_layer(
                Bounds::new(
                    layout.gutter_hitbox.origin + point(Pixels::ZERO, line.offset),
                    size(line.hitbox.size.width, line_height),
                ),
                |window| {
                    let gutter_bounds = Bounds::new(
                        layout.gutter_hitbox.origin + point(Pixels::ZERO, line.offset),
                        size(layout.gutter_hitbox.size.width, line_height),
                    );
                    window.paint_quad(fill(gutter_bounds, self.gutter_background));

                    let text_bounds = Bounds::new(
                        layout.position_map.text_hitbox.origin + point(Pixels::ZERO, line.offset),
                        size(line.available_text_width, line_height),
                    );
                    window.paint_quad(fill(text_bounds, self.content_background));

                    if line.hitbox.is_hovered(window) {
                        let hover_overlay = cx.theme().colors().panel_overlay_hover;
                        window.paint_quad(fill(gutter_bounds, hover_overlay));
                        window.paint_quad(fill(text_bounds, hover_overlay));
                    }

                    line.paint(
                        layout,
                        self.gutter_right_padding,
                        line.available_text_width,
                        layout.content_origin,
                        line_height,
                        whitespace_setting,
                        window,
                        cx,
                    );
                },
            );

            window.set_cursor_style(CursorStyle::IBeam, &line.hitbox);
        }
    }
}

impl StickyHeaderLine {
    pub(super) fn new(
        row: DisplayRow,
        offset: Pixels,
        mut line: LineWithInvisibles,
        line_number: Option<ShapedLine>,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        gutter_hitbox: &Hitbox,
        text_hitbox: &Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let mut elements = SmallVec::<[AnyElement; 1]>::new();
        line.prepaint_with_custom_offset(
            line_height,
            scroll_pixel_position,
            content_origin,
            offset,
            &mut elements,
            window,
            cx,
        );

        let hitbox_bounds = Bounds::new(
            gutter_hitbox.origin + point(Pixels::ZERO, offset),
            size(text_hitbox.right() - gutter_hitbox.left(), line_height),
        );
        let available_text_width =
            (hitbox_bounds.size.width - gutter_hitbox.size.width).max(Pixels::ZERO);

        Self {
            row,
            offset,
            line: Rc::new(line),
            line_number,
            elements,
            available_text_width,
            hitbox: window.insert_hitbox(hitbox_bounds, HitboxBehavior::BlockMouseExceptScroll),
        }
    }

    fn paint(
        &mut self,
        layout: &EditorLayout,
        gutter_right_padding: Pixels,
        available_text_width: Pixels,
        content_origin: gpui::Point<Pixels>,
        line_height: Pixels,
        whitespace_setting: ShowWhitespaceSetting,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(
            Some(ContentMask::new(Bounds::new(
                layout.position_map.text_hitbox.bounds.origin + point(Pixels::ZERO, self.offset),
                size(available_text_width, line_height),
            ))),
            |window| {
                self.line.draw_with_custom_offset(
                    layout,
                    self.row,
                    content_origin,
                    self.offset,
                    whitespace_setting,
                    &[],
                    window,
                    cx,
                );
                for element in &mut self.elements {
                    element.paint(window, cx);
                }
            },
        );

        if let Some(line_number) = &self.line_number {
            let gutter_origin = layout.gutter_hitbox.origin + point(Pixels::ZERO, self.offset);
            let gutter_width = layout.gutter_hitbox.size.width;
            let origin = point(
                gutter_origin.x + gutter_width - gutter_right_padding - line_number.width,
                gutter_origin.y,
            );
            line_number
                .paint(origin, line_height, TextAlign::Left, None, window, cx)
                .log_err();
        }
    }
}
