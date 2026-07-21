use super::*;

impl EditorElement {
    pub(super) fn paint_impl(
        &mut self,
        bounds: Bounds<gpui::Pixels>,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !layout.mode.is_minimap() {
            let focus_handle = self.editor.focus_handle(cx);
            let key_context = self
                .editor
                .update(cx, |editor, cx| editor.key_context(window, cx));

            window.set_key_context(key_context);
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.editor.clone()),
                cx,
            );
            self.register_actions(window, cx);
            self.register_key_listeners(window, cx, layout);
        }

        let text_style = TextStyleRefinement {
            font_size: Some(self.style.text.font_size),
            line_height: Some(self.style.text.line_height),
            ..Default::default()
        };
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            window.with_text_style(Some(text_style), |window| {
                window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
                    self.paint_mouse_listeners(layout, window, cx);

                    // Mask the editor behind sticky scroll headers. Important
                    // for transparent backgrounds.
                    let below_sticky_headers_mask = layout
                        .sticky_headers
                        .as_ref()
                        .and_then(|h| h.lines.last())
                        .map(|last| {
                            ContentMask::new(Bounds {
                                origin: point(
                                    bounds.origin.x,
                                    bounds.origin.y + last.offset + layout.position_map.line_height,
                                ),
                                size: size(
                                    bounds.size.width,
                                    (bounds.size.height
                                        - last.offset
                                        - layout.position_map.line_height)
                                        .max(Pixels::ZERO),
                                ),
                            })
                        });

                    window.with_content_mask(below_sticky_headers_mask, |window| {
                        self.paint_background(layout, window, cx);

                        self.paint_indent_guides(layout, window, cx);

                        if layout.gutter_hitbox.size.width > Pixels::ZERO {
                            self.paint_blamed_display_rows(layout, window, cx);
                            self.paint_line_numbers(layout, window, cx);
                        }

                        self.paint_text(layout, window, cx);

                        if !layout.spacer_blocks.is_empty() {
                            window.with_element_namespace("blocks", |window| {
                                self.paint_spacer_blocks(layout, window, cx);
                            });
                        }

                        if layout.gutter_hitbox.size.width > Pixels::ZERO {
                            self.paint_gutter_highlights(layout, window, cx);
                            self.paint_gutter_indicators(layout, window, cx);
                        }

                        if !layout.blocks.is_empty() {
                            window.with_element_namespace("blocks", |window| {
                                self.paint_non_spacer_blocks(layout, window, cx);
                            });
                        }
                    });

                    window.with_element_namespace("blocks", |window| {
                        if let Some(mut sticky_header) = layout.sticky_buffer_header.take() {
                            sticky_header.paint(window, cx)
                        }
                    });

                    self.paint_sticky_headers(layout, window, cx);
                    self.paint_minimap(layout, window, cx);
                    self.paint_scrollbars(layout, window, cx);
                    self.paint_edit_prediction_popover(layout, window, cx);
                    self.paint_mouse_context_menu(layout, window, cx);
                });
            })
        })
    }
}
