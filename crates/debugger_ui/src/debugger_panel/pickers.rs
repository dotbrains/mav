use super::*;

impl DebugPanel {
    pub(crate) fn toggle_thread_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.thread_picker_menu_handle.toggle(window, cx);
    }

    pub(crate) fn toggle_session_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session_picker_menu_handle.toggle(window, cx);
    }

    pub(super) fn toggle_zoom(
        &mut self,
        _: &workspace::ToggleZoom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_zoomed {
            cx.emit(PanelEvent::ZoomOut);
        } else {
            if !self.focus_handle(cx).contains_focused(window, cx) {
                cx.focus_self(window);
            }
            cx.emit(PanelEvent::ZoomIn);
        }
    }
}
