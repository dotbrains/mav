use super::*;

impl Pane {
    pub fn toggle_zoom(&mut self, _: &ToggleZoom, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_toggle_zoom {
            cx.propagate();
        } else if self.zoomed {
            cx.emit(Event::ZoomOut);
        } else if !self.items.is_empty() {
            if !self.focus_handle.contains_focused(window, cx) {
                cx.focus_self(window);
            }
            cx.emit(Event::ZoomIn);
        }
    }

    pub fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_toggle_zoom {
            cx.propagate();
        } else if !self.zoomed && !self.items.is_empty() {
            if !self.focus_handle.contains_focused(window, cx) {
                cx.focus_self(window);
            }
            cx.emit(Event::ZoomIn);
        }
    }

    pub fn zoom_out(&mut self, _: &ZoomOut, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_toggle_zoom {
            cx.propagate();
        } else if self.zoomed {
            cx.emit(Event::ZoomOut);
        }
    }

    pub fn activate_item(
        &mut self,
        index: usize,
        activate_pane: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use NavigationMode::{GoingBack, GoingForward};
        if index < self.items.len() {
            let prev_active_item_ix = mem::replace(&mut self.active_item_index, index);
            let active_item_changed = prev_active_item_ix != self.active_item_index;
            if active_item_changed || matches!(self.nav_history.mode(), GoingBack | GoingForward) {
                if let Some(prev_item) = self.items.get(prev_active_item_ix) {
                    prev_item.deactivated(window, cx);
                }
            }
            if active_item_changed {
                if let Some(active_item) = self.items.get(self.active_item_index) {
                    active_item.activated(window, cx);
                }
            }
            self.update_history(index);
            self.update_toolbar(window, cx);
            self.update_status_bar(window, cx);

            if focus_item {
                self.focus_active_item(window, cx);
            }

            cx.emit(Event::ActivateItem {
                local: activate_pane,
                focus_changed: focus_item,
            });

            self.update_active_tab(index);
            cx.notify();
        }
    }

    pub(super) fn update_active_tab(&mut self, index: usize) {
        if !self.is_tab_pinned(index) {
            self.suppress_scroll = false;
            self.tab_bar_scroll_handle
                .scroll_to_item(index - self.pinned_tab_count);
        }
    }

    pub(super) fn update_history(&mut self, index: usize) {
        if let Some(newly_active_item) = self.items.get(index) {
            self.activation_history
                .retain(|entry| entry.entity_id != newly_active_item.item_id());
            self.activation_history.push(ActivationHistoryEntry {
                entity_id: newly_active_item.item_id(),
                timestamp: self
                    .next_activation_timestamp
                    .fetch_add(1, Ordering::SeqCst),
            });
        }
    }

    pub fn activate_previous_item(
        &mut self,
        action: &ActivatePreviousItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut index = self.active_item_index;
        if index > 0 {
            index -= 1;
        } else if action.wrap_around && !self.items.is_empty() {
            index = self.items.len() - 1;
        }
        self.activate_item(index, true, true, window, cx);
    }

    pub fn activate_next_item(
        &mut self,
        action: &ActivateNextItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut index = self.active_item_index;
        if index + 1 < self.items.len() {
            index += 1;
        } else if action.wrap_around {
            index = 0;
        }
        self.activate_item(index, true, true, window, cx);
    }

    pub fn swap_item_left(
        &mut self,
        _: &SwapItemLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.active_item_index;
        if index == 0 {
            return;
        }

        self.items.swap(index, index - 1);
        self.activate_item(index - 1, true, true, window, cx);
    }

    pub fn swap_item_right(
        &mut self,
        _: &SwapItemRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.active_item_index;
        if index + 1 >= self.items.len() {
            return;
        }

        self.items.swap(index, index + 1);
        self.activate_item(index + 1, true, true, window, cx);
    }

    pub fn activate_last_item(
        &mut self,
        _: &ActivateLastItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.items.len().saturating_sub(1);
        self.activate_item(index, true, true, window, cx);
    }
}
