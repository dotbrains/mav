use super::*;

impl ThreadView {
    pub(super) fn render_tool_call_body(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        layout: ToolCallLayout,
        use_card_layout: bool,
        failed_or_canceled: bool,
        is_edit: bool,
        is_open: bool,
        should_show_raw_input: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !is_open {
            return None;
        }

        let input_output_header = |label: SharedString| {
            Label::new(label)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .buffer_font(cx)
        };

        Some(
            match &tool_call.status {
                ToolCallStatus::WaitingForConfirmation { options, .. } => {
                    let confirmation_content = v_flex()
                        .w_full()
                        .children(tool_call.content.iter().enumerate().map(
                            |(content_ix, content)| {
                                div()
                                    .child(self.render_tool_call_content(
                                        active_session_id,
                                        entry_ix,
                                        content,
                                        content_ix,
                                        tool_call,
                                        use_card_layout,
                                        failed_or_canceled,
                                        focus_handle,
                                        window,
                                        cx,
                                    ))
                                    .into_any_element()
                            },
                        ))
                        .when_some(
                            tool_call.sandbox_authorization_details.as_ref(),
                            |this, details| {
                                this.child(self.render_sandbox_authorization_details(
                                    entry_ix,
                                    &tool_call.id,
                                    details,
                                    cx,
                                ))
                            },
                        )
                        .when_some(
                            tool_call.sandbox_fallback_authorization_details.as_ref(),
                            |this, details| {
                                this.child(
                                    self.render_sandbox_fallback_authorization_details(details, cx),
                                )
                            },
                        )
                        .when(should_show_raw_input, |this| {
                            let is_raw_input_expanded =
                                self.expanded_tool_call_raw_inputs.contains(&tool_call.id);

                            let input_header = if is_raw_input_expanded {
                                "Raw Input:"
                            } else {
                                "View Raw Input"
                            };

                            this.child(
                                v_flex()
                                    .p_2()
                                    .gap_1()
                                    .border_t_1()
                                    .border_color(self.tool_card_border_color(cx))
                                    .child(
                                        h_flex()
                                            .id("disclosure_container")
                                            .pl_0p5()
                                            .gap_1()
                                            .justify_between()
                                            .rounded_xs()
                                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                                            .child(input_output_header(input_header.into()))
                                            .child(
                                                Disclosure::new(
                                                    ("raw-input-disclosure", entry_ix),
                                                    is_raw_input_expanded,
                                                )
                                                .opened_icon(IconName::ChevronUp)
                                                .closed_icon(IconName::ChevronDown),
                                            )
                                            .on_click(cx.listener({
                                                let id = tool_call.id.clone();

                                                move |this: &mut Self, _, _, cx| {
                                                    if this
                                                        .expanded_tool_call_raw_inputs
                                                        .contains(&id)
                                                    {
                                                        this.expanded_tool_call_raw_inputs
                                                            .remove(&id);
                                                    } else {
                                                        this.expanded_tool_call_raw_inputs
                                                            .insert(id.clone());
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .when(is_raw_input_expanded, |this| {
                                        this.children(tool_call.raw_input_markdown.clone().map(
                                            |input| {
                                                self.render_markdown(
                                                    input,
                                                    MarkdownStyle::themed(
                                                        MarkdownFont::Agent,
                                                        window,
                                                        cx,
                                                    ),
                                                    cx,
                                                )
                                            },
                                        ))
                                    }),
                            )
                        });

                    v_flex()
                        .w_full()
                        .map(|this| {
                            if layout == ToolCallLayout::Floating {
                                // Cap the content so the floating row cannot
                                // squeeze the conversation list to zero height.
                                this.child(
                                    div()
                                        .id(("floating-confirmation-content", entry_ix))
                                        .max_h_40()
                                        .overflow_y_scroll()
                                        .child(confirmation_content),
                                )
                            } else {
                                this.child(confirmation_content)
                            }
                        })
                        .child(self.render_permission_buttons(
                            self.thread.read(cx).session_id().clone(),
                            self.is_first_tool_call(active_session_id, &tool_call.id, cx),
                            options,
                            entry_ix,
                            tool_call.id.clone(),
                            focus_handle,
                            cx,
                        ))
                        .into_any()
                }
                ToolCallStatus::Pending | ToolCallStatus::InProgress
                    if is_edit
                        && tool_call.content.is_empty()
                        && self.as_native_connection(cx).is_some() =>
                {
                    self.render_diff_loading(cx)
                }
                ToolCallStatus::Pending
                | ToolCallStatus::InProgress
                | ToolCallStatus::Completed
                | ToolCallStatus::Failed
                | ToolCallStatus::Canceled => v_flex()
                    .when(should_show_raw_input, |this| {
                        this.mt_1p5().w_full().child(
                            v_flex()
                                .ml(rems(0.4))
                                .px_3p5()
                                .pb_1()
                                .gap_1()
                                .border_l_1()
                                .border_color(self.tool_card_border_color(cx))
                                .child(input_output_header("Raw Input:".into()))
                                .children(tool_call.raw_input_markdown.clone().map(|input| {
                                    div().id(("tool-call-raw-input-markdown", entry_ix)).child(
                                        self.render_markdown(
                                            input,
                                            MarkdownStyle::themed(MarkdownFont::Agent, window, cx),
                                            cx,
                                        ),
                                    )
                                }))
                                .child(input_output_header("Output:".into())),
                        )
                    })
                    .children(
                        tool_call
                            .content
                            .iter()
                            .enumerate()
                            .map(|(content_ix, content)| {
                                div().id(("tool-call-output", entry_ix)).child(
                                    self.render_tool_call_content(
                                        active_session_id,
                                        entry_ix,
                                        content,
                                        content_ix,
                                        tool_call,
                                        use_card_layout,
                                        failed_or_canceled,
                                        focus_handle,
                                        window,
                                        cx,
                                    ),
                                )
                            }),
                    )
                    .when(!use_card_layout, |this| {
                        let button_id =
                            SharedString::from(format!("tool_output-collapse-{:?}", tool_call.id));
                        let tool_call_id = tool_call.id.clone();

                        this.child(
                            div()
                                .ml(rems(0.4))
                                .px_3p5()
                                .pt_2()
                                .border_l_1()
                                .border_color(self.tool_card_border_color(cx))
                                .child(
                                    IconButton::new(button_id, IconName::ChevronUp)
                                        .full_width()
                                        .style(ButtonStyle::Outlined)
                                        .icon_color(Color::Muted)
                                        .on_click(cx.listener({
                                            move |this: &mut Self,
                                                  _,
                                                  window,
                                                  cx: &mut Context<Self>| {
                                                this.entry_view_state.update(cx, |state, _cx| {
                                                    state.collapse_tool_call(&tool_call_id);
                                                });
                                                this.refresh_thread_search(window, cx);
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    .into_any(),
                ToolCallStatus::Rejected => Empty.into_any(),
            }
            .into_any_element(),
        )
    }
}
