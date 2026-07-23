use super::*;

impl CompletionsMenu {
    pub(super) fn select_first(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let entries = self.entries.borrow();
        if entries.is_empty() {
            return;
        }
        let start = if self.scroll_handle.y_flipped() {
            entries.len() - 1
        } else {
            0
        };
        drop(entries);
        let index = self.find_selectable_entry(start, !self.scroll_handle.y_flipped());
        if let Some(index) = index {
            self.update_selection_index(index, provider, window, cx);
        }
    }

    pub(super) fn select_last(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let entries = self.entries.borrow();
        if entries.is_empty() {
            return;
        }
        let start = if self.scroll_handle.y_flipped() {
            0
        } else {
            entries.len() - 1
        };
        drop(entries);
        let index = self.find_selectable_entry(start, self.scroll_handle.y_flipped());
        if let Some(index) = index {
            self.update_selection_index(index, provider, window, cx);
        }
    }

    pub(super) fn select_prev(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let index = if self.scroll_handle.y_flipped() {
            self.next_match_index()
        } else {
            self.prev_match_index()
        };
        self.update_selection_index(index, provider, window, cx);
    }

    pub(super) fn select_next(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let index = if self.scroll_handle.y_flipped() {
            self.prev_match_index()
        } else {
            self.next_match_index()
        };
        self.update_selection_index(index, provider, window, cx);
    }

    pub(crate) fn update_selection_index(
        &mut self,
        match_index: usize,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if self.selected_item != match_index {
            self.selected_item = match_index;
            self.handle_selection_changed(provider, window, cx);
        }
    }

    pub(crate) fn prev_match_index(&self) -> usize {
        let entries = self.entries.borrow();
        let len = entries.len();
        if len == 0 {
            return 0;
        }
        let mut index = if self.selected_item > 0 {
            self.selected_item - 1
        } else {
            len - 1
        };
        let start = index;
        loop {
            if entries[index].is_selectable() {
                return index;
            }
            index = if index > 0 { index - 1 } else { len - 1 };
            if index == start {
                return self.selected_item;
            }
        }
    }

    pub(crate) fn next_match_index(&self) -> usize {
        let entries = self.entries.borrow();
        let len = entries.len();
        if len == 0 {
            return 0;
        }
        let mut index = if self.selected_item + 1 < len {
            self.selected_item + 1
        } else {
            0
        };
        let start = index;
        loop {
            if entries[index].is_selectable() {
                return index;
            }
            index = if index + 1 < len { index + 1 } else { 0 };
            if index == start {
                return self.selected_item;
            }
        }
    }

    pub(crate) fn find_selectable_entry(&self, start: usize, forward: bool) -> Option<usize> {
        let entries = self.entries.borrow();
        let len = entries.len();
        if len == 0 {
            return None;
        }
        let mut index = start;
        loop {
            if entries[index].is_selectable() {
                return Some(index);
            }
            if forward {
                index = if index + 1 < len { index + 1 } else { 0 };
            } else {
                index = if index > 0 { index - 1 } else { len - 1 };
            }
            if index == start {
                return None;
            }
        }
    }

    pub(super) fn handle_selection_changed(
        &mut self,
        provider: Option<&dyn CompletionProvider>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Nearest);
        if let Some(provider) = provider {
            let entries = self.entries.borrow();
            let entry = if self.selected_item < entries.len() {
                entries[self.selected_item].as_match()
            } else {
                None
            };
            provider.selection_changed(entry, window, cx);
        }
        self.resolve_visible_completions(provider, cx);
        self.start_markdown_parse_for_nearby_entries(cx);
        cx.notify();
    }
}
