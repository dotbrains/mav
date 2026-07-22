use super::*;

#[cfg(any(test, feature = "test-support"))]
pub mod test {
    use super::{Item, ItemEvent, SerializableItem, TabContentParams};
    use crate::{
        ItemId, ItemNavHistory, Workspace, WorkspaceId,
        item::{ItemBufferKind, SaveOptions},
    };
    use gpui::{
        AnyElement, App, AppContext as _, Context, Entity, EntityId, EventEmitter, Focusable,
        InteractiveElement, IntoElement, ParentElement, Render, SharedString, Task, WeakEntity,
        Window,
    };
    use project::{Project, ProjectEntryId, ProjectPath, WorktreeId};
    use std::{any::Any, cell::Cell, sync::Arc};
    use util::rel_path::rel_path;

    pub struct TestProjectItem {
        pub entry_id: Option<ProjectEntryId>,
        pub project_path: Option<ProjectPath>,
        pub is_dirty: bool,
    }

    pub struct TestItem {
        pub workspace_id: Option<WorkspaceId>,
        pub state: String,
        pub label: String,
        pub save_count: usize,
        pub save_as_count: usize,
        pub reload_count: usize,
        pub is_dirty: bool,
        pub buffer_kind: ItemBufferKind,
        pub has_conflict: bool,
        pub has_deleted_file: bool,
        pub project_items: Vec<Entity<TestProjectItem>>,
        pub nav_history: Option<ItemNavHistory>,
        pub tab_descriptions: Option<Vec<&'static str>>,
        pub tab_detail: Cell<Option<usize>>,
        serialize: Option<Box<dyn Fn() -> Option<Task<anyhow::Result<()>>>>>,
        focus_handle: gpui::FocusHandle,
        pub child_focus_handles: Vec<gpui::FocusHandle>,
    }

    impl project::ProjectItem for TestProjectItem {
        fn try_open(
            _project: &Entity<Project>,
            _path: &ProjectPath,
            _cx: &mut App,
        ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
            None
        }
        fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
            self.entry_id
        }

        fn project_path(&self, _: &App) -> Option<ProjectPath> {
            self.project_path.clone()
        }

        fn is_dirty(&self) -> bool {
            self.is_dirty
        }
    }

    pub enum TestItemEvent {
        Edit,
    }

    impl TestProjectItem {
        pub fn new(id: u64, path: &str, cx: &mut App) -> Entity<Self> {
            Self::new_in_worktree(id, path, WorktreeId::from_usize(0), cx)
        }

        pub fn new_in_worktree(
            id: u64,
            path: &str,
            worktree_id: WorktreeId,
            cx: &mut App,
        ) -> Entity<Self> {
            let entry_id = Some(ProjectEntryId::from_proto(id));
            let project_path = Some(ProjectPath {
                worktree_id,
                path: rel_path(path).into(),
            });
            cx.new(|_| Self {
                entry_id,
                project_path,
                is_dirty: false,
            })
        }

        pub fn new_untitled(cx: &mut App) -> Entity<Self> {
            cx.new(|_| Self {
                project_path: None,
                entry_id: None,
                is_dirty: false,
            })
        }

        pub fn new_dirty(id: u64, path: &str, cx: &mut App) -> Entity<Self> {
            let entry_id = Some(ProjectEntryId::from_proto(id));
            let project_path = Some(ProjectPath {
                worktree_id: WorktreeId::from_usize(0),
                path: rel_path(path).into(),
            });
            cx.new(|_| Self {
                entry_id,
                project_path,
                is_dirty: true,
            })
        }
    }

    impl TestItem {
        pub fn new(cx: &mut Context<Self>) -> Self {
            Self {
                state: String::new(),
                label: String::new(),
                save_count: 0,
                save_as_count: 0,
                reload_count: 0,
                is_dirty: false,
                has_conflict: false,
                has_deleted_file: false,
                project_items: Vec::new(),
                buffer_kind: ItemBufferKind::Singleton,
                nav_history: None,
                tab_descriptions: None,
                tab_detail: Default::default(),
                workspace_id: Default::default(),
                focus_handle: cx.focus_handle(),
                serialize: None,
                child_focus_handles: Vec::new(),
            }
        }

        pub fn new_deserialized(id: WorkspaceId, cx: &mut Context<Self>) -> Self {
            let mut this = Self::new(cx);
            this.workspace_id = Some(id);
            this
        }

        pub fn with_label(mut self, state: &str) -> Self {
            self.label = state.to_string();
            self
        }

        pub fn with_buffer_kind(mut self, buffer_kind: ItemBufferKind) -> Self {
            self.buffer_kind = buffer_kind;
            self
        }

        pub fn set_has_deleted_file(&mut self, deleted: bool) {
            self.has_deleted_file = deleted;
        }

        pub fn with_dirty(mut self, dirty: bool) -> Self {
            self.is_dirty = dirty;
            self
        }

        pub fn with_conflict(mut self, has_conflict: bool) -> Self {
            self.has_conflict = has_conflict;
            self
        }

        pub fn with_project_items(mut self, items: &[Entity<TestProjectItem>]) -> Self {
            self.project_items.clear();
            self.project_items.extend(items.iter().cloned());
            self
        }

        pub fn with_serialize(
            mut self,
            serialize: impl Fn() -> Option<Task<anyhow::Result<()>>> + 'static,
        ) -> Self {
            self.serialize = Some(Box::new(serialize));
            self
        }

        pub fn with_child_focus_handles(mut self, count: usize, cx: &mut Context<Self>) -> Self {
            self.child_focus_handles = (0..count).map(|_| cx.focus_handle()).collect();
            self
        }

        pub fn set_state(&mut self, state: String, cx: &mut Context<Self>) {
            self.push_to_nav_history(cx);
            self.state = state;
        }

        fn push_to_nav_history(&mut self, cx: &mut Context<Self>) {
            if let Some(history) = &mut self.nav_history {
                history.push(Some(Box::new(self.state.clone())), None, cx);
            }
        }
    }

    impl Render for TestItem {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let parent = gpui::div().track_focus(&self.focus_handle(cx));
            self.child_focus_handles
                .iter()
                .fold(parent, |parent, child_handle| {
                    parent.child(gpui::div().track_focus(child_handle))
                })
        }
    }

    impl EventEmitter<ItemEvent> for TestItem {}

    impl Focusable for TestItem {
        fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Item for TestItem {
        type Event = ItemEvent;

        fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
            f(*event)
        }

        fn tab_content_text(&self, detail: usize, _cx: &App) -> SharedString {
            self.tab_descriptions
                .as_ref()
                .and_then(|descriptions| {
                    let description = *descriptions.get(detail).or_else(|| descriptions.last())?;
                    description.into()
                })
                .unwrap_or_default()
                .into()
        }

        fn telemetry_event_text(&self) -> Option<&'static str> {
            None
        }

        fn tab_content(&self, params: TabContentParams, _window: &Window, _cx: &App) -> AnyElement {
            self.tab_detail.set(params.detail);
            gpui::div().into_any_element()
        }

        fn for_each_project_item(
            &self,
            cx: &App,
            f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
        ) {
            self.project_items
                .iter()
                .for_each(|item| f(item.entity_id(), item.read(cx)))
        }

        fn buffer_kind(&self, _: &App) -> ItemBufferKind {
            self.buffer_kind
        }

        fn set_nav_history(
            &mut self,
            history: ItemNavHistory,
            _window: &mut Window,
            _: &mut Context<Self>,
        ) {
            self.nav_history = Some(history);
        }

        fn navigate(
            &mut self,
            state: Arc<dyn Any + Send>,
            _window: &mut Window,
            _: &mut Context<Self>,
        ) -> bool {
            if let Some(state) = state.downcast_ref::<Box<String>>() {
                let state = *state.clone();
                if state != self.state {
                    false
                } else {
                    self.state = state;
                    true
                }
            } else {
                false
            }
        }

        fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
            self.push_to_nav_history(cx);
        }

        fn can_split(&self) -> bool {
            true
        }

        fn clone_on_split(
            &self,
            _workspace_id: Option<WorkspaceId>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Task<Option<Entity<Self>>>
        where
            Self: Sized,
        {
            Task::ready(Some(cx.new(|cx| {
                Self {
                    state: self.state.clone(),
                    label: self.label.clone(),
                    save_count: self.save_count,
                    save_as_count: self.save_as_count,
                    reload_count: self.reload_count,
                    is_dirty: self.is_dirty,
                    buffer_kind: self.buffer_kind,
                    has_conflict: self.has_conflict,
                    has_deleted_file: self.has_deleted_file,
                    project_items: self.project_items.clone(),
                    nav_history: None,
                    tab_descriptions: None,
                    tab_detail: Default::default(),
                    workspace_id: self.workspace_id,
                    focus_handle: cx.focus_handle(),
                    serialize: None,
                    child_focus_handles: self
                        .child_focus_handles
                        .iter()
                        .map(|_| cx.focus_handle())
                        .collect(),
                }
            })))
        }

        fn is_dirty(&self, _: &App) -> bool {
            self.is_dirty
        }

        fn has_conflict(&self, _: &App) -> bool {
            self.has_conflict
        }

        fn has_deleted_file(&self, _: &App) -> bool {
            self.has_deleted_file
        }

        fn can_save(&self, cx: &App) -> bool {
            !self.project_items.is_empty()
                && self
                    .project_items
                    .iter()
                    .all(|item| item.read(cx).entry_id.is_some())
        }

        fn can_save_as(&self, _cx: &App) -> bool {
            self.buffer_kind == ItemBufferKind::Singleton
        }

        fn save(
            &mut self,
            _: SaveOptions,
            _: Entity<Project>,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> Task<anyhow::Result<()>> {
            self.save_count += 1;
            self.is_dirty = false;
            for item in &self.project_items {
                item.update(cx, |item, _| {
                    if item.is_dirty {
                        item.is_dirty = false;
                    }
                })
            }
            Task::ready(Ok(()))
        }

        fn save_as(
            &mut self,
            _: Entity<Project>,
            _: ProjectPath,
            _window: &mut Window,
            _: &mut Context<Self>,
        ) -> Task<anyhow::Result<()>> {
            self.save_as_count += 1;
            self.is_dirty = false;
            Task::ready(Ok(()))
        }

        fn reload(
            &mut self,
            _: Entity<Project>,
            _window: &mut Window,
            _: &mut Context<Self>,
        ) -> Task<anyhow::Result<()>> {
            self.reload_count += 1;
            self.is_dirty = false;
            Task::ready(Ok(()))
        }
    }

    impl SerializableItem for TestItem {
        fn serialized_item_kind() -> &'static str {
            "TestItem"
        }

        fn deserialize(
            _project: Entity<Project>,
            _workspace: WeakEntity<Workspace>,
            workspace_id: WorkspaceId,
            _item_id: ItemId,
            _window: &mut Window,
            cx: &mut App,
        ) -> Task<anyhow::Result<Entity<Self>>> {
            let entity = cx.new(|cx| Self::new_deserialized(workspace_id, cx));
            Task::ready(Ok(entity))
        }

        fn cleanup(
            _workspace_id: WorkspaceId,
            _alive_items: Vec<ItemId>,
            _window: &mut Window,
            _cx: &mut App,
        ) -> Task<anyhow::Result<()>> {
            Task::ready(Ok(()))
        }

        fn serialize(
            &mut self,
            _workspace: &mut Workspace,
            _item_id: ItemId,
            _closing: bool,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> Option<Task<anyhow::Result<()>>> {
            if let Some(serialize) = self.serialize.take() {
                let result = serialize();
                self.serialize = Some(serialize);
                result
            } else {
                None
            }
        }

        fn should_serialize(&self, _event: &Self::Event) -> bool {
            false
        }
    }
}
