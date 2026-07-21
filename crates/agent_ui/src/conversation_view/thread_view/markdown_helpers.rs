use super::*;

impl ThreadView {
    pub fn open_thread_as_markdown(
        &self,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let thread = self.thread.read(cx);
        let thread_title = thread
            .title()
            .unwrap_or_else(|| DEFAULT_THREAD_TITLE.into())
            .to_string();
        let markdown = thread.to_markdown(cx);

        open_markdown_in_workspace(thread_title, markdown, workspace, window, cx)
    }

    pub(super) fn render_markdown(
        &self,
        markdown: Entity<Markdown>,
        style: MarkdownStyle,
        cx: &App,
    ) -> MarkdownElement {
        render_agent_markdown(
            markdown,
            style,
            &self.workspace,
            &self.code_span_resolver,
            cx,
        )
    }
}
