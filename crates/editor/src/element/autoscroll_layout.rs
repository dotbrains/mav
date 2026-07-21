use super::*;

impl EditorElement {
    pub(super) fn layout_vertical_autoscroll(
        &self,
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        max_scroll_top: ScrollOffset,
        snapshot: &mut EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::VerticalAutoscroll {
        self.editor.update(cx, |editor, cx| {
            let autoscroll_request = editor.scroll_manager.take_autoscroll_request();

            let autoscroll_containing_element =
                autoscroll_request.is_some() || editor.has_pending_selection();

            let (needs_horizontal_autoscroll, was_scrolled) = editor.autoscroll_vertically(
                bounds,
                line_height,
                max_scroll_top,
                autoscroll_request,
                window,
                cx,
            );
            if was_scrolled.0 {
                *snapshot = editor.snapshot(window, cx);
            }

            layout_data::VerticalAutoscroll {
                autoscroll_request,
                autoscroll_containing_element,
                needs_horizontal_autoscroll,
            }
        })
    }
}
