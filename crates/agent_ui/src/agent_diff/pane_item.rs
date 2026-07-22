use super::*;

impl EventEmitter<EditorEvent> for AgentDiffPane {}

impl Focusable for AgentDiffPane {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.multibuffer.read(cx).is_empty() {
            self.focus_handle.clone()
        } else {
            self.editor.focus_handle(cx)
        }
    }
}

impl Item for AgentDiffPane {
    type Event = EditorEvent;

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::MavAssistant).color(Color::Muted))
    }

    fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f)
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.deactivated(window, cx));
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn tab_tooltip_text(&self, _: &App) -> Option<SharedString> {
        Some("Agent Diff".into())
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let title = self.thread.read(cx).title();
        Label::new(if let Some(title) = title {
            format!("Review: {}", title)
        } else {
            "Review".to_string()
        })
        .color(if params.selected {
            Color::Default
        } else {
            Color::Muted
        })
        .into_any_element()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Assistant Diff Opened")
    }

    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.editor.clone()))
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        self.editor
            .read(cx)
            .rhs_editor()
            .for_each_project_item(cx, f)
    }

    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.editor.read(cx).active_project_path(cx)
    }

    fn set_nav_history(
        &mut self,
        nav_history: ItemNavHistory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.rhs_editor().update(cx, |editor, _| {
                editor.set_nav_history(Some(nav_history));
            });
        });
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<workspace::WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        Task::ready(Some(cx.new(|cx| {
            Self::new(self.thread.clone(), self.workspace.clone(), window, cx)
        })))
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.multibuffer.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.multibuffer.read(cx).has_conflict(cx)
    }

    fn can_save(&self, _: &App) -> bool {
        true
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.editor.save(options, project, window, cx)
    }

    fn save_as(
        &mut self,
        _: Entity<Project>,
        _: ProjectPath,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) -> Task<Result<()>> {
        unreachable!()
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.editor.reload(project, window, cx)
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        cx: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else {
            self.editor.act_as_type(type_id, cx)
        }
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.added_to_workspace(workspace, window, cx)
        });
    }

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Agent Diff".into()
    }
}

impl Render for AgentDiffPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_empty = self.multibuffer.read(cx).is_empty();
        let focus_handle = &self.focus_handle;

        div()
            .track_focus(focus_handle)
            .key_context(if is_empty { "EmptyPane" } else { "AgentDiff" })
            .on_action(cx.listener(Self::keep))
            .on_action(cx.listener(Self::reject))
            .on_action(cx.listener(Self::reject_all))
            .on_action(cx.listener(Self::keep_all))
            // Only paint the background for the empty state. When the diff editor
            // is shown it already paints `editor_background`; painting it again
            // here double-composites into a darker patch on transparent windows.
            .when(is_empty, |el| el.bg(cx.theme().colors().editor_background))
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .when(is_empty, |el| {
                el.child(
                    v_flex()
                        .items_center()
                        .gap_2()
                        .child("No changes to review")
                        .child(
                            Button::new("continue-iterating", "Continue Iterating")
                                .style(ButtonStyle::Filled)
                                .start_icon(
                                    Icon::new(IconName::ForwardArrow)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .full_width()
                                .key_binding(KeyBinding::for_action_in(
                                    &ToggleFocus,
                                    &focus_handle.clone(),
                                    cx,
                                ))
                                .on_click(|_event, window, cx| {
                                    window.dispatch_action(ToggleFocus.boxed_clone(), cx)
                                }),
                        ),
                )
            })
            .when(!is_empty, |el| el.child(self.editor.clone()))
    }
}
