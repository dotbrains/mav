use anyhow::{Context as _, Result, anyhow};
use client::{
    ErrorExt,
    proto::{self, ErrorCode},
};
use collections::{HashMap, TypeIdHashMap};
use gpui::{
    AnyEntity, AnyView, App, AppContext, BorrowAppContext, Context, Entity, Global, Task,
    WeakEntity, Window,
};
use project::{Project, ProjectEntryId, ProjectPath};
use std::{any::TypeId, sync::Arc};

use crate::{
    ViewId, Workspace, WorkspaceId,
    item::{
        FollowableItem, FollowableItemHandle, ItemHandle, ProjectItem, SerializableItem,
        SerializableItemHandle,
    },
    pane::Pane,
    persistence::model::ItemId,
};

type BuildProjectItemFn =
    fn(AnyEntity, Entity<Project>, Option<&Pane>, &mut Window, &mut App) -> Box<dyn ItemHandle>;

type BuildProjectItemForPathFn =
    fn(
        &Entity<Project>,
        &ProjectPath,
        &mut Window,
        &mut App,
    ) -> Option<Task<Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>>>;

#[derive(Clone, Default)]
pub(crate) struct ProjectItemRegistry {
    build_project_item_fns_by_type: TypeIdHashMap<BuildProjectItemFn>,
    build_project_item_for_path_fns: Vec<BuildProjectItemForPathFn>,
}

impl ProjectItemRegistry {
    fn register<T: ProjectItem>(&mut self) {
        self.build_project_item_fns_by_type.insert(
            TypeId::of::<T::Item>(),
            |item, project, pane, window, cx| {
                let item = item.downcast().unwrap();
                Box::new(cx.new(|cx| T::for_project_item(project, pane, item, window, cx)))
                    as Box<dyn ItemHandle>
            },
        );
        self.build_project_item_for_path_fns
            .push(|project, project_path, window, cx| {
                let project_path = project_path.clone();
                let is_file = project
                    .read(cx)
                    .entry_for_path(&project_path, cx)
                    .is_some_and(|entry| entry.is_file());
                let entry_abs_path = project.read(cx).absolute_path(&project_path, cx);
                let is_local = project.read(cx).is_local();
                let project_item =
                    <T::Item as project::ProjectItem>::try_open(project, &project_path, cx)?;
                let project = project.clone();
                Some(window.spawn(cx, async move |cx| {
                    match project_item.await.with_context(|| {
                        format!(
                            "opening project path {:?}",
                            entry_abs_path.as_deref().unwrap_or(&project_path.path.as_std_path())
                        )
                    }) {
                        Ok(project_item) => {
                            let project_item = project_item;
                            let project_entry_id: Option<ProjectEntryId> =
                                project_item.read_with(cx, project::ProjectItem::entry_id);
                            let build_workspace_item = Box::new(
                                |pane: &mut Pane, window: &mut Window, cx: &mut Context<Pane>| {
                                    Box::new(cx.new(|cx| {
                                        T::for_project_item(
                                            project,
                                            Some(pane),
                                            project_item,
                                            window,
                                            cx,
                                        )
                                    })) as Box<dyn ItemHandle>
                                },
                            ) as Box<_>;
                            Ok((project_entry_id, build_workspace_item))
                        }
                        Err(e) => {
                            log::warn!("Failed to open a project item: {e:#}");
                            if e.error_code() == ErrorCode::Internal {
                                if let Some(abs_path) =
                                    entry_abs_path.as_deref().filter(|_| is_file)
                                {
                                    if let Some(broken_project_item_view) =
                                        cx.update(|window, cx| {
                                            T::for_broken_project_item(
                                                abs_path, is_local, &e, window, cx,
                                            )
                                        })?
                                    {
                                        let build_workspace_item = Box::new(
                                            move |_: &mut Pane, _: &mut Window, cx: &mut Context<Pane>| {
                                                cx.new(|_| broken_project_item_view).boxed_clone()
                                            },
                                        )
                                        as Box<_>;
                                        return Ok((None, build_workspace_item));
                                    }
                                }
                            }
                            Err(e)
                        }
                    }
                }))
            });
    }

    pub(crate) fn open_path(
        &self,
        project: &Entity<Project>,
        path: &ProjectPath,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>> {
        let Some(open_project_item) = self
            .build_project_item_for_path_fns
            .iter()
            .rev()
            .find_map(|open_project_item| open_project_item(project, path, window, cx))
        else {
            return Task::ready(Err(anyhow!("cannot open file {:?}", path.path)));
        };
        open_project_item
    }

    pub(crate) fn build_item<T: project::ProjectItem>(
        &self,
        item: Entity<T>,
        project: Entity<Project>,
        pane: Option<&Pane>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Box<dyn ItemHandle>> {
        let build = self
            .build_project_item_fns_by_type
            .get(&TypeId::of::<T>())?;
        Some(build(item.into_any(), project, pane, window, cx))
    }
}

pub(crate) type WorkspaceItemBuilder =
    Box<dyn FnOnce(&mut Pane, &mut Window, &mut Context<Pane>) -> Box<dyn ItemHandle>>;

impl Global for ProjectItemRegistry {}

/// Registers a [ProjectItem] for the app. When opening a file, all the registered
/// items will get a chance to open the file, starting from the project item that
/// was added last.
pub fn register_project_item<I: ProjectItem>(cx: &mut App) {
    cx.default_global::<ProjectItemRegistry>().register::<I>();
}

#[derive(Default)]
pub struct FollowableViewRegistry(TypeIdHashMap<FollowableViewDescriptor>);

struct FollowableViewDescriptor {
    from_state_proto: fn(
        Entity<Workspace>,
        ViewId,
        &mut Option<proto::view::Variant>,
        &mut Window,
        &mut App,
    ) -> Option<Task<Result<Box<dyn FollowableItemHandle>>>>,
    to_followable_view: fn(&AnyView) -> Box<dyn FollowableItemHandle>,
}

impl Global for FollowableViewRegistry {}

impl FollowableViewRegistry {
    pub fn register<I: FollowableItem>(cx: &mut App) {
        cx.default_global::<Self>().0.insert(
            TypeId::of::<I>(),
            FollowableViewDescriptor {
                from_state_proto: |workspace, id, state, window, cx| {
                    I::from_state_proto(workspace, id, state, window, cx).map(|task| {
                        cx.foreground_executor()
                            .spawn(async move { Ok(Box::new(task.await?) as Box<_>) })
                    })
                },
                to_followable_view: |view| Box::new(view.clone().downcast::<I>().unwrap()),
            },
        );
    }

    pub fn from_state_proto(
        workspace: Entity<Workspace>,
        view_id: ViewId,
        mut state: Option<proto::view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Task<Result<Box<dyn FollowableItemHandle>>>> {
        cx.update_default_global(|this: &mut Self, cx| {
            this.0.values().find_map(|descriptor| {
                (descriptor.from_state_proto)(workspace.clone(), view_id, &mut state, window, cx)
            })
        })
    }

    pub fn to_followable_view(
        view: impl Into<AnyView>,
        cx: &App,
    ) -> Option<Box<dyn FollowableItemHandle>> {
        let this = cx.try_global::<Self>()?;
        let view = view.into();
        let descriptor = this.0.get(&view.entity_type())?;
        Some((descriptor.to_followable_view)(&view))
    }
}

#[derive(Copy, Clone)]
struct SerializableItemDescriptor {
    deserialize: fn(
        Entity<Project>,
        WeakEntity<Workspace>,
        WorkspaceId,
        ItemId,
        &mut Window,
        &mut Context<Pane>,
    ) -> Task<Result<Box<dyn ItemHandle>>>,
    cleanup: fn(WorkspaceId, Vec<ItemId>, &mut Window, &mut App) -> Task<Result<()>>,
    view_to_serializable_item: fn(AnyView) -> Box<dyn SerializableItemHandle>,
}

#[derive(Default)]
pub(crate) struct SerializableItemRegistry {
    descriptors_by_kind: HashMap<Arc<str>, SerializableItemDescriptor>,
    descriptors_by_type: TypeIdHashMap<SerializableItemDescriptor>,
}

impl Global for SerializableItemRegistry {}

impl SerializableItemRegistry {
    pub(crate) fn deserialize(
        item_kind: &str,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_item: ItemId,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> Task<Result<Box<dyn ItemHandle>>> {
        let Some(descriptor) = Self::descriptor(item_kind, cx) else {
            return Task::ready(Err(anyhow!(
                "cannot deserialize {}, descriptor not found",
                item_kind
            )));
        };

        (descriptor.deserialize)(project, workspace, workspace_id, item_item, window, cx)
    }

    pub(crate) fn cleanup(
        item_kind: &str,
        workspace_id: WorkspaceId,
        loaded_items: Vec<ItemId>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let Some(descriptor) = Self::descriptor(item_kind, cx) else {
            return Task::ready(Err(anyhow!(
                "cannot cleanup {}, descriptor not found",
                item_kind
            )));
        };

        (descriptor.cleanup)(workspace_id, loaded_items, window, cx)
    }

    pub(crate) fn view_to_serializable_item_handle(
        view: AnyView,
        cx: &App,
    ) -> Option<Box<dyn SerializableItemHandle>> {
        let this = cx.try_global::<Self>()?;
        let descriptor = this.descriptors_by_type.get(&view.entity_type())?;
        Some((descriptor.view_to_serializable_item)(view))
    }

    fn descriptor(item_kind: &str, cx: &App) -> Option<SerializableItemDescriptor> {
        let this = cx.try_global::<Self>()?;
        this.descriptors_by_kind.get(item_kind).copied()
    }
}

pub fn register_serializable_item<I: SerializableItem>(cx: &mut App) {
    let serialized_item_kind = I::serialized_item_kind();

    let registry = cx.default_global::<SerializableItemRegistry>();
    let descriptor = SerializableItemDescriptor {
        deserialize: |project, workspace, workspace_id, item_id, window, cx| {
            let task = I::deserialize(project, workspace, workspace_id, item_id, window, cx);
            cx.foreground_executor()
                .spawn(async { Ok(Box::new(task.await?) as Box<_>) })
        },
        cleanup: |workspace_id, loaded_items, window, cx| {
            I::cleanup(workspace_id, loaded_items, window, cx)
        },
        view_to_serializable_item: |view| Box::new(view.downcast::<I>().unwrap()),
    };
    registry
        .descriptors_by_kind
        .insert(Arc::from(serialized_item_kind), descriptor);
    registry
        .descriptors_by_type
        .insert(TypeId::of::<I>(), descriptor);
}
