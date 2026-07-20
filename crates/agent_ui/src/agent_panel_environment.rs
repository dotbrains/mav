use super::*;

impl AgentPanel {
    pub(super) fn default_terminal_working_directory(&self, cx: &App) -> Option<PathBuf> {
        // Reuse the workspace-based helper so behavior matches the regular
        // terminal panel (e.g. `WorkingDirectory::FirstProjectDirectory` falling
        // back to a file's parent directory when the worktree root is a file).
        self.workspace
            .upgrade()
            .and_then(|workspace| terminal_view::default_working_directory(workspace.read(cx), cx))
    }

    pub(super) fn has_open_project(&self, cx: &App) -> bool {
        self.project.read(cx).visible_worktrees(cx).next().is_some()
    }

    pub(super) fn ensure_native_agent_connection(&self, cx: &mut Context<Self>) {
        if !self.has_open_project(cx) {
            return;
        }

        let fs = self.fs.clone();
        let thread_store = self.thread_store.clone();
        self.connection_store.update(cx, |store, cx| {
            store.request_connection(
                Agent::NativeAgent,
                Agent::NativeAgent.server(fs, thread_store),
                cx,
            );
        });
    }
}
