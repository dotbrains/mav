use super::*;

impl Editor {
    pub fn insert_snippet_at_selections(
        &mut self,
        action: &InsertSnippet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.try_insert_snippet_at_selections(action, window, cx)
            .log_err();
    }
}
