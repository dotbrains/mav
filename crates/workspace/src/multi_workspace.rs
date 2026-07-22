use anyhow::Result;
use fs::Fs;

use gpui::{
    AnyView, App, Context, DragMoveEvent, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    ManagedView, MouseButton, Pixels, Render, Subscription, Task, TaskExt, Tiling, WeakEntity,
    Window, WindowId, actions, deferred, px,
};
use mav_actions::sidebar::ToggleThreadSwitcher;
use project::Project;
pub use project::ProjectGroupKey;
use remote::RemoteConnectionOptions;
use settings::Settings;
pub use settings::SidebarSide;
use std::cell::Cell;
use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use ui::prelude::*;
use util::ResultExt;
use util::path_list::PathList;

use crate::workspace_settings::SidebarSettings;
use settings::SidebarDockPosition;
use ui::{ContextMenu, Tooltip, right_click_menu};

const SIDEBAR_RESIZE_HANDLE_SIZE: Pixels = px(6.0);
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_INSET: Pixels = px(9.0);

use crate::open_remote_project_with_existing_connection;
use crate::{
    CloseIntent, CloseWindow, DockPosition, Event as WorkspaceEvent, Item, ModalView, OpenMode,
    PaneKind, Panel, ToggleProjectPane, Workspace, WorkspaceId, client_side_decorations,
    persistence::model::MultiWorkspaceState, workspace_card_gap,
};

actions!(
    sidebar,
    [
        /// Toggles the sidebar.
        ToggleSidebar,
        /// Closes the sidebar.
        CloseSidebar,
        /// Moves focus to or from the sidebar without closing it.
        FocusSidebar,
        /// Activates the next project in the sidebar.
        NextProject,
        /// Activates the previous project in the sidebar.
        PreviousProject,
        /// Activates the next thread in sidebar order.
        NextThread,
        /// Activates the previous thread in sidebar order.
        PreviousThread,
    ]
);

actions!(
    multi_workspace,
    [
        /// Moves the active project to a new window.
        MoveProjectToNewWindow,
    ]
);

mod persistence;
mod project_group_actions;
mod project_groups;
mod render;
mod sidebar;
mod sidebar_lifecycle;
mod workspace_activation;
mod workspace_delegation;
mod workspace_open;

pub use sidebar::{
    SidebarRenderState, render_sidebar_header_controls,
    render_sidebar_header_controls_with_project_pane_visibility,
    render_sidebar_header_controls_with_state, sidebar_side_context_menu,
};
pub enum MultiWorkspaceEvent {
    ActiveWorkspaceChanged {
        source_workspace: Option<WeakEntity<Workspace>>,
    },
    WorkspaceAdded(Entity<Workspace>),
    WorkspaceRemoved(EntityId),
    ProjectGroupsChanged,
}

pub enum SidebarEvent {
    SerializeNeeded,
}

pub trait Sidebar: Focusable + Render + EventEmitter<SidebarEvent> + Sized {
    fn width(&self, cx: &App) -> Pixels;
    fn set_width(&mut self, width: Option<Pixels>, cx: &mut Context<Self>);
    fn has_notifications(&self, cx: &App) -> bool;
    fn side(&self, _cx: &App) -> SidebarSide;

    fn is_threads_list_view_active(&self) -> bool {
        true
    }
    /// Makes focus reset back to the search editor upon toggling the sidebar from outside
    fn prepare_for_focus(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
    /// Opens or cycles the thread switcher popup.
    fn toggle_thread_switcher(
        &mut self,
        _select_last: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// Activates the next or previous project.
    fn cycle_project(&mut self, _forward: bool, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Activates the next or previous thread in sidebar order.
    fn cycle_thread(&mut self, _forward: bool, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Toggles the sidebar's primary options menu, if it has one.
    fn toggle_options_menu(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Simulates update states for chrome testing.
    fn simulate_update_available(&mut self, _cx: &mut Context<Self>) {}

    #[cfg(not(target_os = "macos"))]
    fn open_application_menu(
        &mut self,
        _menu_name: String,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    #[cfg(not(target_os = "macos"))]
    fn activate_application_menu(
        &mut self,
        _right: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// Return an opaque JSON blob of sidebar-specific state to persist.
    fn serialized_state(&self, _cx: &App) -> Option<String> {
        None
    }

    /// Restore sidebar state from a previously-serialized blob.
    fn restore_serialized_state(
        &mut self,
        _state: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

pub trait SidebarHandle: 'static + Send + Sync {
    fn width(&self, cx: &App) -> Pixels;
    fn set_width(&self, width: Option<Pixels>, cx: &mut App);
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn focus(&self, window: &mut Window, cx: &mut App);
    fn prepare_for_focus(&self, window: &mut Window, cx: &mut App);
    fn has_notifications(&self, cx: &App) -> bool;
    fn to_any(&self) -> AnyView;
    fn entity_id(&self) -> EntityId;
    fn toggle_thread_switcher(&self, select_last: bool, window: &mut Window, cx: &mut App);
    fn cycle_project(&self, forward: bool, window: &mut Window, cx: &mut App);
    fn cycle_thread(&self, forward: bool, window: &mut Window, cx: &mut App);
    fn toggle_options_menu(&self, window: &mut Window, cx: &mut App);
    fn simulate_update_available(&self, cx: &mut App);
    #[cfg(not(target_os = "macos"))]
    fn open_application_menu(&self, menu_name: String, window: &mut Window, cx: &mut App);
    #[cfg(not(target_os = "macos"))]
    fn activate_application_menu(&self, right: bool, window: &mut Window, cx: &mut App);

    fn is_threads_list_view_active(&self, cx: &App) -> bool;

    fn side(&self, cx: &App) -> SidebarSide;
    fn serialized_state(&self, cx: &App) -> Option<String>;
    fn restore_serialized_state(&self, state: &str, window: &mut Window, cx: &mut App);
}

#[derive(Clone)]
pub struct DraggedSidebar;

impl Render for DraggedSidebar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl<T: Sidebar> SidebarHandle for Entity<T> {
    fn width(&self, cx: &App) -> Pixels {
        self.read(cx).width(cx)
    }

    fn set_width(&self, width: Option<Pixels>, cx: &mut App) {
        self.update(cx, |this, cx| this.set_width(width, cx))
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn prepare_for_focus(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.prepare_for_focus(window, cx));
    }

    fn has_notifications(&self, cx: &App) -> bool {
        self.read(cx).has_notifications(cx)
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn entity_id(&self) -> EntityId {
        Entity::entity_id(self)
    }

    fn toggle_thread_switcher(&self, select_last: bool, window: &mut Window, cx: &mut App) {
        let entity = self.clone();
        window.defer(cx, move |window, cx| {
            entity.update(cx, |this, cx| {
                this.toggle_thread_switcher(select_last, window, cx);
            });
        });
    }

    fn cycle_project(&self, forward: bool, window: &mut Window, cx: &mut App) {
        let entity = self.clone();
        window.defer(cx, move |window, cx| {
            entity.update(cx, |this, cx| {
                this.cycle_project(forward, window, cx);
            });
        });
    }

    fn cycle_thread(&self, forward: bool, window: &mut Window, cx: &mut App) {
        let entity = self.clone();
        window.defer(cx, move |window, cx| {
            entity.update(cx, |this, cx| {
                this.cycle_thread(forward, window, cx);
            });
        });
    }

    fn toggle_options_menu(&self, window: &mut Window, cx: &mut App) {
        let entity = self.clone();
        window.defer(cx, move |window, cx| {
            entity.update(cx, |this, cx| {
                this.toggle_options_menu(window, cx);
            });
        });
    }

    fn simulate_update_available(&self, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.simulate_update_available(cx);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn open_application_menu(&self, menu_name: String, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.open_application_menu(menu_name, window, cx);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn activate_application_menu(&self, right: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.activate_application_menu(right, window, cx);
        });
    }

    fn is_threads_list_view_active(&self, cx: &App) -> bool {
        self.read(cx).is_threads_list_view_active()
    }

    fn side(&self, cx: &App) -> SidebarSide {
        self.read(cx).side(cx)
    }

    fn serialized_state(&self, cx: &App) -> Option<String> {
        self.read(cx).serialized_state(cx)
    }

    fn restore_serialized_state(&self, state: &str, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.restore_serialized_state(state, window, cx)
        })
    }
}

#[derive(Clone)]
pub struct ProjectGroup {
    pub key: ProjectGroupKey,
    pub workspaces: Vec<Entity<Workspace>>,
    pub expanded: bool,
}

pub struct SerializedProjectGroupState {
    pub key: ProjectGroupKey,
    pub expanded: bool,
}

#[derive(Clone)]
pub struct ProjectGroupState {
    pub key: ProjectGroupKey,
    pub expanded: bool,
    pub last_active_workspace: Option<WeakEntity<Workspace>>,
}

pub struct MultiWorkspace {
    window_id: WindowId,
    retained_workspaces: Vec<Entity<Workspace>>,
    project_groups: Vec<ProjectGroupState>,
    active_workspace: Entity<Workspace>,
    /// Source of truth for which workspace is presented in this window, shared
    /// with each member `Workspace` so they can tell whether they own the
    /// platform window's title and edited indicator. This only exists to prevent
    /// Workspaces from having to read their parent MultiWorkspace to check
    /// chrome ownership, as that might cause a double lease. Kept in sync with
    /// `active_workspace`.
    active_workspace_id: Rc<Cell<EntityId>>,
    sidebar: Option<Box<dyn SidebarHandle>>,
    sidebar_open: bool,
    pending_sidebar_state: Option<String>,
    sidebar_overlay: Option<AnyView>,
    pending_removal_tasks: Vec<Task<()>>,
    _serialize_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    previous_focus_handle: Option<FocusHandle>,
}

impl EventEmitter<MultiWorkspaceEvent> for MultiWorkspace {}
