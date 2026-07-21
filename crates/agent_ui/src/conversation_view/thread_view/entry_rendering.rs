use super::*;

impl ThreadView {
    pub(super) fn render_entries(&mut self, cx: &mut Context<Self>) -> List {
        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        let centered_container = move |content: AnyElement| {
            h_flex().w_full().justify_center().child(
                div()
                    .when_some(max_content_width, |this, max_w| this.max_w(max_w))
                    .w_full()
                    .child(content),
            )
        };

        list(
            self.list_state.clone(),
            cx.processor(move |this, index: usize, window, cx| {
                let entries = this.thread.read(cx).entries();
                if let Some(entry) = entries.get(index) {
                    let rendered = this.render_entry(index, entries.len(), entry, window, cx);
                    centered_container(rendered.into_any_element()).into_any_element()
                } else if this.generating_indicator_in_list {
                    let confirmation = entries
                        .last()
                        .is_some_and(|entry| Self::is_waiting_for_confirmation(entry));
                    let rendered = this.render_generating(confirmation, cx);
                    centered_container(rendered.into_any_element()).into_any_element()
                } else {
                    Empty.into_any()
                }
            }),
        )
        .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
        .flex_grow_1()
    }

    pub(super) fn render_entry(
        &self,
        entry_ix: usize,
        total_entries: usize,
        entry: &AgentThreadEntry,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_indented = entry.is_indented();
        let is_first_indented = is_indented
            && self
                .thread
                .read(cx)
                .entries()
                .get(entry_ix.saturating_sub(1))
                .is_none_or(|entry| !entry.is_indented());

        let primary = match &entry {
            AgentThreadEntry::UserMessage(message) => self.render_user_message_entry(
                entry_ix,
                message,
                is_indented,
                is_first_indented,
                window,
                cx,
            ),
            AgentThreadEntry::AssistantMessage(AssistantMessage {
                chunks,
                indented: _,
                is_subagent_output: _,
            }) => self.render_assistant_message_entry(entry_ix, total_entries, chunks, window, cx),
            AgentThreadEntry::ToolCall(tool_call) => self
                .render_tool_call_entry(entry_ix, tool_call, window, cx)
                .unwrap_or_else(|| Empty.into_any()),
            AgentThreadEntry::CompletedPlan(entries) => {
                self.render_completed_plan(entries, window, cx)
            }
            AgentThreadEntry::ContextCompaction(compaction) => {
                self.render_context_compaction(entry_ix, compaction, window, cx)
            }
        };

        let is_subagent_output = self.is_subagent()
            && matches!(entry, AgentThreadEntry::AssistantMessage(msg) if msg.is_subagent_output);

        let primary = self.wrap_subagent_output(primary, is_subagent_output);
        let primary = self.wrap_indented_entry(primary, is_indented, is_first_indented, cx);
        let primary = self.wrap_last_entry(primary, entry_ix, total_entries, entry, cx);
        self.wrap_edit_backdrop(primary, entry_ix, cx)
    }

    fn render_assistant_message_entry(
        &self,
        entry_ix: usize,
        total_entries: usize,
        chunks: &[AssistantMessageChunk],
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut is_blank = true;
        let is_last = entry_ix + 1 == total_entries;
        let style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx);
        let message_body = v_flex()
            .w_full()
            .gap_3()
            .children(
                chunks
                    .iter()
                    .enumerate()
                    .filter_map(|(chunk_ix, chunk)| match chunk {
                        AssistantMessageChunk::Message { block, .. } => {
                            block.markdown().and_then(|md| {
                                let this_is_blank = md.read(cx).source().trim().is_empty();
                                is_blank = is_blank && this_is_blank;
                                (!this_is_blank).then(|| {
                                    self.render_markdown(md.clone(), style.clone(), cx)
                                        .into_any_element()
                                })
                            })
                        }
                        AssistantMessageChunk::Thought { block, .. } => {
                            block.markdown().and_then(|md| {
                                let this_is_blank = md.read(cx).source().trim().is_empty();
                                is_blank = is_blank && this_is_blank;
                                (!this_is_blank).then(|| {
                                    self.render_thinking_block(
                                        entry_ix,
                                        chunk_ix,
                                        md.clone(),
                                        window,
                                        cx,
                                    )
                                    .into_any_element()
                                })
                            })
                        }
                    }),
            )
            .into_any();

        if is_blank {
            Empty.into_any()
        } else {
            v_flex()
                .px_5()
                .py_1p5()
                .when(is_last, |this| this.pb_4())
                .w_full()
                .text_ui(cx)
                .child(self.render_message_context_menu(entry_ix, message_body, cx))
                .when_some(
                    self.entry_view_state
                        .read(cx)
                        .entry(entry_ix)
                        .and_then(|entry| entry.focus_handle(cx)),
                    |this, handle| this.track_focus(&handle),
                )
                .into_any()
        }
    }

    fn render_tool_call_entry(
        &self,
        entry_ix: usize,
        tool_call: &ToolCall,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if matches!(tool_call.status, ToolCallStatus::Canceled) {
            let has_visible_content = tool_call.content.iter().any(|content| match content {
                ToolCallContent::ContentBlock(block) => block.visible_content(cx),
                ToolCallContent::Diff(_) | ToolCallContent::Terminal(_) => true,
            });
            if !has_visible_content {
                return None;
            }
        }

        let tool_call = self.render_any_tool_call(
            self.thread.read(cx).session_id(),
            entry_ix,
            tool_call,
            &self.focus_handle(cx),
            ToolCallLayout::Standalone,
            window,
            cx,
        );

        Some(
            if let Some(handle) = self
                .entry_view_state
                .read(cx)
                .entry(entry_ix)
                .and_then(|entry| entry.focus_handle(cx))
            {
                tool_call.track_focus(&handle).into_any()
            } else {
                tool_call.into_any()
            },
        )
    }

    fn wrap_subagent_output(&self, primary: AnyElement, is_subagent_output: bool) -> AnyElement {
        if is_subagent_output {
            v_flex()
                .w_full()
                .child(
                    h_flex()
                        .id("subagent_output")
                        .px_5()
                        .py_1()
                        .gap_2()
                        .child(Divider::horizontal())
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Icon::new(IconName::ForwardArrowUp)
                                        .color(Color::Muted)
                                        .size(IconSize::Small),
                                )
                                .child(
                                    Label::new("Subagent Output")
                                        .size(LabelSize::Custom(self.tool_name_font_size()))
                                        .color(Color::Muted),
                                ),
                        )
                        .child(Divider::horizontal())
                        .tooltip(Tooltip::text("Everything below this line was sent as output from this subagent to the main agent.")),
                )
                .child(primary)
                .into_any_element()
        } else {
            primary
        }
    }

    fn wrap_indented_entry(
        &self,
        primary: AnyElement,
        is_indented: bool,
        is_first_indented: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        if is_indented {
            let line_top = if is_first_indented {
                rems_from_px(-12.0)
            } else {
                rems_from_px(0.0)
            };

            div()
                .relative()
                .w_full()
                .pl_5()
                .bg(cx.theme().colors().panel_background.opacity(0.2))
                .child(
                    div()
                        .absolute()
                        .left(rems_from_px(18.0))
                        .top(line_top)
                        .bottom_0()
                        .w_px()
                        .bg(cx.theme().colors().border.opacity(0.6)),
                )
                .child(primary)
                .into_any_element()
        } else {
            primary
        }
    }

    fn wrap_last_entry(
        &self,
        primary: AnyElement,
        entry_ix: usize,
        total_entries: usize,
        entry: &AgentThreadEntry,
        cx: &Context<Self>,
    ) -> AnyElement {
        if entry_ix + 1 != total_entries {
            return primary;
        }

        let thread = self.thread.clone();
        let needs_confirmation = Self::is_waiting_for_confirmation(entry);
        let comments_editor = self.thread_feedback.comments_editor.clone();

        v_flex()
            .w_full()
            .child(primary)
            .when(!needs_confirmation, |this| {
                this.child(self.render_thread_controls(&thread, cx))
            })
            .when_some(comments_editor, |this, editor| {
                this.child(Self::render_feedback_feedback_editor(editor, cx))
            })
            .into_any_element()
    }

    fn wrap_edit_backdrop(
        &self,
        primary: AnyElement,
        entry_ix: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        if let Some(editing_index) = self.editing_message
            && editing_index < entry_ix
        {
            let is_subagent = self.is_subagent();
            let backdrop = div()
                .id(("backdrop", entry_ix))
                .size_full()
                .absolute()
                .inset_0()
                .bg(cx.theme().colors().panel_background)
                .opacity(0.8)
                .block_mouse_except_scroll()
                .on_click(cx.listener(Self::cancel_editing));

            div()
                .relative()
                .child(primary)
                .when(!is_subagent, |this| this.child(backdrop))
                .into_any_element()
        } else {
            primary
        }
    }
}
