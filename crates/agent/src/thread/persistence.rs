use super::*;

impl Thread {
    pub fn to_db(&self, cx: &App) -> Task<DbThread> {
        let initial_project_snapshot = self.initial_project_snapshot.clone();
        let mut thread = DbThread {
            title: self.title().unwrap_or_default(),
            messages: self.messages.clone(),
            updated_at: self.updated_at,
            detailed_summary: self.summary.clone(),
            initial_project_snapshot: None,
            cumulative_token_usage: self.cumulative_token_usage,
            request_token_usage: self.request_token_usage.clone(),
            model: (&self.model).into(),
            profile: Some(self.profile_id.clone()),
            subagent_context: self.subagent_context.clone(),
            speed: self.speed,
            thinking_enabled: self.thinking_enabled,
            thinking_effort: self.thinking_effort.clone(),
            draft_prompt: self.draft_prompt.clone(),
            ui_scroll_position: self.ui_scroll_position.map(|lo| {
                crate::db::SerializedScrollPosition {
                    item_ix: lo.item_ix,
                    offset_in_item: lo.offset_in_item.as_f32(),
                }
            }),
            sandboxed_terminal_temp_dir: self.sandboxed_terminal_temp_dir.clone(),
            sandbox_grants: self.sandbox_grants.borrow().to_db(),
        };

        cx.background_spawn(async move {
            let initial_project_snapshot = initial_project_snapshot.await;
            thread.initial_project_snapshot = initial_project_snapshot;
            thread
        })
    }

    /// Create a snapshot of the current project state including git information and unsaved buffers.
    pub(super) fn project_snapshot(
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Arc<ProjectSnapshot>> {
        let task = project::telemetry_snapshot::TelemetrySnapshot::new(&project, cx);
        cx.spawn(async move |_, _| {
            let snapshot = task.await;

            Arc::new(ProjectSnapshot {
                worktree_snapshots: snapshot.worktree_snapshots,
                timestamp: Utc::now(),
            })
        })
    }

    pub fn project_context(&self) -> &Entity<ProjectContext> {
        &self.project_context
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn action_log(&self) -> &Entity<ActionLog> {
        &self.action_log
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.title.is_none()
    }

    pub fn draft_prompt(&self) -> Option<&[acp::ContentBlock]> {
        self.draft_prompt.as_deref()
    }

    pub fn set_draft_prompt(&mut self, prompt: Option<Vec<acp::ContentBlock>>) {
        self.draft_prompt = prompt;
    }

    pub fn ui_scroll_position(&self) -> Option<gpui::ListOffset> {
        self.ui_scroll_position
    }

    pub fn set_ui_scroll_position(&mut self, position: Option<gpui::ListOffset>) {
        self.ui_scroll_position = position;
    }
}
