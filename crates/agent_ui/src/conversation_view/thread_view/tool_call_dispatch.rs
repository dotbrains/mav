use super::*;

impl ThreadView {
    pub(super) fn is_first_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        tool_call_id: &acp::ToolCallId,
        cx: &App,
    ) -> bool {
        self.conversation
            .read(cx)
            .pending_tool_call(active_session_id, cx)
            .map_or(false, |(pending_session_id, pending_tool_call_id, _)| {
                self.thread.read(cx).session_id() == &pending_session_id
                    && tool_call_id == &pending_tool_call_id
            })
    }

    pub(super) fn render_any_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        layout: ToolCallLayout,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let has_terminals = tool_call.terminals().next().is_some();

        // Give every tool-call subtree a unique element-id prefix derived from
        // the globally-unique tool call id and the layout. This single wrapper
        // is what keeps all the `entry_ix`-keyed element ids inside the card
        // collision-free, even when the same tool call is rendered in multiple
        // places at once (inline list + floating awaiting-permission row) or
        // when subagent entries are inlined into the parent view's element tree.
        let container_id = ElementId::Name(SharedString::from(format!(
            "tool-call-{}-{}",
            tool_call.id.0,
            layout.id_str()
        )));

        div().w_full().id(container_id).map(|this| {
            if tool_call.is_subagent() {
                this.child(
                    self.render_subagent_tool_call(
                        active_session_id,
                        entry_ix,
                        tool_call,
                        tool_call
                            .subagent_session_info
                            .as_ref()
                            .map(|i| i.session_id.clone()),
                        focus_handle,
                        window,
                        cx,
                    ),
                )
            } else if has_terminals {
                this.children(tool_call.terminals().map(|terminal| {
                    self.render_terminal_tool_call(
                        active_session_id,
                        entry_ix,
                        terminal,
                        tool_call,
                        focus_handle,
                        layout,
                        window,
                        cx,
                    )
                }))
            } else {
                this.child(self.render_tool_call(
                    active_session_id,
                    entry_ix,
                    tool_call,
                    focus_handle,
                    layout,
                    window,
                    cx,
                ))
            }
        })
    }
}
