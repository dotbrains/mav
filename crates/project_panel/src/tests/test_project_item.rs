pub(crate) struct TestProjectItemView {
    focus_handle: FocusHandle,
    path: ProjectPath,
}

struct TestProjectItem {
    path: ProjectPath,
}

impl project::ProjectItem for TestProjectItem {
    fn try_open(
        _project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let path = path.clone();
        Some(cx.spawn(async move |cx| Ok(cx.new(|_| Self { path }))))
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        None
    }

    fn project_path(&self, _: &App) -> Option<ProjectPath> {
        Some(self.path.clone())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

impl ProjectItem for TestProjectItemView {
    type Item = TestProjectItem;

    fn for_project_item(
        _: Entity<Project>,
        _: Option<&Pane>,
        project_item: Entity<Self::Item>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            path: project_item.update(cx, |project_item, _| project_item.path.clone()),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Item for TestProjectItemView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Test".into()
    }
}

impl EventEmitter<()> for TestProjectItemView {}

impl Focusable for TestProjectItemView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TestProjectItemView {
    fn render(&mut self, _window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}
