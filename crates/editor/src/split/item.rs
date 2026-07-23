use super::*;

impl Item for SplittableEditor {
    type Event = EditorEvent;

    fn tab_content_text(&self, detail: usize, cx: &App) -> ui::SharedString {
        self.rhs_editor.read(cx).tab_content_text(detail, cx)
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<ui::SharedString> {
        self.rhs_editor.read(cx).tab_tooltip_text(cx)
    }

    fn tab_icon(&self, window: &Window, cx: &App) -> Option<ui::Icon> {
        self.rhs_editor.read(cx).tab_icon(window, cx)
    }

    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> gpui::AnyElement {
        self.rhs_editor.read(cx).tab_content(params, window, cx)
    }

    fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f)
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        self.rhs_editor.read(cx).for_each_project_item(cx, f)
    }

    fn buffer_kind(&self, cx: &App) -> ItemBufferKind {
        self.rhs_editor.read(cx).buffer_kind(cx)
    }

    fn active_project_path(&self, cx: &App) -> Option<project::ProjectPath> {
        self.rhs_editor.read(cx).active_project_path(cx)
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).has_conflict(cx)
    }

    fn has_deleted_file(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).has_deleted_file(cx)
    }

    fn capability(&self, cx: &App) -> language::Capability {
        self.rhs_editor.read(cx).capability(cx)
    }

    fn can_save(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).can_save(cx)
    }

    fn can_save_as(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).can_save_as(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: project::ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.save_as(project, path, window, cx))
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.reload(project, window, cx))
    }

    fn navigate(
        &mut self,
        data: Arc<dyn std::any::Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.focused_editor()
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_editor().update(cx, |editor, cx| {
            editor.deactivated(window, cx);
        });
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace = workspace.weak_handle();
        self.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.added_to_workspace(workspace, window, cx);
        });
        if let Some(lhs) = &self.lhs {
            lhs.editor.update(cx, |lhs_editor, cx| {
                lhs_editor.added_to_workspace(workspace, window, cx);
            });
        }
    }

    fn as_searchable(
        &self,
        handle: &Entity<Self>,
        _: &App,
    ) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(handle.clone()))
    }

    fn breadcrumb_location(&self, cx: &App) -> ToolbarItemLocation {
        self.rhs_editor.read(cx).breadcrumb_location(cx)
    }

    fn breadcrumbs(&self, cx: &App) -> Option<(Vec<HighlightedText>, Option<Font>)> {
        self.rhs_editor.read(cx).breadcrumbs(cx)
    }

    fn pixel_position_of_cursor(&self, cx: &App) -> Option<gpui::Point<gpui::Pixels>> {
        self.focused_editor().read(cx).pixel_position_of_cursor(cx)
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: std::any::TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == std::any::TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == std::any::TypeId::of::<Editor>() {
            Some(self.rhs_editor.clone().into())
        } else {
            None
        }
    }
}
