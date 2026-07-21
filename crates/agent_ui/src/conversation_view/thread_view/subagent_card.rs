use super::*;

impl ThreadView {
    pub(super) fn render_subagent_card(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        thread_view: Option<&Entity<ThreadView>>,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let thread = thread_view
            .as_ref()
            .map(|view| view.read(cx).thread.clone());
        let subagent_session_id = thread
            .as_ref()
            .map(|thread| thread.read(cx).session_id().clone());
        let action_log = thread.as_ref().map(|thread| thread.read(cx).action_log());
        let changed_buffers = action_log
            .map(|log| log.read(cx).changed_buffers(cx).collect::<Vec<_>>())
            .unwrap_or_default();

        let is_pending_tool_call = thread_view
            .as_ref()
            .and_then(|tv| {
                let sid = tv.read(cx).thread.read(cx).session_id();
                self.conversation.read(cx).pending_tool_call(sid, cx)
            })
            .is_some();

        let is_expanded = self
            .entry_view_state
            .read(cx)
            .is_tool_call_expanded(&tool_call.id);
        let files_changed = changed_buffers.len();
        let diff_stats = DiffStats::all_files(changed_buffers, cx);

        let is_running = matches!(
            tool_call.status,
            ToolCallStatus::Pending
                | ToolCallStatus::InProgress
                | ToolCallStatus::WaitingForConfirmation { .. }
        );

        let is_failed = matches!(
            tool_call.status,
            ToolCallStatus::Failed | ToolCallStatus::Rejected
        );

        let is_cancelled = matches!(tool_call.status, ToolCallStatus::Canceled)
            || tool_call.content.iter().any(|c| match c {
                ToolCallContent::ContentBlock(block) => {
                    block.text_content(cx) == Some("User canceled")
                }
                _ => false,
            });

        let thread_title = thread
            .as_ref()
            .and_then(|t| t.read(cx).title())
            .filter(|t| !t.is_empty());
        let tool_call_label = tool_call.label.read(cx).source().to_string();
        let has_tool_call_label = !tool_call_label.is_empty();

        let has_title = thread_title.is_some() || has_tool_call_label;
        let has_no_title_or_canceled = !has_title || is_failed || is_cancelled;

        let title: SharedString = if let Some(thread_title) = thread_title {
            thread_title
        } else if !tool_call_label.is_empty() {
            tool_call_label.into()
        } else if is_cancelled {
            "Subagent Canceled".into()
        } else if is_failed {
            "Subagent Failed".into()
        } else {
            "Spawning Agent…".into()
        };

        let card_header_id = format!("subagent-header-{}", entry_ix);
        let status_icon = format!("status-icon-{}", entry_ix);
        let diff_stat_id = format!("subagent-diff-{}", entry_ix);

        let icon = h_flex().w_4().justify_center().child(if is_running {
            SpinnerLabel::new()
                .size(LabelSize::Small)
                .into_any_element()
        } else if is_cancelled {
            div()
                .id(status_icon)
                .child(
                    Icon::new(IconName::Circle)
                        .size(IconSize::Small)
                        .color(Color::Custom(
                            cx.theme().colors().icon_disabled.opacity(0.5),
                        )),
                )
                .tooltip(Tooltip::text("Subagent Cancelled"))
                .into_any_element()
        } else if is_failed {
            div()
                .id(status_icon)
                .child(
                    Icon::new(IconName::Close)
                        .size(IconSize::Small)
                        .color(Color::Error),
                )
                .tooltip(Tooltip::text("Subagent Failed"))
                .into_any_element()
        } else {
            Icon::new(IconName::Check)
                .size(IconSize::Small)
                .color(Color::Success)
                .into_any_element()
        });

        let has_expandable_content = thread
            .as_ref()
            .map_or(false, |thread| !thread.read(cx).entries().is_empty());

        let tooltip_meta_description = if is_expanded {
            "Click to Collapse"
        } else {
            "Click to Preview"
        };

        let error_message = self.subagent_error_message(&tool_call.status, tool_call, cx);

        v_flex()
            .w_full()
            .rounded_md()
            .border_1()
            .when(has_no_title_or_canceled, |this| this.border_dashed())
            .border_color(self.tool_card_border_color(cx))
            .overflow_hidden()
            .child(
                h_flex()
                    .group(&card_header_id)
                    .h_8()
                    .p_1()
                    .w_full()
                    .justify_between()
                    .when(!has_no_title_or_canceled, |this| {
                        this.bg(self.tool_card_header_bg(cx))
                    })
                    .child(
                        h_flex()
                            .id(format!("subagent-title-{}", entry_ix))
                            .px_1()
                            .min_w_0()
                            .size_full()
                            .gap_2()
                            .justify_between()
                            .rounded_sm()
                            .overflow_hidden()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .w_full()
                                    .gap_1p5()
                                    .child(icon)
                                    .child(
                                        Label::new(title.to_string())
                                            .size(LabelSize::Custom(self.tool_name_font_size()))
                                            .truncate(),
                                    )
                                    .when(files_changed > 0, |this| {
                                        this.child(
                                            Label::new(format!(
                                                "— {} {} changed",
                                                files_changed,
                                                if files_changed == 1 { "file" } else { "files" }
                                            ))
                                            .size(LabelSize::Custom(self.tool_name_font_size()))
                                            .color(Color::Muted),
                                        )
                                        .child(
                                            DiffStat::new(
                                                diff_stat_id.clone(),
                                                diff_stats.lines_added as usize,
                                                diff_stats.lines_removed as usize,
                                            )
                                            .label_size(LabelSize::Custom(
                                                self.tool_name_font_size(),
                                            )),
                                        )
                                    }),
                            )
                            .when(!has_no_title_or_canceled && !is_pending_tool_call, |this| {
                                this.tooltip(move |_, cx| {
                                    Tooltip::with_meta(
                                        title.to_string(),
                                        None,
                                        tooltip_meta_description,
                                        cx,
                                    )
                                })
                            })
                            .when(has_expandable_content && !is_pending_tool_call, |this| {
                                this.cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().colors().element_hover))
                                    .child(
                                        div().visible_on_hover(card_header_id).child(
                                            Icon::new(if is_expanded {
                                                IconName::ChevronUp
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .color(Color::Muted)
                                            .size(IconSize::Small),
                                        ),
                                    )
                                    .on_click(cx.listener({
                                        let tool_call_id = tool_call.id.clone();
                                        move |this, _, window, cx| {
                                            let expanded =
                                                this.entry_view_state.update(cx, |state, _cx| {
                                                    state.toggle_tool_call_expansion(&tool_call_id);
                                                    state.is_tool_call_expanded(&tool_call_id)
                                                });
                                            this.refresh_thread_search(window, cx);
                                            telemetry::event!("Subagent Toggled", expanded);
                                            cx.notify();
                                        }
                                    }))
                            }),
                    )
                    .when(is_running && subagent_session_id.is_some(), |buttons| {
                        buttons.child(
                            IconButton::new(format!("stop-subagent-{}", entry_ix), IconName::Stop)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Error)
                                .tooltip(Tooltip::text("Stop Subagent"))
                                .when_some(
                                    thread_view
                                        .as_ref()
                                        .map(|view| view.read(cx).thread.clone()),
                                    |this, thread| {
                                        this.on_click(cx.listener(
                                            move |_this, _event, _window, cx| {
                                                telemetry::event!("Subagent Stopped");
                                                thread.update(cx, |thread, cx| {
                                                    thread.cancel(cx).detach();
                                                });
                                            },
                                        ))
                                    },
                                ),
                        )
                    }),
            )
            .when_some(thread_view, |this, thread_view| {
                let thread = &thread_view.read(cx).thread;
                let tv_session_id = thread.read(cx).session_id();
                let pending_tool_call = self
                    .conversation
                    .read(cx)
                    .pending_tool_call(tv_session_id, cx);

                let nav_session_id = tv_session_id.clone();

                let fullscreen_toggle = h_flex()
                    .id(entry_ix)
                    .py_1()
                    .w_full()
                    .justify_center()
                    .border_t_1()
                    .when(is_failed, |this| this.border_dashed())
                    .border_color(self.tool_card_border_color(cx))
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().colors().element_hover))
                    .child(
                        Icon::new(IconName::Maximize)
                            .color(Color::Muted)
                            .size(IconSize::Small),
                    )
                    .tooltip(Tooltip::text("Make Subagent Full Screen"))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        telemetry::event!("Subagent Maximized");
                        this.server_view
                            .update(cx, |this, cx| {
                                this.navigate_to_thread(nav_session_id.clone(), window, cx);
                            })
                            .ok();
                    }));

                if is_running && let Some((_, subagent_tool_call_id, _)) = pending_tool_call {
                    if let Some((entry_ix, tool_call)) =
                        thread.read(cx).tool_call(&subagent_tool_call_id)
                    {
                        this.child(Divider::horizontal().color(DividerColor::Border))
                            .child(thread_view.read(cx).render_any_tool_call(
                                active_session_id,
                                entry_ix,
                                tool_call,
                                focus_handle,
                                ToolCallLayout::Embedded,
                                window,
                                cx,
                            ))
                            .child(fullscreen_toggle)
                    } else {
                        this
                    }
                } else {
                    this.when(is_expanded, |this| {
                        this.child(self.render_subagent_expanded_content(
                            thread_view,
                            tool_call,
                            window,
                            cx,
                        ))
                        .when_some(error_message, |this, message| {
                            this.child(
                                Callout::new()
                                    .severity(Severity::Error)
                                    .icon(IconName::XCircle)
                                    .title(message),
                            )
                        })
                        .child(fullscreen_toggle)
                    })
                }
            })
            .into_any_element()
    }
}
