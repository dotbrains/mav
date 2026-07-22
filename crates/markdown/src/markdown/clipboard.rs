use super::*;

impl Markdown {
    pub(super) fn copy(&self, text: &RenderedText, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.end <= self.selection.start {
            return;
        }
        let text = text.text_for_range(self.selection.start..self.selection.end);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn copy_as_markdown(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.context_menu_selected_markdown.take() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
            return;
        }
        if self.selection.end <= self.selection.start {
            return;
        }
        let text = self.source[self.selection.start..self.selection.end].to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn capture_for_context_menu(
        &mut self,
        link: Option<SharedString>,
        rendered_text: Option<&RenderedText>,
    ) {
        let range = self.selection.start..self.selection.end;
        if range.end > range.start {
            self.context_menu_selected_markdown =
                Some(SharedString::new(&self.source[range.clone()]));
            self.context_menu_selected_text = rendered_text
                .map(|text| text.text_for_range(range))
                .map(SharedString::new)
                .or_else(|| self.context_menu_selected_markdown.clone());
        } else {
            self.context_menu_selected_markdown = None;
            self.context_menu_selected_text = None;
        }
        self.context_menu_link = link;
    }
}
