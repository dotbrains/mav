use super::*;

impl ThreadView {
    pub(super) fn render_tool_call_header(
        &self,
        entry_ix: usize,
        tool_call: &ToolCall,
        card_header_id: SharedString,
        is_terminal_tool: bool,
        is_edit: bool,
        is_cancelled_edit: bool,
        has_revealed_diff: bool,
        use_card_layout: bool,
        is_collapsible: bool,
        failed_or_canceled: bool,
        is_open: bool,
        tool_call_output_focus: bool,
        tool_call_output_focus_handle: FocusHandle,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        if is_terminal_tool {
            return self
                .render_collapsible_command(
                    card_header_id.clone(),
                    true,
                    tool_call.label.clone(),
                    window,
                    cx,
                )
                .into_any_element();
        }

        h_flex()
            .group(&card_header_id)
            .relative()
            .w_full()
            .justify_between()
            .when(use_card_layout, |this| {
                this.p_0p5()
                    .rounded_t(rems_from_px(5.))
                    .bg(self.tool_card_header_bg(cx))
            })
            .child(self.render_tool_call_label(
                entry_ix,
                tool_call,
                is_edit,
                is_cancelled_edit,
                has_revealed_diff,
                use_card_layout,
                window,
                cx,
            ))
            .child(
                h_flex()
                    .when(is_collapsible || failed_or_canceled, |this| {
                        this.child(self.render_tool_call_status_controls(
                            entry_ix,
                            tool_call,
                            card_header_id.clone(),
                            is_collapsible,
                            failed_or_canceled,
                            is_cancelled_edit,
                            has_revealed_diff,
                            is_open,
                            cx,
                        ))
                    })
                    .when(tool_call_output_focus, |this| {
                        this.child(
                            Button::new("open-file-button", "Open File")
                                .style(ButtonStyle::Outlined)
                                .label_size(LabelSize::Small)
                                .key_binding(
                                    KeyBinding::for_action_in(
                                        &OpenExcerpts,
                                        &tool_call_output_focus_handle,
                                        cx,
                                    )
                                    .map(|s| s.size(rems_from_px(12.))),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenExcerpts), cx)
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_tool_call_status_controls(
        &self,
        entry_ix: usize,
        tool_call: &ToolCall,
        card_header_id: SharedString,
        is_collapsible: bool,
        failed_or_canceled: bool,
        is_cancelled_edit: bool,
        has_revealed_diff: bool,
        is_open: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let diff_for_discard = if has_revealed_diff && is_cancelled_edit {
            tool_call.diffs().next().cloned()
        } else {
            None
        };

        h_flex()
            .pr_0p5()
            .gap_1()
            .when(is_collapsible, |this| {
                this.child(
                    Disclosure::new(("expand-output", entry_ix), is_open)
                        .opened_icon(IconName::ChevronUp)
                        .closed_icon(IconName::ChevronDown)
                        .visible_on_hover(&card_header_id)
                        .on_click(cx.listener({
                            let id = tool_call.id.clone();
                            move |this: &mut Self, _, window, cx: &mut Context<Self>| {
                                this.entry_view_state.update(cx, |state, _cx| {
                                    state.toggle_tool_call_expansion(&id);
                                });
                                this.refresh_thread_search(window, cx);
                                cx.notify();
                            }
                        })),
                )
            })
            .when(failed_or_canceled, |this| {
                if is_cancelled_edit && !has_revealed_diff {
                    this.child(
                        div()
                            .id(entry_ix)
                            .tooltip(Tooltip::text("Interrupted Edit"))
                            .child(
                                Icon::new(IconName::XCircle)
                                    .color(Color::Muted)
                                    .size(IconSize::Small),
                            ),
                    )
                } else if is_cancelled_edit {
                    this
                } else {
                    this.child(
                        Icon::new(IconName::Close)
                            .color(Color::Error)
                            .size(IconSize::Small),
                    )
                }
            })
            .when_some(diff_for_discard, |this, diff| {
                let tool_call_id = tool_call.id.clone();
                let is_discarded = self.discarded_partial_edits.contains(&tool_call_id);

                this.when(!is_discarded, |this| {
                    this.child(
                        IconButton::new(("discard-partial-edit", entry_ix), IconName::Undo)
                            .icon_size(IconSize::Small)
                            .tooltip(move |_, cx| {
                                Tooltip::with_meta(
                                    "Discard Interrupted Edit",
                                    None,
                                    "You can discard this interrupted partial edit and restore the original file content.",
                                    cx,
                                )
                            })
                            .on_click(cx.listener({
                                let tool_call_id = tool_call_id.clone();
                                move |this, _, _window, cx| {
                                    let diff_data = diff.read(cx);
                                    let base_text = diff_data.base_text().clone();
                                    let buffer = diff_data.buffer().clone();
                                    buffer.update(cx, |buffer, cx| {
                                        buffer.set_text(base_text.as_ref(), cx);
                                    });
                                    this.discarded_partial_edits.insert(tool_call_id.clone());
                                    cx.notify();
                                }
                            })),
                    )
                })
            })
            .into_any_element()
    }
}
