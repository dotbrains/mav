use super::*;

impl ThreadView {
    pub(super) fn toggle_thinking_block_expansion(
        &mut self,
        key: (usize, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.entry_view_state.update(cx, |state, cx| {
            state.toggle_thinking_block_expansion(key, cx);
        });
        self.refresh_thread_search(window, cx);
        cx.notify();
    }

    pub(super) fn render_thinking_block(
        &self,
        entry_ix: usize,
        chunk_ix: usize,
        chunk: Entity<Markdown>,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let header_id = SharedString::from(format!("thinking-block-header-{}", entry_ix));
        let card_header_id = SharedString::from("inner-card-header");

        let key = (entry_ix, chunk_ix);

        let entry_view_state = self.entry_view_state.read(cx);
        let (is_open, is_constrained) = entry_view_state.thinking_block_state(key, cx);
        let should_auto_scroll = entry_view_state.is_auto_expanded_thinking_block(key);
        let scroll_handle = entry_view_state
            .entry(entry_ix)
            .and_then(|entry| entry.scroll_handle_for_assistant_message_chunk(chunk_ix));

        if should_auto_scroll {
            if let Some(ref handle) = scroll_handle {
                handle.scroll_to_bottom();
            }
        }

        let panel_bg = cx.theme().colors().panel_background;

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .id(header_id)
                    .group(&card_header_id)
                    .relative()
                    .w_full()
                    .pr_1()
                    .justify_between()
                    .child(
                        h_flex()
                            .h(window.line_height() - px(2.))
                            .gap_1p5()
                            .overflow_hidden()
                            .child(
                                Icon::new(IconName::ToolThink)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                div()
                                    .text_size(self.tool_name_font_size())
                                    .text_color(cx.theme().colors().text_muted)
                                    .child("Thinking"),
                            ),
                    )
                    .child(
                        Disclosure::new(("expand", entry_ix), is_open)
                            .opened_icon(IconName::ChevronUp)
                            .closed_icon(IconName::ChevronDown)
                            .visible_on_hover(&card_header_id)
                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                this.toggle_thinking_block_expansion(key, window, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                        this.toggle_thinking_block_expansion(key, window, cx);
                    })),
            )
            .when(is_open, |this| {
                this.child(
                    div()
                        .when(is_constrained, |this| this.relative())
                        .child(
                            div()
                                .id(("thinking-content", chunk_ix))
                                .ml_1p5()
                                .pl_3p5()
                                .border_l_1()
                                .border_color(self.tool_card_border_color(cx))
                                .when(is_constrained, |this| this.max_h_64())
                                .when_some(scroll_handle, |this, scroll_handle| {
                                    this.track_scroll(&scroll_handle)
                                })
                                .overflow_hidden()
                                .child(self.render_markdown(
                                    chunk,
                                    MarkdownStyle::themed(MarkdownFont::Agent, window, cx),
                                    cx,
                                )),
                        )
                        .when(is_constrained, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .size_full()
                                    .bg(linear_gradient(
                                        180.,
                                        linear_color_stop(panel_bg.opacity(0.8), 0.),
                                        linear_color_stop(panel_bg.opacity(0.), 0.1),
                                    ))
                                    .block_mouse_except_scroll(),
                            )
                        }),
                )
            })
            .into_any_element()
    }
}
