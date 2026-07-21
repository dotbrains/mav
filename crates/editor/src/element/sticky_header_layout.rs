use super::*;

impl EditorElement {
    pub(super) fn layout_sticky_headers_and_guides(
        &self,
        is_minimap: bool,
        is_singleton: bool,
        snapshot: &EditorSnapshot,
        editor_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        preliminary_scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        gutter_dimensions: &GutterDimensions,
        gutter_hitbox: &Hitbox,
        text_hitbox: &Hitbox,
        current_selection_head: Option<DisplayRow>,
        buffer_rows: Range<MultiBufferRow>,
        indent_guides: Option<Vec<IndentGuideLayout>>,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::StickyHeaderLayouts {
        let sticky_headers = if !is_minimap
            && is_singleton
            && EditorSettings::get_global(cx).sticky_scroll.enabled
        {
            let relative = self.editor.read(cx).relative_line_numbers(cx);
            self.layout_sticky_headers(
                snapshot,
                editor_width,
                is_row_soft_wrapped,
                line_height,
                scroll_pixel_position,
                content_origin,
                gutter_dimensions,
                gutter_hitbox,
                text_hitbox,
                relative,
                current_selection_head,
                window,
                cx,
            )
        } else {
            None
        };

        let indent_guides = if scroll_pixel_position != preliminary_scroll_pixel_position {
            self.layout_indent_guides(
                content_origin,
                text_hitbox.origin,
                buffer_rows,
                scroll_pixel_position,
                line_height,
                snapshot,
                window,
                cx,
            )
        } else {
            indent_guides
        };

        layout_data::StickyHeaderLayouts {
            sticky_headers,
            indent_guides,
        }
    }
}
