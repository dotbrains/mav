use super::*;

impl EditorElement {
    pub(super) fn layout_cursor_surface(
        &self,
        snapshot: &EditorSnapshot,
        selections: &[(PlayerColor, Vec<SelectionLayout>)],
        row_block_types: &HashMap<DisplayRow, bool>,
        visible_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        text_hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        em_width: Pixels,
        em_advance: Pixels,
        autoscroll_containing_element: bool,
        redacted_ranges: &[Range<DisplayPoint>],
        scrollbar_layout_information: &ScrollbarLayoutInformation,
        content_offset: gpui::Point<Pixels>,
        right_margin: Pixels,
        editor_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::CursorSurfaceLayouts {
        let cursors = self.collect_cursors(snapshot, cx);
        let non_visible_cursors = cursors
            .iter()
            .any(|c| !visible_row_range.contains(&c.0.row()));

        let visible_cursors = self.layout_visible_cursors(
            snapshot,
            selections,
            row_block_types,
            visible_row_range.clone(),
            line_layouts,
            text_hitbox,
            content_origin,
            scroll_position,
            scroll_pixel_position,
            line_height,
            em_width,
            em_advance,
            autoscroll_containing_element,
            redacted_ranges,
            window,
            cx,
        );
        let navigation_overlay_paint_commands = self.layout_navigation_overlays(
            snapshot,
            visible_row_range,
            line_layouts,
            text_hitbox,
            content_origin,
            scroll_position,
            scroll_pixel_position,
            line_height,
            window,
            cx,
        );

        let scrollbars_layout = self.layout_scrollbars(
            snapshot,
            scrollbar_layout_information,
            content_offset,
            scroll_position,
            non_visible_cursors,
            right_margin,
            editor_width,
            window,
            cx,
        );

        layout_data::CursorSurfaceLayouts {
            cursors,
            visible_cursors,
            navigation_overlay_paint_commands,
            scrollbars_layout,
        }
    }
}
