use super::*;

impl ThreadView {
    pub(super) fn is_blocked_on_terminal_command(&self, cx: &App) -> bool {
        let thread = self.thread.read(cx);
        if !matches!(thread.status(), ThreadStatus::Generating) {
            return false;
        }

        let mut has_running_terminal_call = false;

        for entry in thread.entries().iter().rev() {
            match entry {
                AgentThreadEntry::UserMessage(_) => break,
                AgentThreadEntry::ToolCall(tool_call)
                    if matches!(
                        tool_call.status,
                        ToolCallStatus::InProgress | ToolCallStatus::Pending
                    ) =>
                {
                    if matches!(tool_call.kind, acp::ToolKind::Execute) {
                        has_running_terminal_call = true;
                    } else {
                        return false;
                    }
                }
                AgentThreadEntry::ToolCall(_)
                | AgentThreadEntry::AssistantMessage(_)
                | AgentThreadEntry::CompletedPlan(_)
                | AgentThreadEntry::ContextCompaction(_) => {}
            }
        }

        has_running_terminal_call
    }

    pub(super) fn render_collapsible_command(
        &self,
        group: SharedString,
        is_preview: bool,
        command: Entity<Markdown>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        // The label's markdown source is a fenced code block (```\n...\n```);
        // strip the fences so the copy button yields just the command text.
        let command_source = command.read(cx).source();
        let command_text = command_source
            .strip_prefix("```\n")
            .and_then(|s| s.strip_suffix("\n```"))
            .unwrap_or(&command_source)
            .to_string();

        let mut style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx).with_buffer_font(cx);
        style.container_style.text.font_size = Some(rems_from_px(12.).into());
        style.container_style.text.line_height = Some(rems_from_px(17.).into());
        style.height_is_multiple_of_line_height = true;
        // Soft-wrap the command instead of horizontally scrolling it: the card is
        // narrow, and in scroll mode a long command wraps anyway but its wrapped
        // lines don't pick up the code block's left padding. Wrap mode lays the
        // text out as a normal block inside the padded content box, so every
        // line (wrapped or not) is padded consistently.
        style.code_block_overflow_x_scroll = false;

        let header_bg = self.tool_card_header_bg(cx);
        let run_command_label = if is_preview {
            Some(
                h_flex().h_6().child(
                    Label::new("Run Command")
                        .buffer_font(cx)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
            )
        } else {
            None
        };
        // Suppress the code block's built-in copy button so we don't stack two
        // copy buttons on top of each other; the outer button below is the one
        // we want, because it copies the unfenced command text.
        let markdown_element = self
            .render_markdown(command, style, cx)
            .code_block_renderer(CodeBlockRenderer::Default {
                copy_button_visibility: CopyButtonVisibility::Hidden,
                wrap_button_visibility: markdown::WrapButtonVisibility::Hidden,
                border: false,
            });
        let copy_button_id = SharedString::from(format!("{group}-copy-command"));
        let copy_button = CopyButton::new(copy_button_id, command_text)
            .tooltip_label("Copy Command")
            .visible_on_hover(group.clone());

        v_flex()
            .group(group)
            .relative()
            .p_1p5()
            .bg(header_bg)
            .when(is_preview, |this| this.pt_1().children(run_command_label))
            .child(markdown_element)
            .child(div().absolute().top_1().right_1().child(copy_button))
    }

    pub(super) fn render_terminal_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        terminal: &Entity<acp_thread::Terminal>,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        layout: ToolCallLayout,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let terminal_data = terminal.read(cx);
        let working_dir = terminal_data.working_dir();
        let started_at = terminal_data.started_at();

        let tool_failed = matches!(
            &tool_call.status,
            ToolCallStatus::Rejected | ToolCallStatus::Canceled | ToolCallStatus::Failed
        );

        let confirmation_options = match &tool_call.status {
            ToolCallStatus::WaitingForConfirmation { options, .. } => Some(options),
            _ => None,
        };
        let needs_confirmation = confirmation_options.is_some();

        let output = terminal_data.output();
        let command_finished = output.is_some()
            && !matches!(
                tool_call.status,
                ToolCallStatus::InProgress | ToolCallStatus::Pending
            );
        let truncated_output =
            output.is_some_and(|output| output.original_content_len > output.content.len());
        let output_line_count = output.map(|output| output.content_line_count).unwrap_or(0);

        let command_failed = command_finished
            && output.is_some_and(|o| o.exit_status.is_some_and(|status| !status.success()));

        let time_elapsed = if let Some(output) = output {
            output.ended_at.duration_since(started_at)
        } else {
            started_at.elapsed()
        };

        let header_id =
            SharedString::from(format!("terminal-tool-header-{}", terminal.entity_id()));
        let header_group = SharedString::from(format!(
            "terminal-tool-header-group-{}",
            terminal.entity_id()
        ));
        let header_bg = cx
            .theme()
            .colors()
            .element_background
            .blend(cx.theme().colors().editor_foreground.opacity(0.025));
        let border_color = cx.theme().colors().border.opacity(0.6);

        let working_dir = working_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "current directory".to_string());

        let command_element = self.render_collapsible_command(
            header_group.clone(),
            false,
            tool_call.label.clone(),
            window,
            cx,
        );

        let is_expanded = self
            .entry_view_state
            .read(cx)
            .is_tool_call_expanded(&tool_call.id);

        let header = h_flex()
            .id(header_id)
            .pt_1()
            .pl_1p5()
            .pr_1()
            .flex_none()
            .gap_1()
            .justify_between()
            .rounded_t_md()
            .child(
                div()
                    .id(("command-target-path", terminal.entity_id()))
                    .w_full()
                    .max_w_full()
                    .overflow_x_scroll()
                    .child(
                        Label::new(working_dir)
                            .buffer_font(cx)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .child(
                Disclosure::new(
                    SharedString::from(format!(
                        "terminal-tool-disclosure-{}",
                        terminal.entity_id()
                    )),
                    is_expanded,
                )
                .opened_icon(IconName::ChevronUp)
                .closed_icon(IconName::ChevronDown)
                .visible_on_hover(&header_group)
                .on_click(cx.listener({
                    let id = tool_call.id.clone();
                    move |this, _event, window, cx| {
                        this.entry_view_state.update(cx, |state, _cx| {
                            state.toggle_tool_call_expansion(&id);
                        });
                        this.refresh_thread_search(window, cx);
                        cx.notify();
                    }
                })),
            )
            .when(time_elapsed > Duration::from_secs(10), |header| {
                header.child(
                    Label::new(format!("({})", duration_alt_display(time_elapsed)))
                        .buffer_font(cx)
                        .color(Color::Muted)
                        .size(LabelSize::XSmall),
                )
            })
            .when(!command_finished && !needs_confirmation, |header| {
                header
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::ArrowCircle)
                            .size(IconSize::XSmall)
                            .color(Color::Muted)
                            .with_rotate_animation(2)
                    )
                    .child(div().h(relative(0.6)).ml_1p5().child(Divider::vertical().color(DividerColor::Border)))
                    .child(
                        IconButton::new(
                            SharedString::from(format!("stop-terminal-{}", terminal.entity_id())),
                            IconName::Stop
                        )
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Error)
                        .tooltip(move |_window, cx| {
                            Tooltip::with_meta(
                                "Stop This Command",
                                None,
                                "Also possible by placing your cursor inside the terminal and using regular terminal bindings.",
                                cx,
                            )
                        })
                        .on_click({
                            let terminal = terminal.clone();
                            cx.listener(move |this, _event, _window, cx| {
                                terminal.update(cx, |terminal, cx| {
                                    terminal.stop_by_user(cx);
                                });
                                if AgentSettings::get_global(cx).cancel_generation_on_terminal_stop {
                                    this.cancel_generation(cx);
                                }
                            })
                        }),
                    )
            })
            .when(truncated_output, |header| {
                let tooltip = if let Some(output) = output {
                    if output_line_count + 10 > terminal::MAX_SCROLL_HISTORY_LINES {
                       format!("Output exceeded terminal max lines and was \
                            truncated, the model received the first {}.", format_file_size(output.content.len() as u64, true))
                    } else {
                        format!(
                            "Output is {} long, and to avoid unexpected token usage, \
                                only {} was sent back to the agent.",
                            format_file_size(output.original_content_len as u64, true),
                             format_file_size(output.content.len() as u64, true)
                        )
                    }
                } else {
                    "Output was truncated".to_string()
                };

                header.child(
                    h_flex()
                        .id(("terminal-tool-truncated-label", terminal.entity_id()))
                        .gap_1()
                        .child(
                            Icon::new(IconName::Info)
                                .size(IconSize::XSmall)
                                .color(Color::Ignored),
                        )
                        .child(
                            Label::new("Truncated")
                                .color(Color::Muted)
                                .size(LabelSize::XSmall),
                        )
                        .tooltip(Tooltip::text(tooltip)),
                )
            })
            .when(tool_failed || command_failed, |header| {
                header.child(
                    div()
                        .id(("terminal-tool-error-code-indicator", terminal.entity_id()))
                        .child(
                            Icon::new(IconName::Close)
                                .size(IconSize::Small)
                                .color(Color::Error),
                        )
                        .when_some(output.and_then(|o| o.exit_status), |this, status| {
                            this.tooltip(Tooltip::text(format!(
                                "Exited with code {}",
                                status.code().unwrap_or(-1),
                            )))
                        }),
                )
            });

        let terminal_view = self
            .entry_view_state
            .read(cx)
            .entry(entry_ix)
            .and_then(|entry| entry.terminal(terminal));

        v_flex()
            .when(layout == ToolCallLayout::Standalone, |this| {
                this.my_1p5()
                    .mx_5()
                    .border_1()
                    .when(tool_failed || command_failed, |card| card.border_dashed())
                    .border_color(border_color)
                    .rounded_md()
            })
            .overflow_hidden()
            .child(
                v_flex()
                    .group(&header_group)
                    .bg(header_bg)
                    .text_xs()
                    .child(header)
                    .child(command_element),
            )
            .when_some(tool_call.sandbox_not_applied.as_ref(), |this, reason| {
                this.child(self.render_sandbox_not_applied_warning(reason, cx))
            })
            .when(is_expanded && terminal_view.is_some(), |this| {
                this.child(
                    div()
                        .pt_2()
                        .border_t_1()
                        .when(tool_failed || command_failed, |card| card.border_dashed())
                        .border_color(border_color)
                        .bg(cx.theme().colors().editor_background)
                        .rounded_b_md()
                        .text_ui_sm(cx)
                        .h_full()
                        .children(terminal_view.map(|terminal_view| {
                            let element = if terminal_view
                                .read(cx)
                                .content_mode(window, cx)
                                .is_scrollable()
                            {
                                div().h_72().child(terminal_view).into_any_element()
                            } else {
                                terminal_view.into_any_element()
                            };

                            div()
                                .on_action(cx.listener(|_this, _: &NewTerminal, window, cx| {
                                    window.dispatch_action(NewThread.boxed_clone(), cx);
                                    cx.stop_propagation();
                                }))
                                .child(element)
                                .into_any_element()
                        })),
                )
            })
            .when_some(confirmation_options, |this, options| {
                let is_first = self.is_first_tool_call(active_session_id, &tool_call.id, cx);
                this.child(self.render_permission_buttons(
                    self.thread.read(cx).session_id().clone(),
                    is_first,
                    options,
                    entry_ix,
                    tool_call.id.clone(),
                    focus_handle,
                    cx,
                ))
            })
            .into_any()
    }
}
