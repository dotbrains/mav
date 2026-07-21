use super::*;

impl Sidebar {
    pub(super) fn create_new_entry(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if workspace_path_list(workspace, cx).paths().is_empty() {
            return;
        }

        if self.should_create_terminal_for_workspace(workspace, cx) {
            self.create_new_terminal(workspace, window, cx);
        } else {
            self.create_new_thread(workspace, window, cx);
        }
    }

    pub(super) fn should_create_terminal_for_workspace(
        &self,
        _workspace: &Entity<Workspace>,
        _cx: &App,
    ) -> bool {
        false
    }

    pub(super) fn create_new_thread(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if workspace_path_list(workspace, cx).paths().is_empty() {
            return;
        }

        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.activate(workspace.clone(), None, window, cx);
        });

        let draft = create_agent_thread_in_workspace(workspace, true, window, cx);

        if let Some(draft) = draft {
            let draft_id = draft.read(cx).thread_id(cx);
            self.active_entry = Some(ActiveEntry::Thread {
                thread_id: draft_id,
                session_id: None,
                workspace: workspace.clone(),
            });
        }
    }

    pub(super) fn create_new_terminal(
        &mut self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if workspace_path_list(workspace, cx).paths().is_empty() {
            return;
        }

        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.activate(workspace.clone(), None, window, cx);
        });

        workspace.update(cx, |workspace, cx| {
            if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.new_terminal(Some(workspace), AgentThreadSource::Sidebar, window, cx);
                });
            }
            workspace.focus_panel::<AgentPanel>(window, cx);
        });
    }
}
