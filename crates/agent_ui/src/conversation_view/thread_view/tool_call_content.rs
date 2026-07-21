use super::*;

impl ThreadView {
    pub(super) fn render_tool_call_content(
        &self,
        session_id: &acp::SessionId,
        entry_ix: usize,
        content: &ToolCallContent,
        context_ix: usize,
        tool_call: &ToolCall,
        card_layout: bool,
        has_failed: bool,
        focus_handle: &FocusHandle,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        match content {
            ToolCallContent::ContentBlock(content) => {
                if let Some((resource, markdown)) = content.embedded_resource() {
                    self.render_embedded_resource_output(
                        resource,
                        markdown.cloned(),
                        entry_ix,
                        context_ix,
                        tool_call,
                        card_layout,
                        window,
                        cx,
                    )
                } else if let Some(resource_link) = content.resource_link() {
                    self.render_resource_link(resource_link, cx)
                } else if let Some(markdown) = content.markdown() {
                    self.render_markdown_output(
                        markdown.clone(),
                        entry_ix,
                        context_ix,
                        tool_call,
                        card_layout,
                        window,
                        cx,
                    )
                } else if let Some((image, _)) = content.image() {
                    let location = tool_call.locations.first().cloned();
                    self.render_image_output(entry_ix, image.clone(), location, card_layout, cx)
                } else {
                    Empty.into_any_element()
                }
            }
            ToolCallContent::Diff(diff) => {
                self.render_diff_editor(entry_ix, diff, tool_call, has_failed, cx)
            }
            ToolCallContent::Terminal(terminal) => self.render_terminal_tool_call(
                session_id,
                entry_ix,
                terminal,
                tool_call,
                focus_handle,
                ToolCallLayout::Standalone,
                window,
                cx,
            ),
        }
    }
}
