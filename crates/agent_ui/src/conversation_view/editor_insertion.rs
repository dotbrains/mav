use super::*;

impl ConversationView {
    pub(crate) fn insert_dragged_files(
        &self,
        paths: Vec<project::ProjectPath>,
        added_worktrees: Vec<Entity<project::Worktree>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(message_editor) = self.loading_draft_editor() {
            message_editor.update(cx, |editor, cx| {
                editor.insert_dragged_files(paths, added_worktrees, window, cx);
                editor.focus_handle(cx).focus(window, cx);
            });
        } else if let Some(active_thread) = self.active_thread() {
            active_thread.update(cx, |thread, cx| {
                thread.message_editor.update(cx, |editor, cx| {
                    editor.insert_dragged_files(paths, added_worktrees, window, cx);
                    editor.focus_handle(cx).focus(window, cx);
                })
            });
        }
    }

    /// Inserts the selected text into the message editor or the message being
    /// edited, if any.
    pub(crate) fn insert_selection(
        &self,
        selection: AgentContextSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(message_editor) = self.loading_draft_editor() {
            message_editor.update(cx, |editor, cx| {
                editor.insert_selections(selection, window, cx);
            });
        } else if let Some(active_thread) = self.active_thread() {
            active_thread.update(cx, |thread, cx| {
                thread.active_editor(cx).update(cx, |editor, cx| {
                    editor.insert_selections(selection, window, cx);
                })
            });
        }
    }
}
