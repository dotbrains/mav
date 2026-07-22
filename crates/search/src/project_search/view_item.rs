use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewEvent {
    UpdateTab,
    Activate,
    EditorEvent(editor::EditorEvent),
    Dismiss,
}

impl EventEmitter<ViewEvent> for ProjectSearchView {}

impl Render for ProjectSearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::default();
        key_context.add("ProjectSearchView");

        if self.has_matches() {
            div()
                .key_context(key_context)
                .on_action(cx.listener(Self::open_text_finder))
                .flex_1()
                .size_full()
                .track_focus(&self.focus_handle(cx))
                .child(self.results_editor.clone())
        } else {
            let model = self.entity.read(cx);

            let heading_text = match model.search_state {
                SearchState::Running(SearchActivity::WaitingForScan) => "Loading project…",
                SearchState::Running(SearchActivity::Searching) => "Searching…",
                SearchState::Completed(SearchCompletion::NoResults) => "No Results",
                _ => "Search All Files",
            };

            let heading_text = div()
                .justify_center()
                .child(Label::new(heading_text).size(LabelSize::Large));

            let page_content: Option<AnyElement> = match model.search_state {
                SearchState::Idle => Some(self.landing_text_minor(cx).into_any_element()),
                SearchState::Completed(SearchCompletion::NoResults) => Some(
                    Label::new("No results found in this project for the provided query")
                        .size(LabelSize::Small)
                        .into_any_element(),
                ),
                _ => None,
            };

            let page_content = page_content.map(|text| div().child(text));

            h_flex()
                .key_context(key_context)
                .on_action(cx.listener(Self::open_text_finder))
                .size_full()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(cx.theme().colors().editor_background)
                .track_focus(&self.focus_handle(cx))
                .child(
                    v_flex()
                        .id("project-search-landing-page")
                        .overflow_y_scroll()
                        .gap_1()
                        .child(heading_text)
                        .children(page_content),
                )
        }
    }
}

impl Focusable for ProjectSearchView {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for ProjectSearchView {
    type Event = ViewEvent;
    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let query_text = self.query_editor.read(cx).text(cx);

        query_text
            .is_empty()
            .not()
            .then(|| query_text.into())
            .or_else(|| Some("Project Search".into()))
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            Some(self.results_editor.clone().into())
        } else {
            None
        }
    }
    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.results_editor.clone()))
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.results_editor
            .update(cx, |editor, cx| editor.deactivated(window, cx));
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::MagnifyingGlass))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        let last_query: Option<SharedString> = self
            .entity
            .read(cx)
            .last_search_query_text
            .as_ref()
            .map(|query| {
                let query = query.replace('\n', "");
                let query_text = util::truncate_and_trailoff(&query, MAX_TAB_TITLE_LEN);
                query_text.into()
            });

        last_query
            .filter(|query| !query.is_empty())
            .unwrap_or_else(|| "Project Search".into())
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Project Search Opened")
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        self.results_editor.for_each_project_item(cx, f)
    }

    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.results_editor.read(cx).active_project_path(cx)
    }

    fn can_save(&self, _: &App) -> bool {
        true
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.results_editor.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.results_editor.read(cx).has_conflict(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.results_editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        _: Entity<Project>,
        _: ProjectPath,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        unreachable!("save_as should not have been called")
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.results_editor
            .update(cx, |editor, cx| editor.reload(project, window, cx))
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        let model = self.entity.update(cx, |model, cx| model.clone(cx));
        Task::ready(Some(cx.new(|cx| {
            Self::new(self.workspace.clone(), model, window, cx, None)
        })))
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.results_editor.update(cx, |editor, cx| {
            editor.added_to_workspace(workspace, window, cx)
        });
    }

    fn set_nav_history(
        &mut self,
        nav_history: ItemNavHistory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.results_editor.update(cx, |editor, _| {
            editor.set_nav_history(Some(nav_history));
        });
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.results_editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            ViewEvent::UpdateTab => {
                f(ItemEvent::UpdateBreadcrumbs);
                f(ItemEvent::UpdateTab);
            }
            ViewEvent::EditorEvent(editor_event) => {
                Editor::to_item_events(editor_event, f);
            }
            ViewEvent::Dismiss => f(ItemEvent::CloseItem),
            _ => {}
        }
    }
}
