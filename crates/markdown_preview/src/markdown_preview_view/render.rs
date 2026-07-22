use super::*;

impl Render for MarkdownPreviewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview_theme = self.resolve_preview_theme(cx);
        let bg_color = preview_theme
            .as_ref()
            .map(|theme| theme.colors().editor_background)
            .unwrap_or_else(|| cx.theme().colors().editor_background);
        let preview_font_size = ThemeSettings::get_global(cx).markdown_preview_font_size(cx);
        div()
            .image_cache(self.image_cache.clone())
            .id("MarkdownPreview")
            .key_context("MarkdownPreview")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(MarkdownPreviewView::scroll_page_up))
            .on_action(cx.listener(MarkdownPreviewView::scroll_page_down))
            .on_action(cx.listener(MarkdownPreviewView::scroll_up))
            .on_action(cx.listener(MarkdownPreviewView::scroll_down))
            .on_action(cx.listener(MarkdownPreviewView::scroll_up_by_item))
            .on_action(cx.listener(MarkdownPreviewView::scroll_down_by_item))
            .on_action(cx.listener(MarkdownPreviewView::scroll_to_top))
            .on_action(cx.listener(MarkdownPreviewView::scroll_to_bottom))
            .on_action(cx.listener(MarkdownPreviewView::increase_font_size))
            .on_action(cx.listener(MarkdownPreviewView::decrease_font_size))
            .on_action(cx.listener(MarkdownPreviewView::reset_font_size))
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(bg_color)
            .child(
                WithRemSize::new(preview_font_size).size_full().child(
                    div()
                        .id("markdown-preview-scroll-container")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle)
                        .p_4()
                        .child({
                            let markdown_element =
                                self.render_markdown_element(&preview_theme, window, cx);
                            let markdown = self.markdown.clone();
                            let max_width = MarkdownPreviewSettings::get_global(cx).max_width;
                            let content = right_click_menu("markdown-preview-context-menu")
                                .trigger(move |_, _, _| markdown_element)
                                .maybe_menu(move |window, cx| {
                                    let focus = window.focused(cx);
                                    let markdown = markdown.read(cx);
                                    let context_menu_link = markdown.context_menu_link().cloned();
                                    let selected_text =
                                        markdown.context_menu_selected_text().cloned();
                                    let selected_markdown =
                                        markdown.context_menu_selected_markdown().cloned();
                                    if context_menu_link.is_none()
                                        && selected_text.is_none()
                                        && selected_markdown.is_none()
                                    {
                                        return None;
                                    }
                                    Some(ContextMenu::build(window, cx, move |menu, _, _cx| {
                                        menu.when_some(focus, |menu, focus| menu.context(focus))
                                            .when_some(selected_text, |menu, text| {
                                                menu.entry(
                                                    "Copy",
                                                    Some(Box::new(markdown::Copy)),
                                                    move |_, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                text.to_string(),
                                                            ),
                                                        );
                                                    },
                                                )
                                            })
                                            .when_some(selected_markdown, |menu, text| {
                                                menu.entry(
                                                    "Copy as Markdown",
                                                    Some(Box::new(markdown::CopyAsMarkdown)),
                                                    move |_, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                text.to_string(),
                                                            ),
                                                        );
                                                    },
                                                )
                                            })
                                            .when_some(context_menu_link, |menu, url| {
                                                menu.entry("Copy Link", None, move |_, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(url.to_string()),
                                                    );
                                                })
                                            })
                                    }))
                                });
                            div()
                                .w_full()
                                .when_some(max_width, |this, max_width| {
                                    this.max_w(max_width).mx_auto()
                                })
                                .child(content)
                        }),
                ),
            )
            .vertical_scrollbar_for(&self.scroll_handle, window, cx)
    }
}
