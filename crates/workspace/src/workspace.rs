mod active_call;
pub mod active_file_name;
mod delayed_edit_action;
pub mod dock;
pub mod history_manager;
pub mod invalid_item_view;
pub mod item;
mod modal_layer;
mod multi_workspace;
#[cfg(test)]
mod multi_workspace_tests;
pub mod notifications;
pub mod pane;
pub mod pane_group;
mod pane_movement;
pub mod panel_pane;
pub mod path_list {
    pub use util::path_list::{PathList, SerializedPathList};
}
pub mod path_link;
mod persistence;
pub mod searchable;
pub mod security_modal;
pub mod shared_screen;
pub use shared_screen::SharedScreen;
pub mod focus_follows_mouse;
mod legacy_panel_size;
mod remote_project_deserialize;
mod remote_workspace_position;
mod status_bar;
pub mod tasks;
mod theme_preview;
mod toast_layer;
mod toolbar;
pub mod welcome;
mod window_chrome;
mod workspace_accessors;
mod workspace_action_wiring;
mod workspace_actions;
mod workspace_app_state;
mod workspace_call;
mod workspace_channel;
mod workspace_constructor;
mod workspace_docks;
pub mod workspace_error;
mod workspace_event;
mod workspace_focus_targets;
mod workspace_follower_rpc;
mod workspace_following;
mod workspace_global_open;
mod workspace_id;
mod workspace_item_opening;
mod workspace_items;
mod workspace_keystrokes;
mod workspace_leader_state;
mod workspace_lifecycle;
mod workspace_local;
mod workspace_location_helpers;
mod workspace_navigation;
mod workspace_notifications;
mod workspace_open_items;
mod workspace_open_options;
mod workspace_open_prompt;
mod workspace_pane_actions;
mod workspace_pane_events;
mod workspace_pane_layout;
mod workspace_pane_membership;
mod workspace_panel_focus;
mod workspace_path_opening;
mod workspace_project_items;
mod workspace_project_join;
mod workspace_providers;
mod workspace_registries;
mod workspace_reload;
mod workspace_remote_open;
mod workspace_render;
mod workspace_render_notifications;
mod workspace_restore;
mod workspace_serialization;
mod workspace_serialized_items;
mod workspace_settings;
mod workspace_store;
#[cfg(any(test, feature = "test-support"))]
mod workspace_test_support;
mod workspace_types;
mod workspace_ui_actions;
mod workspace_window_lookup;
mod workspace_window_state;
mod workspace_window_utils;

pub use active_call::{
    ActiveCallEvent, AnyActiveCall, GlobalAnyActiveCall, ParticipantLocation, RemoteCollaborator,
};
pub use dock::Panel;
pub use multi_workspace::{
    CloseSidebar, DraggedSidebar, FocusSidebar, MoveProjectToNewWindow, MultiWorkspace,
    MultiWorkspaceEvent, NextProject, NextThread, PreviousProject, PreviousThread, ProjectGroup,
    ProjectGroupKey, SerializedProjectGroupState, Sidebar, SidebarEvent, SidebarHandle,
    SidebarRenderState, SidebarSide, ToggleSidebar, render_sidebar_header_controls,
    render_sidebar_header_controls_with_project_pane_visibility,
    render_sidebar_header_controls_with_state, sidebar_side_context_menu,
};
pub use path_list::{PathList, SerializedPathList};
pub use remote::{
    RemoteConnectionIdentity, remote_connection_identity, same_remote_connection_identity,
};
pub use toast_layer::{ToastAction, ToastLayer, ToastView};
pub use workspace_app_state::AppState;
pub use workspace_channel::{
    CopyRoomId, Deafen, LeaveCall, Mute, OpenChannelNotes, OpenChannelNotesById, OpenLog,
    RevealLogInFileManager, ScreenShare, ShareProject, join_channel,
};
pub use workspace_global_open::{
    create_and_open_local_file, find_existing_workspace, open_new, open_paths, open_workspace_by_id,
};
pub use workspace_project_join::{join_in_room_project, with_active_or_new_workspace};
pub(crate) use workspace_render::DraggedDock;

use anyhow::{Context as _, Result, anyhow};
use client::{
    ChannelId, Client, ErrorExt, Status, UserStore,
    proto::{self, ErrorCode, PanelId, PeerId},
};
use collections::{HashMap, HashSet, hash_map};
use dock::{Dock, DockPosition, PanelHandle, RESIZE_HANDLE_SIZE};
use futures::{
    Future, FutureExt, StreamExt,
    channel::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::{Shared, try_join_all},
};
use gpui::{
    Action, AnyWeakView, App, AsyncApp, AsyncWindowContext, Axis, Bounds, Context, DragMoveEvent,
    Entity, EntityId, EventEmitter, FocusHandle, Focusable, Global, KeyContext, Keystroke,
    ManagedView, PathPromptOptions, Point, PromptLevel, Render, Subscription,
    SystemWindowTabController, Task, TaskExt, WeakEntity, WindowBounds, WindowHandle, WindowId,
    WindowOptions, actions, canvas, relative,
};
pub use history_manager::*;
pub use item::{
    FollowableItem, FollowableItemHandle, Item, ItemHandle, ItemSettings, PreviewTabsSettings,
    ProjectItem, SerializableItem, SerializableItemHandle, WeakItemHandle,
};
use itertools::Itertools;
use language::{LanguageRegistry, Rope, language_settings::all_language_settings};
use legacy_panel_size::load_legacy_panel_size;
pub use modal_layer::*;
use node_runtime::NodeRuntime;
use notifications::{
    DetachAndPromptErr, Notifications, dismiss_app_notification,
    simple_message_notification::MessageNotification,
};
pub use pane::*;
pub use pane_group::{
    ActivePaneDecorator, HANDLE_HITBOX_SIZE, Member, PaneAxis, PaneGroup, PaneRenderContext,
    SplitDirection, SplitSizeHint,
};
pub use pane_movement::{clone_active_item, move_active_item, move_item};
use pane_movement::{join_pane_into_active, move_all_items};
use panel_pane::{PanelItem, PanelPaneKind, configure_agent_pane, configure_project_pane};
pub use persistence::{
    RecentWorkspace, WorkspaceDb, delete_unloaded_items,
    model::{
        DockData, DockStructure, ItemId, MultiWorkspaceState, SerializedMultiWorkspace,
        SerializedProjectGroup, SerializedWorkspaceLocation, SessionWorkspace,
    },
    read_serialized_multi_workspaces,
};
use persistence::{SerializedWindowBounds, model::SerializedWorkspace};
use postage::stream::Stream;
use project::{
    DirectoryLister, Project, ProjectEntryId, ProjectPath, ResolvedPath, Worktree, WorktreeId,
    WorktreeSettings,
    debugger::{breakpoint_store::BreakpointStoreEvent, session::ThreadStatus},
    project_settings::ProjectSettings,
    toolchain_store::ToolchainStoreEvent,
    trusted_worktrees::{RemoteHostLocation, TrustedWorktrees, TrustedWorktreesEvent},
};
use session::AppSession;
use settings::{
    CenteredPaddingSettings, DefaultOpenBehavior, Settings, SettingsLocation, SettingsStore,
    update_settings_file,
};

pub(crate) use delayed_edit_action::DelayedDebouncedEditAction;
use mav_actions::{Spawn, theme::ToggleMode};
pub use remote_workspace_position::{WorkspacePosition, remote_workspace_position_from_db};
use status_bar::StatusBar;
pub use status_bar::{HideStatusItem, StatusItemView, add_hide_button_entry};
use std::{
    cell::{Cell, RefCell},
    cmp,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, atomic::AtomicUsize},
    time::Duration,
};
use theme::{ActiveTheme, SystemAppearance};
use theme_settings::ThemeSettings;
pub use toolbar::{
    PaneSearchBarCallbacks, Toolbar, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView,
};
pub use ui;
use ui::{Window, prelude::*};
use url::Url;
use util::{
    ResultExt, TryFutureExt,
    paths::{PathStyle, SanitizedPath},
    rel_path::RelPath,
};
use uuid::Uuid;
pub use window_chrome::client_side_decorations;
pub(crate) use window_chrome::window_bounds_env_override;
pub use workspace_actions::*;
pub use workspace_event::Event;
use workspace_focus_targets::{ActivateInDirectionTarget, dock_has_focus_target};
pub use workspace_id::WorkspaceId;
use workspace_keystrokes::DispatchingKeystrokes;
pub use workspace_location_helpers::{
    WorkspaceHandle, last_opened_workspace_location, last_session_workspace_locations,
};
use workspace_notifications::notify_if_database_failed;
pub(crate) use workspace_open_items::open_items;
pub use workspace_open_options::{OpenOptions, OpenResult, OpenVisible, WorkspaceMatching};
use workspace_open_prompt::prompt_and_open_paths;
pub use workspace_open_prompt::prompt_for_open_path_and_open;
pub use workspace_providers::{DebuggerProvider, TerminalProvider};
pub use workspace_registries::{
    FollowableViewRegistry, register_project_item, register_serializable_item,
};
pub(crate) use workspace_registries::{
    ProjectItemRegistry, SerializableItemRegistry, WorkspaceItemBuilder,
};
pub use workspace_reload::reload;
pub use workspace_remote_open::{
    open_remote_project_with_existing_connection, open_remote_project_with_new_connection,
};
use workspace_restore::apply_restored_sidebar_state;
pub use workspace_restore::{apply_restored_multiworkspace_state, restore_multiworkspace};
pub use workspace_settings::{
    AutosaveSetting, EncodingDisplayOptions, FocusFollowsMouse, RestoreOnStartupBehavior,
    SidebarSettings, StatusBarSettings, TabBarSettings, ToolbarSettings, WorkspaceSettings,
};
pub use workspace_store::WorkspaceStore;
pub use workspace_types::{
    ActiveWorktreeCreation, AutoWatch, CollaboratorId, OpenMode, PreviousWorkspaceState, ViewId,
};
pub use workspace_window_lookup::{
    activate_any_workspace_window, get_any_active_multi_workspace, workspace_windows_for_location,
};

pub(crate) fn workspace_card_gap(cx: &App) -> Pixels {
    gpui::px(WorkspaceSettings::get_global(cx).card_gap.max(0.0))
}

pub(crate) fn title_bar_visible(_cx: &App) -> bool {
    false
}

use crate::{dock::PanelSizeState, item::ItemBufferKind, notifications::NotificationId};
use crate::{
    persistence::{
        SerializedAxis,
        model::{SerializedItem, SerializedPane, SerializedPaneGroup},
    },
    security_modal::SecurityModal,
};

pub const SERIALIZATION_THROTTLE_TIME: Duration = Duration::from_millis(200);

pub fn init(app_state: Arc<AppState>, cx: &mut App) {
    component::init();
    theme_preview::init(cx);
    toast_layer::init(cx);
    history_manager::init(app_state.fs.clone(), cx);

    cx.on_action(|_: &CloseWindow, cx| Workspace::close_global(cx))
        .on_action(|_: &Reload, cx| reload(cx))
        .on_action(|action: &Open, cx: &mut App| {
            let app_state = AppState::global(cx);
            prompt_and_open_paths(
                app_state,
                PathPromptOptions {
                    files: true,
                    directories: true,
                    multiple: true,
                    prompt: None,
                },
                action.create_new_window.unwrap_or_else(|| {
                    matches!(
                        WorkspaceSettings::get_global(cx).default_open_behavior,
                        DefaultOpenBehavior::NewWindow
                    )
                }),
                cx,
            );
        })
        .on_action(|_: &OpenFiles, cx: &mut App| {
            let directories = cx.can_select_mixed_files_and_dirs();
            let app_state = AppState::global(cx);
            prompt_and_open_paths(
                app_state,
                PathPromptOptions {
                    files: true,
                    directories,
                    multiple: true,
                    prompt: None,
                },
                true,
                cx,
            );
        });
}

enum WorkspaceLocation {
    // Valid local paths or SSH project to serialize
    Location(SerializedWorkspaceLocation, PathList),
    // No valid location found hence clear session id
    DetachFromSession,
    // No valid location found to serialize
    None,
}

type PromptForNewPath = Box<
    dyn Fn(
        &mut Workspace,
        DirectoryLister,
        Option<String>,
        &mut Window,
        &mut Context<Workspace>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>>,
>;

type PromptForOpenPath = Box<
    dyn Fn(
        &mut Workspace,
        DirectoryLister,
        &mut Window,
        &mut Context<Workspace>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>>,
>;

/// Collects everything project-related for a certain window opened.
/// In some way, is a counterpart of a window, as the [`WindowHandle`] could be downcast into `Workspace`.
///
/// A `Workspace` usually consists of 1 or more projects, a central pane group, 3 docks and a status bar.
/// The `Workspace` owns everybody's state and serves as a default, "global context",
/// that can be used to register a global action to be triggered from any place in the window.
pub struct Workspace {
    weak_self: WeakEntity<Self>,
    workspace_actions: Vec<Box<dyn Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div>>,
    zoomed: Option<AnyWeakView>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    zoomed_position: Option<DockPosition>,
    center: PaneGroup,
    left_dock: Entity<Dock>,
    right_dock: Entity<Dock>,
    panes: Vec<Entity<Pane>>,
    panes_by_item: HashMap<EntityId, WeakEntity<Pane>>,
    active_pane: Entity<Pane>,
    last_active_center_pane: Option<WeakEntity<Pane>>,
    last_active_view_id: Option<proto::ViewId>,
    status_bar: Entity<StatusBar>,
    pub(crate) modal_layer: Entity<ModalLayer>,
    toast_layer: Entity<ToastLayer>,
    notifications: Notifications,
    suppressed_notifications: HashSet<NotificationId>,
    project: Entity<Project>,
    follower_states: HashMap<CollaboratorId, FollowerState>,
    last_leaders_by_pane: HashMap<WeakEntity<Pane>, CollaboratorId>,
    auto_watch: AutoWatch,
    window_edited: bool,
    last_window_title: Option<String>,
    dirty_items: HashMap<EntityId, Subscription>,
    active_call: Option<(GlobalAnyActiveCall, Vec<Subscription>)>,
    leader_updates_tx: mpsc::UnboundedSender<(PeerId, proto::UpdateFollowers)>,
    database_id: Option<WorkspaceId>,
    app_state: Arc<AppState>,
    dispatching_keystrokes: Rc<RefCell<DispatchingKeystrokes>>,
    _subscriptions: Vec<Subscription>,
    _apply_leader_updates: Task<Result<()>>,
    _observe_current_user: Task<Result<()>>,
    _schedule_serialize_workspace: Option<Task<()>>,
    _serialize_workspace_task: Option<Task<()>>,
    _schedule_serialize_ssh_paths: Option<Task<()>>,
    pane_history_timestamp: Arc<AtomicUsize>,
    bounds: Bounds<Pixels>,
    pub centered_layout: bool,
    bounds_save_task_queued: Option<Task<()>>,
    on_prompt_for_new_path: Option<PromptForNewPath>,
    on_prompt_for_open_path: Option<PromptForOpenPath>,
    terminal_provider: Option<Box<dyn TerminalProvider>>,
    debugger_provider: Option<Arc<dyn DebuggerProvider>>,
    serializable_items_tx: UnboundedSender<Box<dyn SerializableItemHandle>>,
    _items_serializer: Task<Result<()>>,
    session_id: Option<String>,
    scheduled_tasks: Vec<Task<()>>,
    removing: bool,
    open_in_dev_container: bool,
    _dev_container_task: Option<Task<Result<()>>>,
    _panels_task: Option<Task<Result<()>>>,
    sidebar_focus_handle: Option<FocusHandle>,
    multi_workspace: Option<WeakEntity<MultiWorkspace>>,
    /// Shared with the parent `MultiWorkspace` and any sibling workspaces: holds
    /// the id of the single workspace currently presented in this OS window.
    /// `MultiWorkspace` is the only writer; workspaces only read it to decide
    /// whether they may write the shared window's title and edited indicator. We
    /// use this instead of going through the `multi_workspace` field to avoid
    /// reading it as we might end up in a double lease otherwise.
    active_workspace_id: Option<Rc<Cell<EntityId>>>,
    active_worktree_creation: ActiveWorktreeCreation,
    deferred_save_items: Vec<Box<dyn WeakItemHandle>>,
}

impl EventEmitter<Event> for Workspace {}

pub struct FollowerState {
    center_pane: Entity<Pane>,
    dock_pane: Option<Entity<Pane>>,
    active_view_id: Option<ViewId>,
    items_by_leader_view_id: HashMap<ViewId, FollowerView>,
}

struct FollowerView {
    view: Box<dyn FollowableItemHandle>,
    location: Option<proto::PanelId>,
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

    use super::*;
    use crate::{
        dock::{PanelEvent, test::TestPanel},
        item::{
            ItemBufferKind, ItemEvent,
            test::{TestItem, TestProjectItem},
        },
    };
    use fs::FakeFs;
    use gpui::{
        DismissEvent, Empty, EventEmitter, FocusHandle, Focusable, Render, TestAppContext,
        UpdateGlobal, VisualTestContext, px,
    };
    use project::{Project, ProjectEntryId, WorktreeId};
    use serde_json::json;
    use settings::SettingsStore;
    use util::path;
    use util::rel_path::rel_path;

    mod part_01;
    mod part_02;
    mod part_03;
    mod part_04a;
    mod part_04b;
    mod part_05a;
    mod part_05b;
    mod part_06a;
    mod part_06b;
    mod part_07;
    mod part_08;
    mod part_09;
    mod part_10;
    mod part_11;
}
