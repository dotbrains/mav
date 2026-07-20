use super::*;

impl Pane {
    pub(super) fn toggle_pin_tab(
        &mut self,
        _: &TogglePinTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.is_empty() {
            return;
        }
        let active_tab_ix = self.active_item_index();
        if self.is_tab_pinned(active_tab_ix) {
            self.unpin_tab_at(active_tab_ix, window, cx);
        } else {
            self.pin_tab_at(active_tab_ix, window, cx);
        }
    }

    pub(super) fn unpin_all_tabs(
        &mut self,
        _: &UnpinAllTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.is_empty() {
            return;
        }

        let pinned_item_ids = self.pinned_item_ids().into_iter().rev();

        for pinned_item_id in pinned_item_ids {
            if let Some(ix) = self.index_for_item_id(pinned_item_id) {
                self.unpin_tab_at(ix, window, cx);
            }
        }
    }

    pub(super) fn pin_tab_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.change_tab_pin_state(ix, PinOperation::Pin, window, cx);
    }

    pub(super) fn unpin_tab_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.change_tab_pin_state(ix, PinOperation::Unpin, window, cx);
    }

    fn change_tab_pin_state(
        &mut self,
        ix: usize,
        operation: PinOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let pane = cx.entity();

            let destination_index = match operation {
                PinOperation::Pin => self.pinned_tab_count.min(ix),
                PinOperation::Unpin => self.pinned_tab_count.checked_sub(1)?,
            };

            let id = self.item_for_index(ix)?.item_id();
            let should_activate = ix == self.active_item_index;

            if matches!(operation, PinOperation::Pin) {
                self.unpreview_item_if_preview(id);
            }

            match operation {
                PinOperation::Pin => self.pinned_tab_count += 1,
                PinOperation::Unpin => self.pinned_tab_count -= 1,
            }

            if ix == destination_index {
                cx.notify();
            } else {
                self.workspace
                    .update(cx, |_, cx| {
                        cx.defer_in(window, move |_, window, cx| {
                            move_item(
                                &pane,
                                &pane,
                                id,
                                destination_index,
                                should_activate,
                                window,
                                cx,
                            );
                        });
                    })
                    .ok()?;
            }

            let event = match operation {
                PinOperation::Pin => Event::ItemPinned,
                PinOperation::Unpin => Event::ItemUnpinned,
            };

            cx.emit(event);

            Some(())
        });
    }

    pub(super) fn is_tab_pinned(&self, ix: usize) -> bool {
        self.pinned_tab_count > ix
    }

    pub(super) fn has_unpinned_tabs(&self) -> bool {
        self.pinned_tab_count < self.items.len()
    }

    pub(super) fn activate_unpinned_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        let Some(index) = self
            .items()
            .enumerate()
            .find_map(|(index, _item)| (!self.is_tab_pinned(index)).then_some(index))
        else {
            return;
        };
        self.activate_item(index, true, true, window, cx);
    }
}
