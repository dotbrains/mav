use super::*;

impl ThreadView {
    pub(crate) fn scroll_to_most_recent_user_prompt(&mut self, cx: &mut Context<Self>) {
        let entries = self.thread.read(cx).entries();
        if entries.is_empty() {
            return;
        }

        // Find the most recent user message and scroll it to the top of the viewport.
        // (Fallback: if no user message exists, scroll to the bottom.)
        if let Some(ix) = entries
            .iter()
            .rposition(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
        {
            self.list_state.scroll_to(ListOffset {
                item_ix: ix,
                offset_in_item: px(0.0),
            });
            cx.notify();
        } else {
            self.scroll_to_end(cx);
        }
    }

    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.list_state.scroll_to_end();
        cx.notify();
    }

    pub(super) fn handle_feedback_click(
        &mut self,
        feedback: ThreadFeedback,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.thread_feedback
            .submit(self.thread.clone(), feedback, window, cx);
        cx.notify();
    }

    pub(super) fn submit_feedback_message(&mut self, cx: &mut Context<Self>) {
        let thread = self.thread.clone();
        self.thread_feedback.submit_comments(thread, cx);
        cx.notify();
    }

    pub(crate) fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.list_state.scroll_to(ListOffset::default());
        cx.notify();
    }

    pub(super) fn scroll_output_page_up(
        &mut self,
        _: &ScrollOutputPageUp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page_height = self.list_state.viewport_bounds().size.height;
        self.list_state.scroll_by(-page_height * 0.9);
        cx.notify();
    }

    pub(super) fn scroll_output_page_down(
        &mut self,
        _: &ScrollOutputPageDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page_height = self.list_state.viewport_bounds().size.height;
        self.list_state.scroll_by(page_height * 0.9);
        cx.notify();
    }

    pub(super) fn scroll_output_line_up(
        &mut self,
        _: &ScrollOutputLineUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list_state.scroll_by(-window.line_height() * 3.);
        cx.notify();
    }

    pub(super) fn scroll_output_line_down(
        &mut self,
        _: &ScrollOutputLineDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list_state.scroll_by(window.line_height() * 3.);
        cx.notify();
    }

    pub(super) fn scroll_output_to_top(
        &mut self,
        _: &ScrollOutputToTop,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_to_top(cx);
    }

    pub(super) fn scroll_output_to_bottom(
        &mut self,
        _: &ScrollOutputToBottom,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_to_end(cx);
    }

    pub(super) fn scroll_output_to_previous_message(
        &mut self,
        _: &ScrollOutputToPreviousMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.thread.read(cx).entries();
        let current_ix = self.list_state.logical_scroll_top().item_ix;
        if let Some(target_ix) = (0..current_ix)
            .rev()
            .find(|&i| matches!(entries.get(i), Some(AgentThreadEntry::UserMessage(_))))
        {
            self.list_state.scroll_to(ListOffset {
                item_ix: target_ix,
                offset_in_item: px(0.),
            });
            cx.notify();
        }
    }

    pub(super) fn scroll_output_to_next_message(
        &mut self,
        _: &ScrollOutputToNextMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.thread.read(cx).entries();
        let current_ix = self.list_state.logical_scroll_top().item_ix;
        if let Some(target_ix) = (current_ix + 1..entries.len())
            .find(|&i| matches!(entries.get(i), Some(AgentThreadEntry::UserMessage(_))))
        {
            self.list_state.scroll_to(ListOffset {
                item_ix: target_ix,
                offset_in_item: px(0.),
            });
            cx.notify();
        }
    }

    pub(super) fn refresh_thread_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.thread_search_visible {
            return;
        }
        if let Some(bar) = self.thread_search_bar.clone() {
            bar.update(cx, |bar, cx| bar.update_matches(window, cx));
        }
    }

    /// Hides the thread search bar, clears its highlights, and returns focus to
    /// the message editor. Returns `true` if the search bar was visible.
    pub(crate) fn close_thread_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.thread_search_visible {
            return false;
        }

        if let Some(bar) = self.thread_search_bar.clone() {
            bar.update(cx, |bar, cx| bar.clear_highlights(cx));
        }

        self.thread_search_visible = false;
        self.message_editor.focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn toggle_search(
        &mut self,
        _: &crate::ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_search_bar.is_none() {
            let thread = self.thread.clone();
            let view = cx.entity().downgrade();
            let on_activate =
                Arc::new(move |entry_ix: usize, _window: &mut Window, cx: &mut App| {
                    // Avoid re-entering `ThreadView` when search navigation is forwarded
                    // from a `ThreadView` action handler.
                    let view = view.clone();
                    cx.defer(move |cx| {
                        view.update(cx, |this, cx| {
                            this.list_state.scroll_to(gpui::ListOffset {
                                item_ix: entry_ix,
                                offset_in_item: gpui::px(0.),
                            });
                            cx.notify();
                        })
                        .ok();
                    });
                });
            let search_bar = cx.new(|cx| {
                ThreadSearchBar::new(
                    thread,
                    self.entry_view_state.clone(),
                    on_activate,
                    window,
                    cx,
                )
            });
            self._subscriptions.push(cx.subscribe_in(
                &search_bar,
                window,
                |this, _bar, event, window, cx| {
                    if matches!(event, ThreadSearchBarEvent::Dismissed) {
                        this.thread_search_visible = false;
                        this.message_editor.focus_handle(cx).focus(window, cx);
                        cx.notify();
                    }
                },
            ));
            self.thread_search_bar = Some(search_bar);
        }

        let search_bar_focused = self
            .thread_search_bar
            .as_ref()
            .is_some_and(|bar| bar.focus_handle(cx).contains_focused(window, cx));

        if self.thread_search_visible && search_bar_focused {
            if let Some(bar) = &self.thread_search_bar {
                bar.update(cx, |bar, cx| bar.clear_highlights(cx));
            }
            self.thread_search_visible = false;
            self.message_editor.focus_handle(cx).focus(window, cx);
            cx.notify();
        } else {
            self.thread_search_visible = true;
            if let Some(bar) = self.thread_search_bar.clone() {
                bar.update(cx, |bar, cx| bar.focus_and_refresh(window, cx));
            }
            cx.notify();
        }
    }
}
