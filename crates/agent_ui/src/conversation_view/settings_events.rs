use super::*;

impl ConversationView {
    pub(super) fn agent_ui_font_size_changed(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry_view_state) = self
            .active_thread()
            .map(|active| active.read(cx).entry_view_state.clone())
        {
            entry_view_state.update(cx, |entry_view_state, cx| {
                entry_view_state.agent_ui_font_size_changed(cx);
            });
        }
    }

    pub(super) fn invalidate_mermaid_caches(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_theme_id = cx.theme().id.clone();
        if self.last_theme_id.as_ref() == Some(&current_theme_id) {
            return;
        }
        self.last_theme_id = Some(current_theme_id);

        if let Some(connected) = self.as_connected() {
            let threads: Vec<_> = connected
                .conversation
                .read(cx)
                .threads
                .values()
                .cloned()
                .collect();
            for thread in threads {
                thread.update(cx, |thread, cx| {
                    thread.invalidate_mermaid_caches(cx);
                });
            }
        }
    }
}
