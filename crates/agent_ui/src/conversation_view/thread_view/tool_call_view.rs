use super::*;

impl ThreadView {
    pub(super) fn render_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        layout: ToolCallLayout,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let has_location = tool_call.locations.len() == 1;
        let card_header_id = SharedString::from(format!("inner-tool-call-header-{entry_ix}"));

        let failed_or_canceled = match &tool_call.status {
            ToolCallStatus::Rejected | ToolCallStatus::Canceled | ToolCallStatus::Failed => true,
            _ => false,
        };

        let needs_confirmation = matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation { .. }
        );
        let is_terminal_tool = matches!(tool_call.kind, acp::ToolKind::Execute);

        let is_edit =
            matches!(tool_call.kind, acp::ToolKind::Edit) || tool_call.diffs().next().is_some();

        let is_cancelled_edit = is_edit && matches!(tool_call.status, ToolCallStatus::Canceled);
        let (has_revealed_diff, tool_call_output_focus, tool_call_output_focus_handle) = tool_call
            .diffs()
            .next()
            .and_then(|diff| {
                let editor = self
                    .entry_view_state
                    .read(cx)
                    .entry(entry_ix)
                    .and_then(|entry| entry.editor_for_diff(diff))?;
                let has_revealed_diff = diff.read(cx).has_revealed_range(cx);
                let has_focus = editor.read(cx).is_focused(window);
                let focus_handle = editor.focus_handle(cx);
                Some((has_revealed_diff, has_focus, focus_handle))
            })
            .unwrap_or_else(|| (false, false, focus_handle.clone()));

        let use_card_layout = needs_confirmation || is_edit || is_terminal_tool;

        let has_image_content = tool_call.content.iter().any(|c| c.image().is_some());
        let is_collapsible = !tool_call.content.is_empty() && !needs_confirmation;
        let mut is_open = self
            .entry_view_state
            .read(cx)
            .is_tool_call_expanded(&tool_call.id);

        is_open |= needs_confirmation;

        let should_show_raw_input = !is_terminal_tool && !is_edit && !has_image_content;

        let tool_output_display = self.render_tool_call_body(
            active_session_id,
            entry_ix,
            tool_call,
            focus_handle,
            layout,
            use_card_layout,
            failed_or_canceled,
            is_edit,
            is_open,
            should_show_raw_input,
            window,
            cx,
        );

        v_flex()
            .map(|this| {
                if matches!(layout, ToolCallLayout::Embedded | ToolCallLayout::Floating) {
                    this
                } else if use_card_layout {
                    this.my_1p5()
                        .rounded_md()
                        .border_1()
                        .when(failed_or_canceled, |this| this.border_dashed())
                        .border_color(self.tool_card_border_color(cx))
                        .bg(cx.theme().colors().editor_background)
                        .overflow_hidden()
                } else {
                    this.my_1()
                }
            })
            .when(layout == ToolCallLayout::Standalone, |this| {
                this.map(|this| {
                    if has_location && !use_card_layout {
                        this.ml_4()
                    } else {
                        this.ml_5()
                    }
                })
                .mr_5()
            })
            .child(self.render_tool_call_header(
                entry_ix,
                tool_call,
                card_header_id,
                is_terminal_tool,
                is_edit,
                is_cancelled_edit,
                has_revealed_diff,
                use_card_layout,
                is_collapsible,
                failed_or_canceled,
                is_open,
                tool_call_output_focus,
                tool_call_output_focus_handle,
                window,
                cx,
            ))
            .children(tool_output_display)
    }
}
