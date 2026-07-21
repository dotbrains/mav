use super::*;

impl ThreadView {
    fn collect_subagent_items_for_sessions(
        entries: &[AgentThreadEntry],
        awaiting_session_ids: &[acp::SessionId],
        cx: &App,
    ) -> Vec<(SharedString, usize)> {
        let tool_calls_by_session: HashMap<_, _> = entries
            .iter()
            .enumerate()
            .filter_map(|(entry_ix, entry)| {
                let AgentThreadEntry::ToolCall(tool_call) = entry else {
                    return None;
                };
                let info = tool_call.subagent_session_info.as_ref()?;
                let summary_text = tool_call.label.read(cx).source().to_string();
                let subagent_summary = if summary_text.is_empty() {
                    SharedString::from("Subagent")
                } else {
                    SharedString::from(summary_text)
                };
                Some((info.session_id.clone(), (subagent_summary, entry_ix)))
            })
            .collect();

        awaiting_session_ids
            .iter()
            .filter_map(|session_id| tool_calls_by_session.get(session_id).cloned())
            .collect()
    }

    pub(super) fn render_subagents_awaiting_permission(
        &self,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let awaiting = self.conversation.read(cx).subagents_awaiting_permission(cx);

        if awaiting.is_empty() {
            return None;
        }

        let awaiting_session_ids: Vec<_> = awaiting
            .iter()
            .map(|(session_id, _)| session_id.clone())
            .collect();

        let thread = self.thread.read(cx);
        let entries = thread.entries();
        let subagent_items =
            Self::collect_subagent_items_for_sessions(entries, &awaiting_session_ids, cx);

        if subagent_items.is_empty() {
            return None;
        }

        let item_count = subagent_items.len();

        Some(
            v_flex()
                .child(
                    h_flex()
                        .py_1()
                        .px_2()
                        .w_full()
                        .gap_1()
                        .border_b_1()
                        .border_color(cx.theme().colors().border)
                        .child(
                            Label::new("Subagents Awaiting Permission:")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(Label::new(item_count.to_string()).size(LabelSize::Small)),
                )
                .child(
                    v_flex().children(subagent_items.into_iter().enumerate().map(
                        |(ix, (label, entry_ix))| {
                            let is_last = ix == item_count - 1;
                            let group = format!("group-{}", entry_ix);

                            h_flex()
                                .cursor_pointer()
                                .id(format!("subagent-permission-{}", entry_ix))
                                .group(&group)
                                .p_1()
                                .pl_2()
                                .min_w_0()
                                .w_full()
                                .gap_1()
                                .justify_between()
                                .bg(cx.theme().colors().editor_background)
                                .hover(|s| s.bg(cx.theme().colors().element_hover))
                                .when(!is_last, |this| {
                                    this.border_b_1().border_color(cx.theme().colors().border)
                                })
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .child(
                                            Icon::new(IconName::Circle)
                                                .size(IconSize::XSmall)
                                                .color(Color::Warning),
                                        )
                                        .child(
                                            Label::new(label)
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .truncate(),
                                        ),
                                )
                                .child(
                                    div().visible_on_hover(&group).child(
                                        Label::new("Scroll to Subagent")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .truncate(),
                                    ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.list_state.scroll_to(ListOffset {
                                        item_ix: entry_ix,
                                        offset_in_item: px(0.0),
                                    });
                                    cx.notify();
                                }))
                        },
                    )),
                )
                .into_any(),
        )
    }

    pub(crate) fn render_main_agent_awaiting_permission(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if self.is_subagent() {
            return None;
        }

        let active_session_id = self.thread.read(cx).session_id().clone();
        let conversation = self.conversation.read(cx);
        let tool_call_id = conversation.pending_tool_call_for_session(&active_session_id, cx)?;
        let pending_count = conversation.pending_tool_call_count_for_session(&active_session_id);

        let thread = self.thread.read(cx);
        let (entry_ix, tool_call) = thread.tool_call(&tool_call_id)?;

        let scroll_icon = if self.list_state.item_is_above_viewport(entry_ix)? {
            IconName::ArrowUp
        } else if self.list_state.item_is_below_viewport(entry_ix)? {
            IconName::ArrowDown
        } else {
            return None;
        };

        let focus_handle = self.focus_handle(cx);

        let card = self.render_any_tool_call(
            &active_session_id,
            entry_ix,
            tool_call,
            &focus_handle,
            ToolCallLayout::Floating,
            window,
            cx,
        );

        let label: SharedString = if pending_count > 1 {
            format!("Awaiting Confirmation ({pending_count})").into()
        } else {
            "Awaiting Confirmation".into()
        };

        let header = h_flex()
            .p_1p5()
            .pl_2()
            .w_full()
            .gap_1p5()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .w_2()
                            .justify_center()
                            .child(GeneratingSpinnerElement::new(SpinnerVariant::Sand)),
                    )
                    .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
            )
            .child(
                Button::new("main-agent-permission-scroll-to", "Scroll")
                    .label_size(LabelSize::Small)
                    .end_icon(
                        Icon::new(scroll_icon)
                            .size(IconSize::XSmall)
                            .color(Color::Default),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.list_state.scroll_to(ListOffset {
                            item_ix: entry_ix,
                            offset_in_item: px(0.0),
                        });
                        cx.notify();
                    })),
            );

        Some(v_flex().child(header).child(card).into_any())
    }
}
