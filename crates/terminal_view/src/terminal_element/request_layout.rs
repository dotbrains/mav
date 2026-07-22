use super::*;

impl TerminalElement {
    pub(super) fn request_terminal_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let height: Length = match self.terminal_view.read(cx).content_mode(window, cx) {
            ContentMode::Inline {
                displayed_lines,
                total_lines: _,
            } => {
                let rem_size = window.rem_size();
                let line_height = f32::from(window.text_style().font_size.to_pixels(rem_size))
                    * TerminalSettings::get_global(cx).line_height.value();
                px(displayed_lines as f32 * line_height).into()
            }
            ContentMode::Scrollable => {
                if let TerminalMode::Embedded { .. } = &self.mode {
                    let term = self.terminal.read(cx);
                    if !term.scrolled_to_top() && !term.scrolled_to_bottom() && self.focused {
                        self.interactivity.occlude_mouse();
                    }
                }

                relative(1.).into()
            }
        };

        let layout_id = self.interactivity.request_layout(
            global_id,
            inspector_id,
            window,
            cx,
            |mut style, window, cx| {
                style.size.width = relative(1.).into();
                style.size.height = height;

                window.request_layout(style, None, cx)
            },
        );
        (layout_id, ())
    }
}
