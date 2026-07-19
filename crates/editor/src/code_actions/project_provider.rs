use super::*;

impl CodeActionProvider for Entity<Project> {
    fn id(&self) -> Arc<str> {
        "project".into()
    }

    fn code_actions(
        &self,
        buffer: &Entity<Buffer>,
        range: Range<text::Anchor>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Vec<CodeAction>>> {
        self.update(cx, |project, cx| {
            let code_lens_actions = if EditorSettings::get_global(cx).code_lens.show_in_menu() {
                Some(project.code_lens_actions(buffer, range.clone(), cx))
            } else {
                None
            };
            let code_actions = project.code_actions(buffer, range, None, cx);
            cx.background_spawn(async move {
                let code_lens_actions = match code_lens_actions {
                    Some(task) => task.await.context("code lens fetch")?.unwrap_or_default(),
                    None => Vec::new(),
                };
                let code_actions = code_actions
                    .await
                    .context("code action fetch")?
                    .unwrap_or_default();
                Ok(code_lens_actions.into_iter().chain(code_actions).collect())
            })
        })
    }

    fn apply_code_action(
        &self,
        buffer_handle: Entity<Buffer>,
        action: CodeAction,
        push_to_history: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<ProjectTransaction>> {
        self.update(cx, |project, cx| {
            project.apply_code_action(buffer_handle, action, push_to_history, cx)
        })
    }
}
