use super::*;

impl EditorElement {
    pub(super) fn layout_scroll_position(
        &self,
        mut scroll_position: gpui::Point<ScrollOffset>,
        max_scroll_top: ScrollOffset,
        start_row: DisplayRow,
        editor_width: Pixels,
        scroll_width: Pixels,
        em_advance: Pixels,
        em_layout_width: Pixels,
        line_height: Pixels,
        line_layouts: &[LineWithInvisibles],
        needs_horizontal_autoscroll: NeedsHorizontalAutoscroll,
        autoscroll_request: Option<(Autoscroll, bool)>,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::ScrollPositionLayout {
        let scroll_max: gpui::Point<ScrollPixelOffset> = point(
            ScrollPixelOffset::from(((scroll_width - editor_width) / em_layout_width).max(0.0)),
            max_scroll_top,
        );

        self.editor.update(cx, |editor, cx| {
            if editor.scroll_manager.clamp_scroll_left(scroll_max.x, cx) {
                scroll_position.x = scroll_max.x.min(scroll_position.x);
            }

            if needs_horizontal_autoscroll.0
                && let Some(new_scroll_position) = editor.autoscroll_horizontally(
                    start_row,
                    editor_width,
                    scroll_width,
                    em_advance,
                    line_layouts,
                    autoscroll_request,
                    window,
                    cx,
                )
            {
                scroll_position.x = new_scroll_position.x;
            }
        });

        if !em_layout_width.is_zero() {
            scroll_position.x = window
                .pixel_snap_f64(scroll_position.x * f64::from(em_layout_width))
                / f64::from(em_layout_width);
        }

        let scroll_pixel_position = point(
            scroll_position.x * f64::from(em_layout_width),
            scroll_position.y * f64::from(line_height),
        );

        layout_data::ScrollPositionLayout {
            scroll_position,
            scroll_pixel_position,
            scroll_max,
        }
    }
}
