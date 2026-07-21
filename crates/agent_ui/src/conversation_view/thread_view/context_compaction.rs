use super::*;

impl ThreadView {
    pub(super) fn render_context_compaction(
        &self,
        entry_ix: usize,
        compaction: &acp_thread::ContextCompaction,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_compacting = compaction.is_in_progress();
        let summary = compaction.summary.clone();
        let is_expanded = self
            .entry_view_state
            .read(cx)
            .is_compaction_expanded(entry_ix);

        let id = format!("context-compaction-{entry_ix}");
        let header_label = match compaction.status {
            acp_thread::ContextCompactionStatus::InProgress => "Compacting Context…",
            acp_thread::ContextCompactionStatus::Completed => "Context Compacted",
            acp_thread::ContextCompactionStatus::Canceled => "Compaction Canceled",
        };
        let chevron_end = if is_expanded {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        };
        let header = h_flex()
            .gap_1()
            .w_full()
            .child(Divider::horizontal())
            .child(
                Button::new(id, header_label)
                    .label_size(LabelSize::Small)
                    .loading(is_compacting)
                    .disabled(is_compacting)
                    .start_icon(
                        Icon::new(IconName::Compact)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .when(!is_compacting, |this| {
                        this.end_icon(
                            Icon::new(chevron_end)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .on_click(cx.listener(
                            move |this, _event: &ClickEvent, window, cx| {
                                this.toggle_compaction_expansion(entry_ix, window, cx);
                            },
                        ))
                    }),
            )
            .child(Divider::horizontal());

        div()
            .px_5()
            .w_full()
            .child(
                v_flex()
                    .pt_1p5()
                    .mb_1p5()
                    .gap_1p5()
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .rounded_sm()
                    .child(header)
                    .when_some(summary.filter(|_| is_expanded), |this, summary| {
                        this.border_color(self.tool_card_border_color(cx))
                            .bg(cx.theme().colors().editor_background.opacity(0.2))
                            .child(
                                div()
                                    .id(("compaction-summary", entry_ix))
                                    .p_2()
                                    .text_ui(cx)
                                    .child(self.render_markdown(
                                        summary,
                                        MarkdownStyle::themed(MarkdownFont::Agent, window, cx),
                                        cx,
                                    )),
                            )
                            .child(
                                h_flex()
                                    .border_t_1()
                                    .border_color(self.tool_card_border_color(cx))
                                    .child(
                                        IconButton::new(
                                            ("compaction-summary-collapse", entry_ix),
                                            IconName::ChevronUp,
                                        )
                                        .full_width()
                                        .on_click(
                                            cx.listener(
                                                move |this, _event: &ClickEvent, window, cx| {
                                                    this.entry_view_state.update(
                                                        cx,
                                                        |state, _cx| {
                                                            state.collapse_compaction(entry_ix);
                                                        },
                                                    );
                                                    this.refresh_thread_search(window, cx);
                                                    cx.notify();
                                                },
                                            ),
                                        ),
                                    ),
                            )
                    }),
            )
            .into_any()
    }

    fn toggle_compaction_expansion(
        &mut self,
        entry_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.entry_view_state.update(cx, |state, _cx| {
            state.toggle_compaction_expansion(entry_ix);
        });
        self.refresh_thread_search(window, cx);
        cx.notify();
    }
}
