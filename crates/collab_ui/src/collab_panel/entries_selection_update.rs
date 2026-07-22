use super::*;

impl CollabPanel {
    fn restore_selection_and_scroll(
        &mut self,
        select_same_item: bool,
        prev_selected_entry: Option<ListEntry>,
        old_entries: Vec<ListEntry>,
        scroll_to_top: bool,
    ) {
        if select_same_item {
            if let Some(prev_selected_entry) = prev_selected_entry {
                let prev_selection = self.selection.take();
                for (ix, entry) in self.entries.iter().enumerate() {
                    if *entry == prev_selected_entry {
                        self.selection = Some(ix);
                        break;
                    }
                }
                if self.selection.is_none() {
                    self.selection = prev_selection.and_then(|prev_ix| {
                        if self.entries.is_empty() {
                            None
                        } else {
                            Some(prev_ix.min(self.entries.len() - 1))
                        }
                    });
                }
            }
        } else {
            self.selection = self.selection.and_then(|prev_selection| {
                if self.entries.is_empty() {
                    None
                } else {
                    Some(prev_selection.min(self.entries.len() - 1))
                }
            });
        }

        let old_scroll_top = self.list_state.logical_scroll_top();
        self.list_state.reset(self.entries.len());

        if scroll_to_top {
            self.list_state.scroll_to(ListOffset::default());
        } else {
            // Attempt to maintain the same scroll position.
            if let Some(old_top_entry) = old_entries.get(old_scroll_top.item_ix) {
                let new_scroll_top = self
                    .entries
                    .iter()
                    .position(|entry| entry == old_top_entry)
                    .map(|item_ix| ListOffset {
                        item_ix,
                        offset_in_item: old_scroll_top.offset_in_item,
                    })
                    .or_else(|| {
                        let entry_after_old_top = old_entries.get(old_scroll_top.item_ix + 1)?;
                        let item_ix = self
                            .entries
                            .iter()
                            .position(|entry| entry == entry_after_old_top)?;
                        Some(ListOffset {
                            item_ix,
                            offset_in_item: Pixels::ZERO,
                        })
                    })
                    .or_else(|| {
                        let entry_before_old_top =
                            old_entries.get(old_scroll_top.item_ix.saturating_sub(1))?;
                        let item_ix = self
                            .entries
                            .iter()
                            .position(|entry| entry == entry_before_old_top)?;
                        Some(ListOffset {
                            item_ix,
                            offset_in_item: Pixels::ZERO,
                        })
                    });

                self.list_state
                    .scroll_to(new_scroll_top.unwrap_or(old_scroll_top));
            }
        }
    }
}
