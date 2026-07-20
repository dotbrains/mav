use super::*;

impl ThreadView {
    pub(super) fn render_subagent_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        tool_call: &ToolCall,
        subagent_session_id: Option<acp::SessionId>,
        focus_handle: &FocusHandle,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let subagent_thread_view = subagent_session_id.and_then(|session_id| {
            self.server_view
                .upgrade()
                .and_then(|server_view| server_view.read(cx).as_connected())
                .and_then(|connected| connected.threads.get(&session_id))
        });

        let content = self.render_subagent_card(
            active_session_id,
            entry_ix,
            subagent_thread_view,
            tool_call,
            focus_handle,
            window,
            cx,
        );

        v_flex().mx_5().my_1p5().gap_3().child(content)
    }

    pub(super) fn render_subagent_expanded_content(
        &self,
        thread_view: &Entity<ThreadView>,
        tool_call: &ToolCall,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        const MAX_PREVIEW_ENTRIES: usize = 8;

        let subagent_view = thread_view.read(cx);
        let session_id = subagent_view.thread.read(cx).session_id().clone();

        let is_canceled_or_failed = matches!(
            tool_call.status,
            ToolCallStatus::Canceled | ToolCallStatus::Failed | ToolCallStatus::Rejected
        );

        let editor_bg = cx.theme().colors().editor_background;
        let overlay = {
            div()
                .absolute()
                .inset_0()
                .size_full()
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(editor_bg.opacity(0.5), 0.),
                    linear_color_stop(editor_bg.opacity(0.), 0.1),
                ))
                .block_mouse_except_scroll()
        };

        let entries = subagent_view.thread.read(cx).entries();
        let total_entries = entries.len();
        let mut entry_range = if let Some(info) = tool_call.subagent_session_info.as_ref() {
            info.message_start_index
                ..info
                    .message_end_index
                    .map(|i| (i + 1).min(total_entries))
                    .unwrap_or(total_entries)
        } else {
            0..total_entries
        };
        entry_range.start = entry_range
            .end
            .saturating_sub(MAX_PREVIEW_ENTRIES)
            .max(entry_range.start);
        let start_ix = entry_range.start;

        let scroll_handle = self
            .subagent_scroll_handles
            .borrow_mut()
            .entry(subagent_view.session_id.clone())
            .or_default()
            .clone();

        scroll_handle.scroll_to_bottom();

        let rendered_entries: Vec<AnyElement> = entries
            .get(entry_range)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let actual_ix = start_ix + i;
                subagent_view.render_entry(actual_ix, total_entries, entry, window, cx)
            })
            .collect();

        v_flex()
            .w_full()
            .border_t_1()
            .when(is_canceled_or_failed, |this| this.border_dashed())
            .border_color(self.tool_card_border_color(cx))
            .overflow_hidden()
            .child(
                div()
                    .pb_1()
                    .min_h_0()
                    .id(format!(
                        "subagent-entries-{}-{}",
                        session_id, tool_call.id.0
                    ))
                    .track_scroll(&scroll_handle)
                    .children(rendered_entries),
            )
            .h_56()
            .child(overlay)
            .into_any_element()
    }

    pub(super) fn subagent_error_message(
        &self,
        status: &ToolCallStatus,
        tool_call: &ToolCall,
        cx: &App,
    ) -> Option<SharedString> {
        if matches!(status, ToolCallStatus::Failed) {
            tool_call.content.iter().find_map(|content| {
                if let ToolCallContent::ContentBlock(block) = content {
                    if let Some(source) = block.text_content(cx).filter(|source| !source.is_empty())
                    {
                        if source == "User canceled" {
                            return None;
                        } else {
                            return Some(SharedString::from(source));
                        }
                    }
                }
                None
            })
        } else {
            None
        }
    }
}
