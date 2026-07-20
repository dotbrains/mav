use super::*;

pub(super) struct InitialProjectHandles {
    pub(super) bookmark_store: Option<Entity<BookmarkStore>>,
    pub(super) breakpoint_store: Option<Entity<BreakpointStore>>,
    pub(super) code_action_providers: Vec<Rc<dyn CodeActionProvider>>,
    pub(super) load_uncommitted_diff: Option<Shared<Task<()>>>,
}

impl Editor {
    pub(super) fn initial_project_handles(
        mode: &EditorMode,
        project: &Option<Entity<Project>>,
        multi_buffer: &Entity<MultiBuffer>,
        cx: &mut Context<Self>,
    ) -> InitialProjectHandles {
        let bookmark_store = match (mode, project.as_ref()) {
            (EditorMode::Full { .. }, Some(project)) => Some(project.read(cx).bookmark_store()),
            _ => None,
        };

        let breakpoint_store = match (mode, project.as_ref()) {
            (EditorMode::Full { .. }, Some(project)) => Some(project.read(cx).breakpoint_store()),
            _ => None,
        };

        let mut code_action_providers = Vec::new();
        let mut load_uncommitted_diff = None;
        if let Some(project) = project.clone() {
            load_uncommitted_diff = Some(
                update_uncommitted_diff_for_buffer(
                    cx.entity(),
                    &project,
                    multi_buffer.read(cx).all_buffers(),
                    multi_buffer.clone(),
                    cx,
                )
                .shared(),
            );
            code_action_providers.push(Rc::new(project) as Rc<_>);
        }

        InitialProjectHandles {
            bookmark_store,
            breakpoint_store,
            code_action_providers,
            load_uncommitted_diff,
        }
    }
}
