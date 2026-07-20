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
mod workspace_actions;
mod workspace_call;
mod workspace_channel;
mod workspace_docks;
pub mod workspace_error;
mod workspace_event;
mod workspace_focus_targets;
mod workspace_follower_rpc;
mod workspace_following;
mod workspace_global_open;
mod workspace_id;
mod workspace_keystrokes;
mod workspace_leader_state;
mod workspace_lifecycle;
mod workspace_location_helpers;
mod workspace_navigation;
mod workspace_notifications;
mod workspace_open_items;
mod workspace_open_options;
mod workspace_open_prompt;
mod workspace_pane_layout;
mod workspace_panel_focus;
mod workspace_path_opening;
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
mod workspace_types;
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

pub struct AppState {
    pub languages: Arc<LanguageRegistry>,
    pub client: Arc<Client>,
    pub user_store: Entity<UserStore>,
    pub workspace_store: Entity<WorkspaceStore>,
    pub fs: Arc<dyn fs::Fs>,
    pub build_window_options: fn(Option<Uuid>, &mut App) -> WindowOptions,
    pub node_runtime: NodeRuntime,
    pub session: Entity<AppSession>,
}

struct GlobalAppState(Arc<AppState>);

impl Global for GlobalAppState {}

impl AppState {
    #[track_caller]
    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalAppState>().0.clone()
    }
    pub fn try_global(cx: &App) -> Option<Arc<Self>> {
        cx.try_global::<GlobalAppState>()
            .map(|state| state.0.clone())
    }
    pub fn set_global(state: Arc<AppState>, cx: &mut App) {
        cx.set_global(GlobalAppState(state));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(cx: &mut App) -> Arc<Self> {
        use fs::Fs;
        use node_runtime::NodeRuntime;
        use session::Session;
        use settings::SettingsStore;

        if !cx.has_global::<SettingsStore>() {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        }

        let fs = fs::FakeFs::new(cx.background_executor().clone());
        <dyn Fs>::set_global(fs.clone(), cx);
        let languages = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        let clock = Arc::new(clock::FakeSystemClock::new());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let client = Client::new(clock, http_client, cx);
        let session = cx.new(|cx| AppSession::new(Session::test(), cx));
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        let workspace_store = cx.new(|cx| WorkspaceStore::new(client.clone(), cx));

        theme_settings::init(theme::LoadThemes::JustBase, cx);
        client::init(&client, cx);

        Arc::new(Self {
            client,
            fs,
            languages,
            user_store,
            workspace_store,
            node_runtime: NodeRuntime::unavailable(),
            build_window_options: |_, _| Default::default(),
            session,
        })
    }
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

impl Workspace {
    pub fn new(
        workspace_id: Option<WorkspaceId>,
        project: Entity<Project>,
        app_state: Arc<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            cx.subscribe(&trusted_worktrees, |_, worktrees_store, e, cx| {
                if let TrustedWorktreesEvent::Trusted(..) = e {
                    // Do not persist auto trusted worktrees
                    if !ProjectSettings::get_global(cx).session.trust_all_worktrees {
                        worktrees_store.update(cx, |worktrees_store, cx| {
                            worktrees_store.schedule_serialization(
                                cx,
                                |new_trusted_worktrees, cx| {
                                    let timeout =
                                        cx.background_executor().timer(SERIALIZATION_THROTTLE_TIME);
                                    let db = WorkspaceDb::global(cx);
                                    cx.background_spawn(async move {
                                        timeout.await;
                                        db.save_trusted_worktrees(new_trusted_worktrees)
                                            .await
                                            .log_err();
                                    })
                                },
                            )
                        });
                    }
                }
            })
            .detach();

            cx.observe_global::<SettingsStore>(|_, cx| {
                if ProjectSettings::get_global(cx).session.trust_all_worktrees {
                    if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
                        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                            trusted_worktrees.auto_trust_all(cx);
                        })
                    }
                }
            })
            .detach();
        }

        cx.subscribe_in(&project, window, move |this, _, event, window, cx| {
            match event {
                project::Event::RemoteIdChanged(_) => {
                    this.update_window_title(window, cx);
                }

                project::Event::CollaboratorLeft(peer_id) => {
                    this.collaborator_left(*peer_id, window, cx);
                }

                &project::Event::WorktreeRemoved(_) => {
                    this.update_window_title(window, cx);
                    this.serialize_workspace(window, cx);
                    this.update_history(cx);
                }

                &project::Event::WorktreeAdded(id) => {
                    this.update_window_title(window, cx);
                    if this
                        .project()
                        .read(cx)
                        .worktree_for_id(id, cx)
                        .is_some_and(|wt| wt.read(cx).is_visible())
                    {
                        this.serialize_workspace(window, cx);
                        this.update_history(cx);
                    }
                }
                project::Event::WorktreeUpdatedEntries(..) => {
                    this.update_window_title(window, cx);
                    this.serialize_workspace(window, cx);
                }

                project::Event::DisconnectedFromHost => {
                    this.update_window_edited(window, cx);
                    let leaders_to_unfollow =
                        this.follower_states.keys().copied().collect::<Vec<_>>();
                    for leader_id in leaders_to_unfollow {
                        this.unfollow(leader_id, window, cx);
                    }
                }

                project::Event::DisconnectedFromRemote {
                    server_not_running: _,
                } => {
                    this.update_window_edited(window, cx);
                }

                project::Event::Closed => {
                    window.remove_window();
                }

                project::Event::DeletedEntry(_, entry_id) => {
                    for pane in this.panes.iter() {
                        pane.update(cx, |pane, cx| {
                            pane.handle_deleted_project_item(*entry_id, window, cx)
                        });
                    }
                }

                project::Event::Toast {
                    notification_id,
                    message,
                    link,
                } => this.show_notification(
                    NotificationId::named(notification_id.clone()),
                    cx,
                    |cx| {
                        let mut notification = MessageNotification::new(message.clone(), cx);
                        if let Some(link) = link {
                            notification = notification
                                .more_info_message(link.label)
                                .more_info_url(link.url);
                        }

                        cx.new(|_| notification)
                    },
                ),

                project::Event::HideToast { notification_id } => {
                    this.dismiss_notification(&NotificationId::named(notification_id.clone()), cx)
                }

                project::Event::LanguageServerPrompt(request) => {
                    struct LanguageServerPrompt;

                    this.show_notification(
                        NotificationId::composite::<LanguageServerPrompt>(request.id),
                        cx,
                        |cx| {
                            cx.new(|cx| {
                                notifications::LanguageServerPrompt::new(request.clone(), cx)
                            })
                        },
                    );
                }

                project::Event::AgentLocationChanged => {
                    this.handle_agent_location_changed(window, cx)
                }

                _ => {}
            }
            cx.notify()
        })
        .detach();

        cx.subscribe_in(
            &project.read(cx).breakpoint_store(),
            window,
            |workspace, _, event, window, cx| match event {
                BreakpointStoreEvent::BreakpointsUpdated(_, _)
                | BreakpointStoreEvent::BreakpointsCleared(_) => {
                    workspace.serialize_workspace(window, cx);
                }
                BreakpointStoreEvent::SetDebugLine | BreakpointStoreEvent::ClearDebugLines => {}
            },
        )
        .detach();
        if let Some(toolchain_store) = project.read(cx).toolchain_store() {
            cx.subscribe_in(
                &toolchain_store,
                window,
                |workspace, _, event, window, cx| match event {
                    ToolchainStoreEvent::CustomToolchainsModified => {
                        workspace.serialize_workspace(window, cx);
                    }
                    _ => {}
                },
            )
            .detach();
        }

        cx.on_focus_lost(window, |this, window, cx| {
            let focus_handle = this.focus_handle(cx);
            window.focus(&focus_handle, cx);
        })
        .detach();

        let weak_handle = cx.entity().downgrade();
        let pane_history_timestamp = Arc::new(AtomicUsize::new(0));

        let center_pane = cx.new(|cx| {
            let mut center_pane = Pane::new(
                weak_handle.clone(),
                project.clone(),
                pane_history_timestamp.clone(),
                None,
                NewFile.boxed_clone(),
                true,
                window,
                cx,
            );
            center_pane.set_can_split(Some(Arc::new(|_, _, _, _| true)));
            center_pane.set_should_display_welcome_page(true);
            center_pane
        });
        cx.subscribe_in(&center_pane, window, Self::handle_pane_event)
            .detach();

        window.focus(&center_pane.focus_handle(cx), cx);

        cx.emit(Event::PaneAdded(center_pane.clone()));

        let any_window_handle = window.window_handle();
        app_state.workspace_store.update(cx, |store, _| {
            store
                .workspaces
                .insert((any_window_handle, weak_handle.clone()));
        });

        let mut current_user = app_state.user_store.read(cx).watch_current_user();
        let mut connection_status = app_state.client.status();
        let _observe_current_user = cx.spawn_in(window, async move |this, cx| {
            current_user.next().await;
            connection_status.next().await;
            let mut stream =
                Stream::map(current_user, drop).merge(Stream::map(connection_status, drop));

            while stream.recv().await.is_some() {
                this.update(cx, |_, cx| cx.notify())?;
            }
            anyhow::Ok(())
        });

        // All leader updates are enqueued and then processed in a single task, so
        // that each asynchronous operation can be run in order.
        let (leader_updates_tx, mut leader_updates_rx) =
            mpsc::unbounded::<(PeerId, proto::UpdateFollowers)>();
        let _apply_leader_updates = cx.spawn_in(window, async move |this, cx| {
            while let Some((leader_id, update)) = leader_updates_rx.next().await {
                Self::process_leader_update(&this, leader_id, update, cx)
                    .await
                    .log_err();
            }

            Ok(())
        });

        cx.emit(Event::WorkspaceCreated(weak_handle.clone()));
        let modal_layer = cx.new(|_| ModalLayer::new());
        let toast_layer = cx.new(|_| ToastLayer::new());
        cx.subscribe(
            &modal_layer,
            |_, _, _: &modal_layer::ModalOpenedEvent, cx| {
                cx.emit(Event::ModalOpened);
            },
        )
        .detach();

        let left_dock = Dock::new(DockPosition::Left, modal_layer.clone(), window, cx);
        let right_dock = Dock::new(DockPosition::Right, modal_layer.clone(), window, cx);
        let multi_workspace = window
            .root::<MultiWorkspace>()
            .flatten()
            .map(|mw| mw.downgrade());
        let status_bar =
            cx.new(|cx| StatusBar::new(&center_pane.clone(), multi_workspace.clone(), window, cx));

        let session_id = app_state.session.read(cx).id().to_owned();

        let mut active_call = None;
        if let Some(call) = GlobalAnyActiveCall::try_global(cx).cloned() {
            let subscriptions =
                vec![
                    call.0
                        .subscribe(window, cx, Box::new(Self::on_active_call_event)),
                ];
            active_call = Some((call, subscriptions));
        }

        let (serializable_items_tx, serializable_items_rx) =
            mpsc::unbounded::<Box<dyn SerializableItemHandle>>();
        let _items_serializer = cx.spawn_in(window, async move |this, cx| {
            Self::serialize_items(&this, serializable_items_rx, cx).await
        });

        let subscriptions = vec![
            cx.observe_window_activation(window, Self::on_window_activation_changed),
            cx.observe_window_bounds(window, move |this, window, cx| {
                if this.bounds_save_task_queued.is_some() {
                    return;
                }
                this.bounds_save_task_queued = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    this.update_in(cx, |this, window, cx| {
                        this.save_window_bounds(window, cx).detach();
                        this.bounds_save_task_queued.take();
                    })
                    .ok();
                }));
                cx.notify();
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();

                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());

                theme_settings::reload_theme(cx);
                theme_settings::reload_icon_theme(cx);
            }),
            cx.on_release({
                let weak_handle = weak_handle.clone();
                move |this, cx| {
                    this.app_state.workspace_store.update(cx, move |store, _| {
                        store.workspaces.retain(|(_, weak)| weak != &weak_handle);
                    })
                }
            }),
        ];

        cx.defer_in(window, move |this, window, cx| {
            this.update_window_title(window, cx);
            this.show_initial_notifications(cx);
        });

        let mut center = PaneGroup::new(center_pane.clone());
        center.set_is_center(true);
        center.mark_positions(cx);

        Workspace {
            weak_self: weak_handle.clone(),
            zoomed: None,
            zoomed_position: None,
            previous_dock_drag_coordinates: None,
            center,
            panes: vec![center_pane.clone()],
            panes_by_item: Default::default(),
            active_pane: center_pane.clone(),
            last_active_center_pane: Some(center_pane.downgrade()),
            last_active_view_id: None,
            status_bar,
            modal_layer,
            toast_layer,
            notifications: Notifications::default(),
            suppressed_notifications: HashSet::default(),
            left_dock,
            right_dock,
            _panels_task: None,
            project: project.clone(),
            follower_states: Default::default(),
            last_leaders_by_pane: Default::default(),
            auto_watch: AutoWatch::Off,
            dispatching_keystrokes: Default::default(),
            window_edited: false,
            last_window_title: None,
            dirty_items: Default::default(),
            active_call,
            database_id: workspace_id,
            app_state,
            _observe_current_user,
            _apply_leader_updates,
            _schedule_serialize_workspace: None,
            _serialize_workspace_task: None,
            _schedule_serialize_ssh_paths: None,
            leader_updates_tx,
            _subscriptions: subscriptions,
            pane_history_timestamp,
            workspace_actions: Default::default(),
            // This data will be incorrect, but it will be overwritten by the time it needs to be used.
            bounds: Default::default(),
            centered_layout: false,
            bounds_save_task_queued: None,
            on_prompt_for_new_path: None,
            on_prompt_for_open_path: None,
            terminal_provider: None,
            debugger_provider: None,
            serializable_items_tx,
            _items_serializer,
            session_id: Some(session_id),

            scheduled_tasks: Vec::new(),
            removing: false,
            sidebar_focus_handle: None,
            multi_workspace,
            active_workspace_id: None,
            active_worktree_creation: ActiveWorktreeCreation::default(),
            open_in_dev_container: false,
            _dev_container_task: None,
            deferred_save_items: Vec::new(),
        }
    }

    pub fn new_local(
        abs_paths: Vec<PathBuf>,
        app_state: Arc<AppState>,
        requesting_window: Option<WindowHandle<MultiWorkspace>>,
        env: Option<HashMap<String, String>>,
        init: Option<Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send>>,
        open_mode: OpenMode,
        cx: &mut App,
    ) -> Task<anyhow::Result<OpenResult>> {
        let project_handle = Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            env,
            Default::default(),
            cx,
        );

        let db = WorkspaceDb::global(cx);
        let kvp = db::kvp::KeyValueStore::global(cx);
        cx.spawn(async move |cx| {
            let mut paths_to_open = Vec::with_capacity(abs_paths.len());
            for path in abs_paths.into_iter() {
                if let Some(canonical) = app_state.fs.canonicalize(&path).await.ok() {
                    paths_to_open.push(canonical)
                } else {
                    paths_to_open.push(path)
                }
            }

            let serialized_workspace = db.workspace_for_roots(paths_to_open.as_slice());
            let restored_multi_workspace_state = if open_mode != OpenMode::Add
                && let Some(window_id) = serialized_workspace.as_ref().and_then(|ws| ws.window_id)
            {
                cx.update(move |cx| {
                    persistence::read_multi_workspace_state_if_present(
                        WindowId::from(window_id),
                        cx,
                    )
                })
            } else {
                None
            };
            let initial_sidebar_open = restored_multi_workspace_state
                .as_ref()
                .map(|state| state.sidebar_open);

            if let Some(paths) = serialized_workspace.as_ref().map(|ws| &ws.paths) {
                paths_to_open = paths.ordered_paths().cloned().collect();
            }

            // Get project paths for all of the abs_paths
            let mut project_paths: Vec<(PathBuf, Option<ProjectPath>)> =
                Vec::with_capacity(paths_to_open.len());

            for path in paths_to_open.into_iter() {
                if let Some((_, project_entry)) = cx
                    .update(|cx| {
                        Workspace::project_path_for_path(project_handle.clone(), &path, true, cx)
                    })
                    .await
                    .log_err()
                {
                    project_paths.push((path, Some(project_entry)));
                } else {
                    project_paths.push((path, None));
                }
            }

            let workspace_id = if let Some(serialized_workspace) = serialized_workspace.as_ref() {
                serialized_workspace.id
            } else {
                db.next_id().await.unwrap_or_else(|_| Default::default())
            };

            let toolchains = db.toolchains(workspace_id).await?;

            for (toolchain, worktree_path, path) in toolchains {
                let toolchain_path = PathBuf::from(toolchain.path.clone().to_string());
                let Some(worktree_id) = project_handle.read_with(cx, |this, cx| {
                    this.find_worktree(&worktree_path, cx)
                        .and_then(|(worktree, rel_path)| {
                            if rel_path.is_empty() {
                                Some(worktree.read(cx).id())
                            } else {
                                None
                            }
                        })
                }) else {
                    // We did not find a worktree with a given path, but that's whatever.
                    continue;
                };
                if !app_state.fs.is_file(toolchain_path.as_path()).await {
                    continue;
                }

                project_handle
                    .update(cx, |this, cx| {
                        this.activate_toolchain(ProjectPath { worktree_id, path }, toolchain, cx)
                    })
                    .await;
            }
            if let Some(workspace) = serialized_workspace.as_ref() {
                project_handle.update(cx, |this, cx| {
                    for (scope, toolchains) in &workspace.user_toolchains {
                        for toolchain in toolchains {
                            this.add_toolchain(toolchain.clone(), scope.clone(), cx);
                        }
                    }
                });
            }

            let window_to_replace = match open_mode {
                OpenMode::NewWindow => None,
                _ => requesting_window,
            };

            let (window, workspace): (WindowHandle<MultiWorkspace>, Entity<Workspace>) =
                if let Some(window) = window_to_replace {
                    let centered_layout = serialized_workspace
                        .as_ref()
                        .map(|w| w.centered_layout)
                        .unwrap_or(false);

                    let workspace = window.update(cx, |multi_workspace, window, cx| {
                        let workspace = cx.new(|cx| {
                            let mut workspace = Workspace::new(
                                Some(workspace_id),
                                project_handle.clone(),
                                app_state.clone(),
                                window,
                                cx,
                            );

                            workspace.centered_layout = centered_layout;

                            // Call init callback to add items before window renders
                            if let Some(init) = init {
                                init(&mut workspace, window, cx);
                            }

                            workspace
                        });
                        match open_mode {
                            OpenMode::Activate => {
                                multi_workspace.activate(workspace.clone(), None, window, cx);
                            }
                            OpenMode::Add => {
                                multi_workspace.add(workspace.clone(), &*window, cx);
                            }
                            OpenMode::NewWindow => {
                                unreachable!()
                            }
                        }
                        workspace
                    })?;
                    (window, workspace)
                } else {
                    let window_bounds_override = window_bounds_env_override();

                    let (window_bounds, display) = if let Some(bounds) = window_bounds_override {
                        (Some(WindowBounds::Windowed(bounds)), None)
                    } else if let Some(workspace) = serialized_workspace.as_ref()
                        && let Some(display) = workspace.display
                        && let Some(bounds) = workspace.window_bounds.as_ref()
                    {
                        // Reopening an existing workspace - restore its saved bounds
                        (Some(bounds.0), Some(display))
                    } else if let Some((display, bounds)) =
                        persistence::read_default_window_bounds(&kvp)
                    {
                        // New or empty workspace - use the last known window bounds
                        (Some(bounds), Some(display))
                    } else {
                        // New window - let GPUI's default_bounds() handle cascading
                        (None, None)
                    };

                    // Use the serialized workspace to construct the new window
                    let mut options = cx.update(|cx| (app_state.build_window_options)(display, cx));
                    options.window_bounds = window_bounds;
                    let centered_layout = serialized_workspace
                        .as_ref()
                        .map(|w| w.centered_layout)
                        .unwrap_or(false);
                    let window = cx.open_window(options, {
                        let app_state = app_state.clone();
                        let project_handle = project_handle.clone();
                        move |window, cx| {
                            let workspace = cx.new(|cx| {
                                let mut workspace = Workspace::new(
                                    Some(workspace_id),
                                    project_handle,
                                    app_state,
                                    window,
                                    cx,
                                );
                                workspace.centered_layout = centered_layout;

                                // Call init callback to add items before window renders
                                if let Some(init) = init {
                                    init(&mut workspace, window, cx);
                                }

                                workspace
                            });
                            cx.new(|cx| {
                                if let Some(sidebar_open) = initial_sidebar_open {
                                    MultiWorkspace::new_with_initial_sidebar_open(
                                        workspace,
                                        window,
                                        cx,
                                        sidebar_open,
                                    )
                                } else {
                                    MultiWorkspace::new(workspace, window, cx)
                                }
                            })
                        }
                    })?;
                    let workspace =
                        window.update(cx, |multi_workspace: &mut MultiWorkspace, _, _cx| {
                            multi_workspace.workspace().clone()
                        })?;
                    (window, workspace)
                };

            if let Some(state) = &restored_multi_workspace_state {
                apply_restored_sidebar_state(window, state, cx);
            }

            notify_if_database_failed(window, cx);
            // Check if this is an empty workspace (no paths to open)
            // An empty workspace is one where project_paths is empty
            let is_empty_workspace = project_paths.is_empty();
            // Check if serialized workspace has paths before it's moved
            let serialized_workspace_has_paths = serialized_workspace
                .as_ref()
                .map(|ws| !ws.paths.is_empty())
                .unwrap_or(false);

            let opened_items = window
                .update(cx, |_, window, cx| {
                    workspace.update(cx, |_workspace: &mut Workspace, cx| {
                        open_items(serialized_workspace, project_paths, window, cx)
                    })
                })?
                .await
                .unwrap_or_default();

            // Restore default dock state for empty workspaces
            // Only restore if:
            // 1. This is an empty workspace (no paths), AND
            // 2. The serialized workspace either doesn't exist or has no paths
            if is_empty_workspace && !serialized_workspace_has_paths {
                if let Some(default_docks) = persistence::read_default_dock_state(&kvp) {
                    window
                        .update(cx, |_, window, cx| {
                            workspace.update(cx, |workspace, cx| {
                                for (dock, serialized_dock) in [
                                    (&workspace.right_dock, &default_docks.right),
                                    (&workspace.left_dock, &default_docks.left),
                                ] {
                                    dock.update(cx, |dock, cx| {
                                        dock.serialized_dock = Some(serialized_dock.clone());
                                        dock.restore_state(window, cx);
                                    });
                                }
                                cx.notify();
                            });
                        })
                        .log_err();
                }
            }

            window
                .update(cx, |_, _window, cx| {
                    workspace.update(cx, |this: &mut Workspace, cx| {
                        this.update_history(cx);
                    });
                })
                .log_err();

            if open_mode == OpenMode::NewWindow || open_mode == OpenMode::Activate {
                window
                    .update(cx, |_, window, _cx| {
                        window.activate_window();
                    })
                    .log_err();
            }

            // Auto-show the security modal if the project has restricted worktrees
            window
                .update(cx, |_, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.show_worktree_trust_security_modal(false, window, cx);
                    });
                })
                .log_err();

            Ok(OpenResult {
                window,
                workspace,
                opened_items,
            })
        })
    }

    pub fn status_bar(&self) -> &Entity<StatusBar> {
        &self.status_bar
    }

    pub fn set_sidebar_focus_handle(&mut self, handle: Option<FocusHandle>) {
        self.sidebar_focus_handle = handle;
    }

    pub fn notify_panes(&self, cx: &mut App) {
        for pane in &self.panes {
            cx.notify(pane.entity_id());
        }
    }

    pub fn status_bar_visible(&self, cx: &App) -> bool {
        StatusBarSettings::get_global(cx).show
    }

    pub fn multi_workspace(&self) -> Option<&WeakEntity<MultiWorkspace>> {
        self.multi_workspace.as_ref()
    }

    pub fn set_multi_workspace(
        &mut self,
        multi_workspace: WeakEntity<MultiWorkspace>,
        active_workspace_id: Rc<Cell<EntityId>>,
        cx: &mut App,
    ) {
        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_multi_workspace(multi_workspace.clone(), cx);
        });
        self.multi_workspace = Some(multi_workspace);
        self.active_workspace_id = Some(active_workspace_id);
    }

    pub fn app_state(&self) -> &Arc<AppState> {
        &self.app_state
    }

    pub fn set_panels_task(&mut self, task: Task<Result<()>>) {
        self._panels_task = Some(task);
    }

    pub fn take_panels_task(&mut self) -> Option<Task<Result<()>>> {
        self._panels_task.take()
    }

    pub fn user_store(&self) -> &Entity<UserStore> {
        &self.app_state.user_store
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn path_style(&self, cx: &App) -> PathStyle {
        self.project.read(cx).path_style(cx)
    }

    pub fn recently_activated_items(&self, cx: &App) -> HashMap<EntityId, usize> {
        let mut history: HashMap<EntityId, usize> = HashMap::default();

        for pane_handle in &self.panes {
            let pane = pane_handle.read(cx);

            for entry in pane.activation_history() {
                history.insert(
                    entry.entity_id,
                    history
                        .get(&entry.entity_id)
                        .cloned()
                        .unwrap_or(0)
                        .max(entry.timestamp),
                );
            }
        }

        history
    }

    pub fn client(&self) -> &Arc<Client> {
        &self.app_state.client
    }

    pub fn set_prompt_for_new_path(&mut self, prompt: PromptForNewPath) {
        self.on_prompt_for_new_path = Some(prompt)
    }

    pub fn set_prompt_for_open_path(&mut self, prompt: PromptForOpenPath) {
        self.on_prompt_for_open_path = Some(prompt)
    }

    pub fn set_terminal_provider(&mut self, provider: impl TerminalProvider + 'static) {
        self.terminal_provider = Some(Box::new(provider));
    }

    pub fn set_debugger_provider(&mut self, provider: impl DebuggerProvider + 'static) {
        self.debugger_provider = Some(Arc::new(provider));
    }

    pub fn set_open_in_dev_container(&mut self, value: bool) {
        self.open_in_dev_container = value;
    }

    pub fn open_in_dev_container(&self) -> bool {
        self.open_in_dev_container
    }

    pub fn set_dev_container_task(&mut self, task: Task<Result<()>>) {
        self._dev_container_task = Some(task);
    }

    pub fn debugger_provider(&self) -> Option<Arc<dyn DebuggerProvider>> {
        self.debugger_provider.clone()
    }

    pub fn prompt_for_open_path(
        &mut self,
        path_prompt_options: PathPromptOptions,
        lister: DirectoryLister,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>> {
        // TODO: If `on_prompt_for_open_path` is set, we should always use it
        // rather than gating on `use_system_path_prompts`. This would let tests
        // inject a mock without also having to disable the setting.
        if !lister.is_local(cx) || !WorkspaceSettings::get_global(cx).use_system_path_prompts {
            let prompt = self.on_prompt_for_open_path.take().unwrap();
            let rx = prompt(self, lister, window, cx);
            self.on_prompt_for_open_path = Some(prompt);
            rx
        } else {
            let (tx, rx) = oneshot::channel();
            let abs_path = cx.prompt_for_paths(path_prompt_options);

            cx.spawn_in(window, async move |workspace, cx| {
                let Ok(result) = abs_path.await else {
                    return Ok(());
                };

                match result {
                    Ok(result) => {
                        tx.send(result).ok();
                    }
                    Err(err) => {
                        let rx = workspace.update_in(cx, |workspace, window, cx| {
                            workspace
                                .show_error(workspace_error::PortalError::new(err.to_string()), cx);
                            let prompt = workspace.on_prompt_for_open_path.take().unwrap();
                            let rx = prompt(workspace, lister, window, cx);
                            workspace.on_prompt_for_open_path = Some(prompt);
                            rx
                        })?;
                        if let Ok(path) = rx.await {
                            tx.send(path).ok();
                        }
                    }
                };
                anyhow::Ok(())
            })
            .detach();

            rx
        }
    }

    pub fn prompt_for_new_path(
        &mut self,
        lister: DirectoryLister,
        suggested_name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<Vec<PathBuf>>> {
        if self.project.read(cx).is_via_collab()
            || self.project.read(cx).is_via_remote_server()
            || !WorkspaceSettings::get_global(cx).use_system_path_prompts
        {
            let prompt = self.on_prompt_for_new_path.take().unwrap();
            let rx = prompt(self, lister, suggested_name, window, cx);
            self.on_prompt_for_new_path = Some(prompt);
            return rx;
        }

        let (tx, rx) = oneshot::channel();
        cx.spawn_in(window, async move |workspace, cx| {
            let abs_path = workspace.update(cx, |workspace, cx| {
                let relative_to = workspace
                    .most_recent_active_path(cx)
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .or_else(|| {
                        let project = workspace.project.read(cx);
                        project.visible_worktrees(cx).find_map(|worktree| {
                            Some(worktree.read(cx).as_local()?.abs_path().to_path_buf())
                        })
                    })
                    .or_else(std::env::home_dir)
                    .unwrap_or_else(|| PathBuf::from(""));
                cx.prompt_for_new_path(&relative_to, suggested_name.as_deref())
            })?;
            let abs_path = match abs_path.await? {
                Ok(path) => path,
                Err(err) => {
                    let rx = workspace.update_in(cx, |workspace, window, cx| {
                        workspace
                            .show_error(workspace_error::PortalError::new(err.to_string()), cx);

                        let prompt = workspace.on_prompt_for_new_path.take().unwrap();
                        let rx = prompt(workspace, lister, suggested_name, window, cx);
                        workspace.on_prompt_for_new_path = Some(prompt);
                        rx
                    })?;
                    if let Ok(path) = rx.await {
                        tx.send(path).ok();
                    }
                    return anyhow::Ok(());
                }
            };

            tx.send(abs_path.map(|path| vec![path])).ok();
            anyhow::Ok(())
        })
        .detach();

        rx
    }

    /// Call the given callback with a workspace whose project is local or remote via WSL (allowing host access).
    ///
    /// If the given workspace has a local project, then it will be passed
    /// to the callback. Otherwise, a new empty window will be created.
    pub fn with_local_workspace<T, F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        callback: F,
    ) -> Task<Result<T>>
    where
        T: 'static,
        F: 'static + FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> T,
    {
        if self.project.read(cx).is_local() {
            Task::ready(Ok(callback(self, window, cx)))
        } else {
            let env = self.project.read(cx).cli_environment(cx);
            let task = Self::new_local(
                Vec::new(),
                self.app_state.clone(),
                None,
                env,
                None,
                OpenMode::Activate,
                cx,
            );
            cx.spawn_in(window, async move |_vh, cx| {
                let OpenResult {
                    window: multi_workspace_window,
                    ..
                } = task.await?;
                multi_workspace_window.update(cx, |multi_workspace, window, cx| {
                    let workspace = multi_workspace.workspace().clone();
                    workspace.update(cx, |workspace, cx| callback(workspace, window, cx))
                })
            })
        }
    }

    /// Call the given callback with a workspace whose project is local or remote via WSL (allowing host access).
    ///
    /// If the given workspace has a local project, then it will be passed
    /// to the callback. Otherwise, a new empty window will be created.
    pub fn with_local_or_wsl_workspace<T, F>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        callback: F,
    ) -> Task<Result<T>>
    where
        T: 'static,
        F: 'static + FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> T,
    {
        let project = self.project.read(cx);
        if project.is_local() || project.is_via_wsl_with_host_interop(cx) {
            Task::ready(Ok(callback(self, window, cx)))
        } else {
            let env = self.project.read(cx).cli_environment(cx);
            let task = Self::new_local(
                Vec::new(),
                self.app_state.clone(),
                None,
                env,
                None,
                OpenMode::Activate,
                cx,
            );
            cx.spawn_in(window, async move |_vh, cx| {
                let OpenResult {
                    window: multi_workspace_window,
                    ..
                } = task.await?;
                multi_workspace_window.update(cx, |multi_workspace, window, cx| {
                    let workspace = multi_workspace.workspace().clone();
                    workspace.update(cx, |workspace, cx| callback(workspace, window, cx))
                })
            })
        }
    }

    pub fn worktrees<'a>(&self, cx: &'a App) -> impl 'a + Iterator<Item = Entity<Worktree>> {
        self.project.read(cx).worktrees(cx)
    }

    pub fn visible_worktrees<'a>(
        &self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = Entity<Worktree>> {
        self.project.read(cx).visible_worktrees(cx)
    }

    pub fn worktree_scans_complete(&self, cx: &App) -> impl Future<Output = ()> + 'static + use<> {
        let futures = self
            .worktrees(cx)
            .filter_map(|worktree| worktree.read(cx).as_local())
            .map(|worktree| worktree.scan_complete())
            .collect::<Vec<_>>();
        async move {
            for future in futures {
                future.await;
            }
        }
    }

    pub fn items<'a>(&'a self, cx: &'a App) -> impl 'a + Iterator<Item = &'a Box<dyn ItemHandle>> {
        self.panes.iter().flat_map(|pane| pane.read(cx).items())
    }

    pub fn item_of_type<T: Item>(&self, cx: &App) -> Option<Entity<T>> {
        self.items_of_type(cx).max_by_key(|item| item.item_id())
    }

    pub fn items_of_type<'a, T: Item>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = Entity<T>> {
        self.panes
            .iter()
            .flat_map(|pane| pane.read(cx).items_of_type())
    }

    pub fn active_item(&self, cx: &App) -> Option<Box<dyn ItemHandle>> {
        self.active_pane().read(cx).active_item()
    }

    pub fn active_item_as<I: 'static>(&self, cx: &App) -> Option<Entity<I>> {
        let item = self.active_item(cx)?;
        item.to_any_view().downcast::<I>().ok()
    }

    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.active_item(cx).and_then(|item| item.project_path(cx))
    }

    pub fn most_recent_active_path(&self, cx: &App) -> Option<PathBuf> {
        self.recent_navigation_history_iter(cx)
            .filter_map(|(path, abs_path)| {
                let worktree = self
                    .project
                    .read(cx)
                    .worktree_for_id(path.worktree_id, cx)?;
                if !worktree.read(cx).is_visible() {
                    return None;
                }
                let settings_location = SettingsLocation {
                    worktree_id: path.worktree_id,
                    path: &path.path,
                };
                if WorktreeSettings::get(Some(settings_location), cx).is_path_read_only(&path.path)
                {
                    return None;
                }
                abs_path
            })
            .next()
    }

    pub fn save_active_item(
        &mut self,
        save_intent: SaveIntent,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let project = self.project.clone();
        let pane = self.active_pane();
        let item = pane.read(cx).active_item();
        let pane = pane.downgrade();

        window.spawn(cx, async move |cx| {
            if let Some(item) = item {
                Pane::save_item(project, &pane, item.as_ref(), save_intent, cx)
                    .await
                    .map(|_| ())
            } else {
                Ok(())
            }
        })
    }

    pub fn close_inactive_items_and_panes(
        &mut self,
        action: &CloseInactiveTabsAndPanes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.close_all_internal(
            true,
            action.save_intent.unwrap_or(SaveIntent::Close),
            window,
            cx,
        ) {
            task.detach_and_log_err(cx)
        }
    }

    pub fn close_all_items_and_panes(
        &mut self,
        action: &CloseAllItemsAndPanes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.close_all_internal(
            false,
            action.save_intent.unwrap_or(SaveIntent::Close),
            window,
            cx,
        ) {
            task.detach_and_log_err(cx)
        }
    }

    /// Closes the active item across all panes.
    pub fn close_item_in_all_panes(
        &mut self,
        action: &CloseItemInAllPanes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_item) = self.active_pane().read(cx).active_item() else {
            return;
        };

        let save_intent = action.save_intent.unwrap_or(SaveIntent::Close);
        let close_pinned = action.close_pinned;

        if let Some(project_path) = active_item.project_path(cx) {
            self.close_items_with_project_path(
                &project_path,
                save_intent,
                close_pinned,
                window,
                cx,
            );
        } else if close_pinned || !self.active_pane().read(cx).is_active_item_pinned() {
            let item_id = active_item.item_id();
            self.active_pane().update(cx, |pane, cx| {
                pane.close_item_by_id(item_id, save_intent, window, cx)
                    .detach_and_log_err(cx);
            });
        }
    }

    /// Closes all items with the given project path across all panes.
    pub fn close_items_with_project_path(
        &mut self,
        project_path: &ProjectPath,
        save_intent: SaveIntent,
        close_pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panes = self.panes().to_vec();
        for pane in panes {
            pane.update(cx, |pane, cx| {
                pane.close_items_for_project_path(
                    project_path,
                    save_intent,
                    close_pinned,
                    window,
                    cx,
                )
                .detach_and_log_err(cx);
            });
        }
    }

    fn close_all_internal(
        &mut self,
        retain_active_pane: bool,
        save_intent: SaveIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let current_pane = self.active_pane();

        let mut tasks = Vec::new();

        if retain_active_pane {
            let current_pane_close = current_pane.update(cx, |pane, cx| {
                pane.close_other_items(
                    &CloseOtherItems {
                        save_intent: None,
                        close_pinned: false,
                    },
                    None,
                    window,
                    cx,
                )
            });

            tasks.push(current_pane_close);
        }

        for pane in self.panes() {
            if retain_active_pane && pane.entity_id() == current_pane.entity_id() {
                continue;
            }

            let close_pane_items = pane.update(cx, |pane: &mut Pane, cx| {
                pane.close_all_items(
                    &CloseAllItems {
                        save_intent: Some(save_intent),
                        close_pinned: false,
                    },
                    window,
                    cx,
                )
            });

            tasks.push(close_pane_items)
        }

        if tasks.is_empty() {
            None
        } else {
            Some(cx.spawn_in(window, async move |_, _| {
                for task in tasks {
                    task.await?
                }
                Ok(())
            }))
        }
    }

    pub fn add_item_to_center(
        &mut self,
        item: Box<dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(center_pane) = self.last_tabbed_pane(cx) {
            center_pane.update(cx, |pane, cx| {
                pane.add_item(item, true, true, None, window, cx)
            });
            true
        } else {
            let center_pane = self.ensure_tabbed_pane(window, cx);
            center_pane.update(cx, |pane, cx| {
                pane.add_item(item, true, true, None, window, cx)
            });
            true
        }
    }

    pub fn add_item_to_active_pane(
        &mut self,
        item: Box<dyn ItemHandle>,
        destination_index: Option<usize>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = if self.active_pane.read(cx).is_tabbed() {
            self.active_pane.clone()
        } else {
            self.last_tabbed_pane(cx)
                .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx))
        };
        self.add_item(pane, item, destination_index, false, focus_item, window, cx)
    }

    pub fn add_item(
        &mut self,
        pane: Entity<Pane>,
        item: Box<dyn ItemHandle>,
        destination_index: Option<usize>,
        activate_pane: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_panel_item = item.downcast::<PanelItem>().is_some();
        let pane = if pane.read(cx).is_tabbed() || is_panel_item {
            pane
        } else {
            self.ensure_tabbed_pane(window, cx)
        };

        pane.update(cx, |pane, cx| {
            pane.add_item(
                item,
                activate_pane,
                focus_item,
                destination_index,
                window,
                cx,
            )
        });
    }

    pub fn split_item(
        &mut self,
        split_direction: SplitDirection,
        item: Box<dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_pane = self.split_pane(self.active_pane.clone(), split_direction, window, cx);
        self.add_item(new_pane, item, None, true, true, window, cx);
    }

    pub fn open_abs_path(
        &mut self,
        abs_path: PathBuf,
        options: OpenOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn_in(window, async move |workspace, cx| {
            let open_paths_task_result = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_paths(vec![abs_path.clone()], options, None, window, cx)
                })
                .with_context(|| format!("open abs path {abs_path:?} task spawn"))?
                .await;
            anyhow::ensure!(
                open_paths_task_result.len() == 1,
                "open abs path {abs_path:?} task returned incorrect number of results"
            );
            match open_paths_task_result
                .into_iter()
                .next()
                .expect("ensured single task result")
            {
                Some(open_result) => {
                    open_result.with_context(|| format!("open abs path {abs_path:?} task join"))
                }
                None => anyhow::bail!("open abs path {abs_path:?} task returned None"),
            }
        })
    }

    pub fn split_abs_path(
        &mut self,
        abs_path: PathBuf,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let project_path_task =
            Workspace::project_path_for_path(self.project.clone(), &abs_path, visible, cx);
        cx.spawn_in(window, async move |this, cx| {
            let (_, path) = project_path_task.await?;
            this.update_in(cx, |this, window, cx| this.split_path(path, window, cx))?
                .await
        })
    }

    pub fn open_path(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.open_path_preview(path, pane, focus_item, false, true, window, cx)
    }

    pub fn open_path_in_tabbed_pane(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.open_path_preview_in_tabbed_pane(path, pane, focus_item, false, true, window, cx)
    }

    pub fn open_path_preview(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let Some(pane) = self.existing_tabbed_pane(pane, cx) else {
            return Task::ready(Err(anyhow!("no tabbed pane available")));
        };

        self.open_path_preview_in_pane(
            path.into(),
            pane.downgrade(),
            focus_item,
            allow_preview,
            activate,
            window,
            cx,
        )
    }

    pub fn open_path_preview_in_tabbed_pane(
        &mut self,
        path: impl Into<ProjectPath>,
        pane: Option<WeakEntity<Pane>>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let pane = pane
            .and_then(|pane| pane.upgrade())
            .filter(|pane| pane.read(cx).is_tabbed() && pane.read(cx).is_visible())
            .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx));

        self.open_path_preview_in_pane(
            path.into(),
            pane.downgrade(),
            focus_item,
            allow_preview,
            activate,
            window,
            cx,
        )
    }

    fn open_path_preview_in_pane(
        &mut self,
        project_path: ProjectPath,
        pane: WeakEntity<Pane>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let task = self.load_path(project_path.clone(), window, cx);
        window.spawn(cx, async move |cx| {
            let (project_entry_id, build_item) = task.await?;

            pane.update_in(cx, |pane, window, cx| {
                pane.open_item(
                    project_entry_id,
                    project_path,
                    focus_item,
                    allow_preview,
                    activate,
                    None,
                    window,
                    cx,
                    build_item,
                )
            })
        })
    }

    /// Opens a URL or file path, intelligently routing to the appropriate handler:
    ///
    /// - `http://` and `https://` URLs are opened in the system's default browser
    /// - `file://` URIs are opened as files in the editor
    /// - Paths without a URL scheme are treated as file paths:
    ///   - Absolute paths are opened directly
    ///   - Relative paths are first resolved against `base_path` (if provided),
    ///     then against visible project worktrees
    /// - Other URI schemes (e.g., `mailto:`, `vscode:`) are passed to the system handler
    ///
    /// # Arguments
    /// * `url_or_path` - The URL or file path to open
    /// * `base_path` - Optional base directory for resolving relative paths (e.g., the
    ///   directory containing a markdown file). If not provided, relative paths are
    ///   resolved against project worktrees.
    ///
    /// This method provides a unified way to handle links that may be either URLs
    /// or file paths, such as those found in markdown documents, terminal output,
    /// or LSP responses.
    pub fn open_url_or_file(
        &mut self,
        url_or_path: &str,
        base_path: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut open_abs_path = |this: &mut Self, path, cx: &mut _| {
            let url_or_path = url_or_path.to_owned();
            let task = this.open_abs_path(
                path,
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            );
            (**cx)
                .spawn(async move |cx| {
                    if let Err(_) = task.await {
                        cx.update(|cx| cx.open_url(&url_or_path));
                    }
                })
                .detach();
        };

        if let Ok(url) = Url::parse(url_or_path) {
            match url.scheme() {
                "http" | "https" => cx.open_url(url_or_path),
                "file" => open_abs_path(self, PathBuf::from(url.path()), cx),
                _ => cx.open_url(url_or_path),
            }
            return;
        }

        // Not a valid URL - treat as a file path
        let project = self.project();
        let path_style = project.read(cx).path_style(cx);

        // If it's an absolute path, open it directly
        if path_style.is_absolute(url_or_path) {
            open_abs_path(self, PathBuf::from(url_or_path), cx);
            return;
        }

        let path = Path::new(url_or_path);
        // Try to resolve relative path against base_path first
        if let Some(base) = base_path
            // TODO: remotes, the exists check below hits the local FS, unsure
            // if this runs on the remote or not
            && project.read(cx).is_local()
        {
            let resolved = path_style.join(base, path).map(PathBuf::from);
            if let Some(resolved) = resolved
                && resolved.exists()
            {
                open_abs_path(self, resolved, cx);
                return;
            }
        }

        // Try to resolve against project worktrees
        if let Some(project_path) =
            project.update(cx, |project, cx| project.find_project_path(url_or_path, cx))
        {
            let url_or_path = url_or_path.to_owned();
            let task = self.open_path_in_tabbed_pane(project_path, None, true, window, cx);
            (**cx)
                .spawn(async move |cx| {
                    if let Err(_) = task.await {
                        cx.update(|cx| cx.open_url(&url_or_path));
                    }
                })
                .detach();
            return;
        }

        // Couldn't resolve as a file path - try opening as URL anyway
        // (the OS might be able to handle it)
        cx.open_url(url_or_path);
    }

    pub fn split_path(
        &mut self,
        path: impl Into<ProjectPath>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.split_path_preview(path, false, None, window, cx)
    }

    pub fn split_path_preview(
        &mut self,
        path: impl Into<ProjectPath>,
        allow_preview: bool,
        split_direction: Option<SplitDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let pane = self.ensure_tabbed_pane(window, cx).downgrade();

        if let Member::Pane(center_pane) = &self.center.root
            && center_pane.read(cx).items_len() == 0
        {
            return self.open_path_in_tabbed_pane(path, Some(pane), true, window, cx);
        }

        let project_path = path.into();
        let task = self.load_path(project_path.clone(), window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let (project_entry_id, build_item) = task.await?;
            this.update_in(cx, move |this, window, cx| -> Option<_> {
                let pane = pane.upgrade()?;
                let new_pane = this.split_pane(
                    pane,
                    split_direction.unwrap_or(SplitDirection::Right),
                    window,
                    cx,
                );
                new_pane.update(cx, |new_pane, cx| {
                    Some(new_pane.open_item(
                        project_entry_id,
                        project_path,
                        true,
                        allow_preview,
                        true,
                        None,
                        window,
                        cx,
                        build_item,
                    ))
                })
            })
            .map(|option| option.context("pane was dropped"))?
        })
    }

    fn load_path(
        &mut self,
        path: ProjectPath,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>> {
        let registry = cx.default_global::<ProjectItemRegistry>().clone();
        registry.open_path(self.project(), &path, window, cx)
    }

    pub fn find_project_item<T>(
        &self,
        pane: &Entity<Pane>,
        project_item: &Entity<T::Item>,
        cx: &App,
    ) -> Option<Entity<T>>
    where
        T: ProjectItem,
    {
        use project::ProjectItem as _;
        let project_item = project_item.read(cx);
        let entry_id = project_item.entry_id(cx);
        let project_path = project_item.project_path(cx);

        let mut item = None;
        if let Some(entry_id) = entry_id {
            item = pane.read(cx).item_for_entry(entry_id, cx);
        }
        if item.is_none()
            && let Some(project_path) = project_path
        {
            item = pane.read(cx).item_for_path(project_path, cx);
        }

        item.and_then(|item| item.downcast::<T>())
    }

    pub fn is_project_item_open<T>(
        &self,
        pane: &Entity<Pane>,
        project_item: &Entity<T::Item>,
        cx: &App,
    ) -> bool
    where
        T: ProjectItem,
    {
        self.find_project_item::<T>(pane, project_item, cx)
            .is_some()
    }

    pub fn open_project_item<T>(
        &mut self,
        mut pane: Entity<Pane>,
        project_item: Entity<T::Item>,
        activate_pane: bool,
        focus_item: bool,
        keep_old_preview: bool,
        allow_new_preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<T>
    where
        T: ProjectItem,
    {
        if !pane.read(cx).is_tabbed() {
            pane = self.ensure_tabbed_pane(window, cx);
        }

        let old_item_id = pane.read(cx).active_item().map(|item| item.item_id());

        if let Some(item) = self.find_project_item(&pane, &project_item, cx) {
            if !keep_old_preview
                && let Some(old_id) = old_item_id
                && old_id != item.item_id()
            {
                // switching to a different item, so unpreview old active item
                pane.update(cx, |pane, _| {
                    pane.unpreview_item_if_preview(old_id);
                });
            }

            self.activate_item(&item, activate_pane, focus_item, window, cx);
            if !allow_new_preview {
                pane.update(cx, |pane, _| {
                    pane.unpreview_item_if_preview(item.item_id());
                });
            }
            return item;
        }

        let item = pane.update(cx, |pane, cx| {
            cx.new(|cx| {
                T::for_project_item(self.project().clone(), Some(pane), project_item, window, cx)
            })
        });
        let mut destination_index = None;
        pane.update(cx, |pane, cx| {
            if !keep_old_preview && let Some(old_id) = old_item_id {
                pane.unpreview_item_if_preview(old_id);
            }
            if allow_new_preview {
                destination_index = pane.replace_preview_item_id(item.item_id(), window, cx);
            }
        });

        self.add_item(
            pane,
            Box::new(item.clone()),
            destination_index,
            activate_pane,
            focus_item,
            window,
            cx,
        );
        item
    }

    pub fn open_shared_screen(
        &mut self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = if self.active_pane.read(cx).is_tabbed() {
            self.active_pane.clone()
        } else {
            self.ensure_tabbed_pane(window, cx)
        };

        if let Some(shared_screen) = self.shared_screen_for_peer(peer_id, &pane, window, cx) {
            pane.update(cx, |pane, cx| {
                pane.add_item(Box::new(shared_screen), false, true, None, window, cx)
            });
        }
    }

    pub fn activate_item(
        &mut self,
        item: &dyn ItemHandle,
        activate_pane: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let result = self.panes.iter().find_map(|pane| {
            pane.read(cx)
                .index_for_item(item)
                .map(|ix| (pane.clone(), ix))
        });
        if let Some((pane, ix)) = result {
            pane.update(cx, |pane, cx| {
                pane.activate_item(ix, activate_pane, focus_item, window, cx)
            });
            true
        } else {
            false
        }
    }

    fn activate_pane_at_index(
        &mut self,
        action: &ActivatePane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panes = self.center.panes();
        if let Some(pane) = panes.get(action.0).map(|p| (*p).clone()) {
            window.focus(&pane.focus_handle(cx), cx);
        } else {
            self.split_and_clone(self.active_pane.clone(), SplitDirection::Right, window, cx)
                .detach();
        }
    }

    fn move_item_to_pane_at_index(
        &mut self,
        action: &MoveItemToPane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panes = self.center.panes();
        let destination = match panes.get(action.destination) {
            Some(&destination) => destination.clone(),
            None => {
                if !action.clone && self.active_pane.read(cx).items_len() < 2 {
                    return;
                }
                let direction = SplitDirection::Right;
                let split_off_pane = self
                    .find_pane_in_direction(direction, cx)
                    .unwrap_or_else(|| self.active_pane.clone());
                let new_pane = self.add_pane(window, cx);
                self.center.split(&split_off_pane, &new_pane, direction, cx);
                new_pane
            }
        };

        if action.clone {
            if self
                .active_pane
                .read(cx)
                .active_item()
                .is_some_and(|item| item.can_split(cx))
            {
                clone_active_item(
                    self.database_id(),
                    &self.active_pane,
                    &destination,
                    action.focus,
                    window,
                    cx,
                );
                return;
            }
        }
        move_active_item(
            &self.active_pane,
            &destination,
            action.focus,
            true,
            window,
            cx,
        )
    }

    pub fn activate_next_pane(&mut self, window: &mut Window, cx: &mut App) {
        let panes = self.center.panes();
        if let Some(ix) = panes.iter().position(|pane| **pane == self.active_pane) {
            let next_ix = (ix + 1) % panes.len();
            let next_pane = panes[next_ix].clone();
            window.focus(&next_pane.focus_handle(cx), cx);
        }
    }

    pub fn activate_previous_pane(&mut self, window: &mut Window, cx: &mut App) {
        let panes = self.center.panes();
        if let Some(ix) = panes.iter().position(|pane| **pane == self.active_pane) {
            let prev_ix = cmp::min(ix.wrapping_sub(1), panes.len() - 1);
            let prev_pane = panes[prev_ix].clone();
            window.focus(&prev_pane.focus_handle(cx), cx);
        }
    }

    pub fn activate_last_pane(&mut self, window: &mut Window, cx: &mut App) {
        let last_pane = self.center.last_pane();
        window.focus(&last_pane.focus_handle(cx), cx);
    }

    pub fn activate_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut App,
    ) {
        use ActivateInDirectionTarget as Target;
        enum Origin {
            Sidebar,
            LeftDock,
            RightDock,
            Center,
        }

        let origin: Origin = if self
            .sidebar_focus_handle
            .as_ref()
            .is_some_and(|h| h.contains_focused(window, cx))
        {
            Origin::Sidebar
        } else {
            [
                (&self.left_dock, Origin::LeftDock),
                (&self.right_dock, Origin::RightDock),
            ]
            .into_iter()
            .find_map(|(dock, origin)| {
                if dock.focus_handle(cx).contains_focused(window, cx)
                    && dock_has_focus_target(dock, cx)
                {
                    Some(origin)
                } else {
                    None
                }
            })
            .unwrap_or(Origin::Center)
        };

        let get_last_active_pane = || {
            let pane = self.last_focusable_center_pane(cx)?;
            pane.read(cx).active_item().is_some().then_some(pane)
        };

        let try_dock = |dock: &Entity<Dock>| {
            dock_has_focus_target(dock, cx).then(|| Target::Dock(dock.clone()))
        };

        let sidebar_target = self
            .sidebar_focus_handle
            .as_ref()
            .map(|h| Target::Sidebar(h.clone()));

        let sidebar_on_right = self
            .multi_workspace
            .as_ref()
            .and_then(|mw| mw.upgrade())
            .map_or(false, |mw| {
                mw.read(cx).sidebar_side(cx) == SidebarSide::Right
            });

        let away_from_sidebar = if sidebar_on_right {
            SplitDirection::Left
        } else {
            SplitDirection::Right
        };

        let (near_dock, far_dock) = if sidebar_on_right {
            (&self.right_dock, &self.left_dock)
        } else {
            (&self.left_dock, &self.right_dock)
        };

        let target = match (origin, direction) {
            (Origin::Sidebar, dir) if dir == away_from_sidebar => try_dock(near_dock)
                .or_else(|| get_last_active_pane().map(Target::Pane))
                .or_else(|| try_dock(far_dock)),

            (Origin::Sidebar, _) => None,

            // We're in the center, so we first try to go to a different pane,
            // otherwise try to go to a dock.
            (Origin::Center, direction) => {
                if let Some(pane) = self.find_pane_in_direction(direction, cx) {
                    Some(Target::Pane(pane))
                } else {
                    match direction {
                        SplitDirection::Up => None,
                        SplitDirection::Down => None,
                        SplitDirection::Left => {
                            let dock_target = try_dock(&self.left_dock);
                            if sidebar_on_right {
                                dock_target
                            } else {
                                dock_target.or(sidebar_target)
                            }
                        }
                        SplitDirection::Right => {
                            let dock_target = try_dock(&self.right_dock);
                            if sidebar_on_right {
                                dock_target.or(sidebar_target)
                            } else {
                                dock_target
                            }
                        }
                    }
                }
            }

            (Origin::LeftDock, SplitDirection::Right) => {
                if let Some(last_active_pane) = get_last_active_pane() {
                    Some(Target::Pane(last_active_pane))
                } else {
                    try_dock(&self.right_dock)
                }
            }

            (Origin::LeftDock, SplitDirection::Left) => {
                if sidebar_on_right {
                    None
                } else {
                    sidebar_target
                }
            }

            (Origin::LeftDock, SplitDirection::Down)
            | (Origin::RightDock, SplitDirection::Down) => None,

            (Origin::RightDock, SplitDirection::Left) => {
                if let Some(last_active_pane) = get_last_active_pane() {
                    Some(Target::Pane(last_active_pane))
                } else {
                    try_dock(&self.left_dock)
                }
            }

            (Origin::RightDock, SplitDirection::Right) => {
                if sidebar_on_right {
                    sidebar_target
                } else {
                    None
                }
            }

            _ => None,
        };

        match target {
            Some(ActivateInDirectionTarget::Pane(pane)) => {
                let pane = pane.read(cx);
                if let Some(item) = pane.active_item() {
                    item.item_focus_handle(cx).focus(window, cx);
                } else {
                    log::error!(
                        "Could not find a focus target when in switching focus in {direction} direction for a pane",
                    );
                }
            }
            Some(ActivateInDirectionTarget::Dock(dock)) => {
                // Defer this to avoid a panic when the dock's active panel is already on the stack.
                window.defer(cx, move |window, cx| {
                    let dock = dock.read(cx);
                    if let Some(panel) = dock.active_panel() {
                        panel.panel_focus_handle(cx).focus(window, cx);
                    } else {
                        log::error!("Could not find a focus target when in switching focus in {direction} direction for a {:?} dock", dock.position());
                    }
                })
            }
            Some(ActivateInDirectionTarget::Sidebar(focus_handle)) => {
                focus_handle.focus(window, cx);
            }
            None => {}
        }
    }

    pub fn move_item_to_pane_in_direction(
        &mut self,
        action: &MoveItemToPaneInDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destination = match self.find_pane_in_direction(action.direction, cx) {
            Some(destination) => destination,
            None => {
                if !action.clone && self.active_pane.read(cx).items_len() < 2 {
                    return;
                }
                let new_pane = self.add_pane(window, cx);
                self.center
                    .split(&self.active_pane, &new_pane, action.direction, cx);
                new_pane
            }
        };

        if action.clone {
            if self
                .active_pane
                .read(cx)
                .active_item()
                .is_some_and(|item| item.can_split(cx))
            {
                clone_active_item(
                    self.database_id(),
                    &self.active_pane,
                    &destination,
                    action.focus,
                    window,
                    cx,
                );
                return;
            }
        }
        move_active_item(
            &self.active_pane,
            &destination,
            action.focus,
            true,
            window,
            cx,
        );
    }

    pub fn bounding_box_for_pane(&self, pane: &Entity<Pane>) -> Option<Bounds<Pixels>> {
        self.center.bounding_box_for_pane(pane)
    }

    pub fn find_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        cx: &App,
    ) -> Option<Entity<Pane>> {
        self.center
            .find_pane_in_direction(&self.active_pane, direction, cx)
    }

    pub fn swap_pane_in_direction(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        if let Some(to) = self.find_pane_in_direction(direction, cx) {
            self.center.swap(&self.active_pane, &to, cx);
            cx.notify();
        }
    }

    pub fn move_pane_to_border(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        if self
            .center
            .move_to_border(&self.active_pane, direction, cx)
            .unwrap()
        {
            cx.notify();
        }
    }

    pub fn resize_pane(
        &mut self,
        axis: gpui::Axis,
        amount: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let docks = self.all_docks();
        let active_dock = docks
            .into_iter()
            .find(|dock| dock.focus_handle(cx).contains_focused(window, cx));

        if let Some(dock_entity) = active_dock {
            let dock = dock_entity.read(cx);
            let Some(panel_size) = self.dock_size(&dock, window, cx) else {
                return;
            };
            match dock.position() {
                DockPosition::Left => self.resize_left_dock(panel_size + amount, window, cx),
                DockPosition::Right => self.resize_right_dock(panel_size + amount, window, cx),
                DockPosition::Bottom => {}
            }
        } else {
            self.center
                .resize(&self.active_pane, axis, amount, &self.bounds, cx);
        }
        self.serialize_workspace(window, cx);
        cx.notify();
    }

    pub fn reset_pane_sizes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.center.reset_pane_sizes(cx);
        self.serialize_workspace(window, cx);
        cx.notify();
    }

    fn handle_pane_focused(
        &mut self,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_deferred_saves(window, cx);

        // This is explicitly hoisted out of the following check for pane identity as
        // terminal panel panes are not registered as a center panes.
        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_active_pane(&pane, window, cx);
        });
        if self.active_pane != pane {
            self.set_active_pane(&pane, window, cx);
        }

        if self.last_active_center_pane.is_none() && self.pane_is_in_center(&pane) {
            self.last_active_center_pane = Some(pane.downgrade());
        }

        // If this pane is in a dock, preserve that dock when dismissing zoomed items.
        // This prevents the dock from closing when focus events fire during window activation.
        // We also preserve any dock whose active panel itself has focus — this covers
        // panels like AgentPanel that don't implement `pane()` but can still be zoomed.
        let dock_to_preserve = self.all_docks().iter().find_map(|dock| {
            let dock_read = dock.read(cx);
            if let Some(panel) = dock_read.active_panel() {
                if panel.pane(cx).is_some_and(|dock_pane| dock_pane == pane)
                    || panel.panel_focus_handle(cx).contains_focused(window, cx)
                {
                    return Some(dock_read.position());
                }
            }
            None
        });

        self.dismiss_zoomed_items_to_reveal(dock_to_preserve, window, cx);
        if pane.read(cx).is_zoomed() {
            self.zoomed = Some(pane.downgrade().into());
        } else {
            self.zoomed = None;
        }
        self.zoomed_position = None;
        cx.emit(Event::ZoomChanged);
        self.update_active_view_for_followers(window, cx);
        pane.update(cx, |pane, _| {
            pane.track_alternate_file_items();
        });

        cx.notify();
    }

    fn set_active_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane.clone();
        self.active_item_path_changed(true, window, cx);
        if self.pane_is_in_center(pane) {
            self.last_active_center_pane = Some(pane.downgrade());
        }
    }

    fn handle_panel_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_deferred_saves(window, cx);
        self.update_active_view_for_followers(window, cx);
    }

    fn flush_deferred_saves(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let deferred = std::mem::take(&mut self.deferred_save_items);
        for weak_item in deferred {
            let Some(item) = weak_item.upgrade() else {
                continue;
            };
            // Skip if focus returned to this item
            let focus_handle = item.item_focus_handle(cx);
            if focus_handle.contains_focused(window, cx) {
                continue;
            }
            Pane::autosave_item(item.as_ref(), self.project.clone(), window, cx)
                .detach_and_log_err(cx);
        }
    }

    fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut serialize_workspace = true;
        match event {
            pane::Event::AddItem { item } => {
                item.added_to_pane(self, pane.clone(), window, cx);
                cx.emit(Event::ItemAdded {
                    item: item.boxed_clone(),
                });
            }
            pane::Event::Split { direction, mode } => {
                match mode {
                    SplitMode::ClonePane => {
                        self.split_and_clone(pane.clone(), *direction, window, cx)
                            .detach();
                    }
                    SplitMode::EmptyPane => {
                        self.split_pane(pane.clone(), *direction, window, cx);
                    }
                    SplitMode::MovePane => {
                        self.split_and_move(pane.clone(), *direction, window, cx);
                    }
                };
            }
            pane::Event::JoinIntoNext => {
                self.join_pane_into_next(pane.clone(), window, cx);
            }
            pane::Event::JoinAll => {
                self.join_all_panes(window, cx);
            }
            pane::Event::Remove { focus_on_pane } => {
                self.remove_pane(pane.clone(), focus_on_pane.clone(), window, cx);
            }
            pane::Event::ActivateItem {
                local,
                focus_changed,
            } => {
                window.invalidate_character_coordinates();

                pane.update(cx, |pane, _| {
                    pane.track_alternate_file_items();
                });
                if *local {
                    self.unfollow_in_pane(pane, window, cx);
                }
                serialize_workspace = *focus_changed || pane != self.active_pane();
                if pane == self.active_pane() {
                    self.active_item_path_changed(*focus_changed, window, cx);
                    self.update_active_view_for_followers(window, cx);
                } else if *local {
                    self.set_active_pane(pane, window, cx);
                }
            }
            pane::Event::UserSavedItem { item, save_intent } => {
                cx.emit(Event::UserSavedItem {
                    pane: pane.downgrade(),
                    item: item.boxed_clone(),
                    save_intent: *save_intent,
                });
                serialize_workspace = false;
            }
            pane::Event::ChangeItemTitle => {
                if *pane == self.active_pane {
                    self.active_item_path_changed(false, window, cx);
                }
                serialize_workspace = false;
            }
            pane::Event::RemovedItem { item } => {
                cx.emit(Event::ActiveItemChanged);
                self.update_window_edited(window, cx);
                if let hash_map::Entry::Occupied(entry) = self.panes_by_item.entry(item.item_id())
                    && entry.get().entity_id() == pane.entity_id()
                {
                    entry.remove();
                }
                cx.emit(Event::ItemRemoved {
                    item_id: item.item_id(),
                });
            }
            pane::Event::Focus => {
                window.invalidate_character_coordinates();
                self.handle_pane_focused(pane.clone(), window, cx);
            }
            pane::Event::ZoomIn => {
                if *pane == self.active_pane {
                    pane.update(cx, |pane, cx| pane.set_zoomed(true, cx));
                    if pane.read(cx).has_focus(window, cx) {
                        self.zoomed = Some(pane.downgrade().into());
                        self.zoomed_position = None;
                        cx.emit(Event::ZoomChanged);
                    }
                    cx.notify();
                }
            }
            pane::Event::ZoomOut => {
                pane.update(cx, |pane, cx| pane.set_zoomed(false, cx));
                if self.zoomed_position.is_none() {
                    self.zoomed = None;
                    cx.emit(Event::ZoomChanged);
                }
                cx.notify();
            }
            pane::Event::ItemPinned | pane::Event::ItemUnpinned => {}
        }

        if serialize_workspace {
            self.serialize_workspace(window, cx);
        }
    }

    pub fn unfollow_in_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Option<CollaboratorId> {
        let leader_id = self.leader_for_pane(pane)?;
        self.unfollow(leader_id, window, cx);
        Some(leader_id)
    }

    pub fn split_pane(
        &mut self,
        pane_to_split: Entity<Pane>,
        split_direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let new_pane = self.add_pane(window, cx);
        self.center
            .split(&pane_to_split, &new_pane, split_direction, cx);
        cx.notify();
        new_pane
    }

    fn split_size_hint_for_inserted_pane(
        &mut self,
        pane: &Entity<Pane>,
        target_pane: &Entity<Pane>,
        split_direction: SplitDirection,
        cx: &mut Context<Self>,
    ) -> Option<SplitSizeHint> {
        if pane.read(cx).pane_kind() != PaneKind::Project {
            return None;
        }

        if let Some(width) = self.center.horizontal_size_for_pane(pane) {
            pane.update(cx, |pane, _| {
                pane.remember_horizontal_split_size(width);
            });
        }

        if split_direction.axis() != Axis::Horizontal {
            return None;
        }

        let inserted_size = pane.read(cx).preferred_horizontal_split_size()?;
        let available_size = self
            .center
            .horizontal_size_for_pane(target_pane)
            .map(|target_size| {
                target_size
                    + self
                        .center
                        .horizontal_size_for_pane(pane)
                        .unwrap_or(Pixels::ZERO)
            });

        Some(match available_size {
            Some(available_size) => {
                SplitSizeHint::inserted_size_in_available_space(inserted_size, available_size)
            }
            None => SplitSizeHint::inserted_size(inserted_size),
        })
    }

    pub fn move_pane_to_pane(
        &mut self,
        pane_to_move: Entity<Pane>,
        target_pane: Entity<Pane>,
        split_direction: Option<SplitDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pane_to_move == target_pane {
            return;
        }

        if let Some(split_direction) = split_direction {
            let size_hint = self.split_size_hint_for_inserted_pane(
                &pane_to_move,
                &target_pane,
                split_direction,
                cx,
            );
            if self.center.remove(&pane_to_move, cx).unwrap_or(false) {
                self.center.split_with_size_hint(
                    &target_pane,
                    &pane_to_move,
                    split_direction,
                    size_hint,
                    cx,
                );
            }
        } else {
            self.center.swap(&pane_to_move, &target_pane, cx);
        }

        self.set_active_pane(&pane_to_move, window, cx);
        pane_to_move.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        self.serialize_workspace(window, cx);
        cx.notify();
    }

    pub fn split_and_move(
        &mut self,
        pane: Entity<Pane>,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = pane.update(cx, |pane, cx| pane.take_active_item(window, cx)) else {
            return;
        };
        let new_pane = self.add_pane(window, cx);
        new_pane.update(cx, |pane, cx| {
            pane.add_item(item, true, true, None, window, cx)
        });
        self.center.split(&pane, &new_pane, direction, cx);
        cx.notify();
    }

    pub fn split_and_clone(
        &mut self,
        pane: Entity<Pane>,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Pane>>> {
        let Some(item) = pane.read(cx).active_item() else {
            return Task::ready(None);
        };
        if !item.can_split(cx) {
            return Task::ready(None);
        }
        let task = item.clone_on_split(self.database_id(), window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Some(clone) = task.await {
                this.update_in(cx, |this, window, cx| {
                    let new_pane = this.add_pane(window, cx);
                    let nav_history = pane.read(cx).fork_nav_history();
                    new_pane.update(cx, |pane, cx| {
                        pane.set_nav_history(nav_history, cx);
                        pane.add_item(clone, true, true, None, window, cx)
                    });
                    this.center.split(&pane, &new_pane, direction, cx);
                    cx.notify();
                    new_pane
                })
                .ok()
            } else {
                None
            }
        })
    }

    pub fn join_all_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_item = self.active_pane.read(cx).active_item();
        for pane in &self.panes {
            join_pane_into_active(&self.active_pane, pane, window, cx);
        }
        if let Some(active_item) = active_item {
            self.activate_item(active_item.as_ref(), true, true, window, cx);
        }
        cx.notify();
    }

    pub fn join_pane_into_next(
        &mut self,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_pane = self
            .find_pane_in_direction(SplitDirection::Right, cx)
            .or_else(|| self.find_pane_in_direction(SplitDirection::Down, cx))
            .or_else(|| self.find_pane_in_direction(SplitDirection::Left, cx))
            .or_else(|| self.find_pane_in_direction(SplitDirection::Up, cx));
        let Some(next_pane) = next_pane else {
            return;
        };
        move_all_items(&pane, &next_pane, window, cx);
        cx.notify();
    }

    fn remove_pane(
        &mut self,
        pane: Entity<Pane>,
        focus_on: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.center.remove(&pane, cx).unwrap() {
            self.force_remove_pane(&pane, &focus_on, window, cx);
            self.unfollow_in_pane(&pane, window, cx);
            self.last_leaders_by_pane.remove(&pane.downgrade());
            for removed_item in pane.read(cx).items() {
                self.panes_by_item.remove(&removed_item.item_id());
            }

            cx.notify();
        } else {
            self.active_item_path_changed(true, window, cx);
        }
        cx.emit(Event::PaneRemoved);
    }

    pub fn panes_mut(&mut self) -> &mut [Entity<Pane>] {
        &mut self.panes
    }

    pub fn panes(&self) -> &[Entity<Pane>] {
        &self.panes
    }

    pub fn active_pane(&self) -> &Entity<Pane> {
        &self.active_pane
    }

    pub fn focused_pane(&self, window: &Window, cx: &App) -> Entity<Pane> {
        for dock in self.all_docks() {
            if dock.focus_handle(cx).contains_focused(window, cx)
                && let Some(pane) = dock
                    .read(cx)
                    .active_panel()
                    .and_then(|panel| panel.pane(cx))
            {
                return pane;
            }
        }
        self.active_pane().clone()
    }

    pub fn adjacent_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        self.find_pane_in_direction(SplitDirection::Right, cx)
            .unwrap_or_else(|| {
                self.split_pane(self.active_pane.clone(), SplitDirection::Right, window, cx)
            })
    }

    pub fn pane_for(&self, handle: &dyn ItemHandle) -> Option<Entity<Pane>> {
        self.pane_for_item_id(handle.item_id())
    }

    pub fn pane_for_item_id(&self, item_id: EntityId) -> Option<Entity<Pane>> {
        let weak_pane = self.panes_by_item.get(&item_id)?;
        weak_pane.upgrade()
    }

    pub fn pane_for_entity_id(&self, entity_id: EntityId) -> Option<Entity<Pane>> {
        self.panes
            .iter()
            .find(|pane| pane.entity_id() == entity_id)
            .cloned()
    }

    pub(crate) fn active_item_path_changed(
        &mut self,
        focus_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(Event::ActiveItemChanged);
        let active_entry = self.active_project_path(cx);
        self.project.update(cx, |project, cx| {
            project.set_active_path(active_entry.clone(), cx)
        });

        if focus_changed && let Some(project_path) = &active_entry {
            let git_store_entity = self.project.read(cx).git_store().clone();
            git_store_entity.update(cx, |git_store, cx| {
                git_store.set_active_repo_for_path(project_path, cx);
            });
        }

        self.update_window_title(window, cx);
    }

    // RPC handlers

    pub fn on_window_activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            self.update_active_view_for_followers(window, cx);

            if let Some(database_id) = self.database_id {
                let db = WorkspaceDb::global(cx);
                cx.background_spawn(async move { db.update_timestamp(database_id).await })
                    .detach();
            }
        } else {
            // When window is deactivated, flush any deferred saves since focus has left the window
            self.flush_deferred_saves(window, cx);
            for pane in &self.panes {
                pane.update(cx, |pane, cx| {
                    if let Some(item) = pane.active_item() {
                        item.workspace_deactivated(window, cx);
                    }
                    for item in pane.items() {
                        if matches!(
                            item.workspace_settings(cx).autosave,
                            AutosaveSetting::OnWindowChange | AutosaveSetting::OnFocusChange
                        ) {
                            Pane::autosave_item(item.as_ref(), self.project.clone(), window, cx)
                                .detach_and_log_err(cx);
                        }
                    }
                });
            }
        }
    }

    pub fn key_context(&self, cx: &App) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add("Workspace");
        context.set("keyboard_layout", cx.keyboard_layout().name().to_string());
        if let Some(status) = self
            .debugger_provider
            .as_ref()
            .and_then(|provider| provider.active_thread_state(cx))
        {
            match status {
                ThreadStatus::Running | ThreadStatus::Stepping => {
                    context.add("debugger_running");
                }
                ThreadStatus::Stopped => context.add("debugger_stopped"),
                ThreadStatus::Exited | ThreadStatus::Ended => {}
            }
        }

        if self.left_dock.read(cx).is_open() {
            if let Some(active_panel) = self.left_dock.read(cx).active_panel() {
                context.set("left_dock", active_panel.panel_key());
            }
        }

        if self.right_dock.read(cx).is_open() {
            if let Some(active_panel) = self.right_dock.read(cx).active_panel() {
                context.set("right_dock", active_panel.panel_key());
            }
        }

        context
    }

    /// Multiworkspace uses this to add workspace action handling to itself
    pub fn actions(&self, div: Div, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.add_workspace_actions_listeners(div, window, cx)
            .on_action(cx.listener(
                |_workspace, action_sequence: &settings::ActionSequence, window, cx| {
                    for action in &action_sequence.0 {
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                },
            ))
            .on_action(cx.listener(Self::close_inactive_items_and_panes))
            .on_action(cx.listener(Self::close_all_items_and_panes))
            .on_action(cx.listener(Self::close_item_in_all_panes))
            .on_action(cx.listener(Self::save_all))
            .on_action(cx.listener(Self::send_keystrokes))
            .on_action(cx.listener(Self::add_folder_to_project))
            .on_action(cx.listener(Self::follow_next_collaborator))
            .on_action(cx.listener(Self::activate_pane_at_index))
            .on_action(cx.listener(Self::move_item_to_pane_at_index))
            .on_action(cx.listener(Self::reopen_last_picker))
            .on_action(cx.listener(Self::toggle_edit_predictions_all_files))
            .on_action(cx.listener(Self::toggle_theme_mode))
            .on_action(cx.listener(|workspace, _: &Unfollow, window, cx| {
                let pane = workspace.active_pane().clone();
                workspace.unfollow_in_pane(&pane, window, cx);
            }))
            .on_action(cx.listener(|workspace, action: &Save, window, cx| {
                workspace
                    .save_active_item(action.save_intent.unwrap_or(SaveIntent::Save), window, cx)
                    .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
            }))
            .on_action(cx.listener(|workspace, _: &FormatAndSave, window, cx| {
                workspace
                    .save_active_item(SaveIntent::FormatAndSave, window, cx)
                    .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
            }))
            .on_action(cx.listener(|workspace, _: &SaveWithoutFormat, window, cx| {
                workspace
                    .save_active_item(SaveIntent::SaveWithoutFormat, window, cx)
                    .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
            }))
            .on_action(cx.listener(|workspace, _: &SaveAs, window, cx| {
                workspace
                    .save_active_item(SaveIntent::SaveAs, window, cx)
                    .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
            }))
            .on_action(
                cx.listener(|workspace, _: &ActivatePreviousPane, window, cx| {
                    workspace.activate_previous_pane(window, cx)
                }),
            )
            .on_action(cx.listener(|workspace, _: &ActivateNextPane, window, cx| {
                workspace.activate_next_pane(window, cx)
            }))
            .on_action(cx.listener(|workspace, _: &ActivateLastPane, window, cx| {
                workspace.activate_last_pane(window, cx)
            }))
            .on_action(
                cx.listener(|workspace, _: &ActivateNextWindow, _window, cx| {
                    workspace.activate_next_window(cx)
                }),
            )
            .on_action(
                cx.listener(|workspace, _: &ActivatePreviousWindow, _window, cx| {
                    workspace.activate_previous_window(cx)
                }),
            )
            .on_action(cx.listener(|workspace, _: &ActivatePaneLeft, window, cx| {
                workspace.activate_pane_in_direction(SplitDirection::Left, window, cx)
            }))
            .on_action(cx.listener(|workspace, _: &ActivatePaneRight, window, cx| {
                workspace.activate_pane_in_direction(SplitDirection::Right, window, cx)
            }))
            .on_action(cx.listener(|workspace, _: &ActivatePaneUp, window, cx| {
                workspace.activate_pane_in_direction(SplitDirection::Up, window, cx)
            }))
            .on_action(cx.listener(|workspace, _: &ActivatePaneDown, window, cx| {
                workspace.activate_pane_in_direction(SplitDirection::Down, window, cx)
            }))
            .on_action(cx.listener(
                |workspace, action: &MoveItemToPaneInDirection, window, cx| {
                    workspace.move_item_to_pane_in_direction(action, window, cx)
                },
            ))
            .on_action(cx.listener(|workspace, _: &SwapPaneLeft, _, cx| {
                workspace.swap_pane_in_direction(SplitDirection::Left, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SwapPaneRight, _, cx| {
                workspace.swap_pane_in_direction(SplitDirection::Right, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SwapPaneUp, _, cx| {
                workspace.swap_pane_in_direction(SplitDirection::Up, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SwapPaneDown, _, cx| {
                workspace.swap_pane_in_direction(SplitDirection::Down, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SwapPaneAdjacent, window, cx| {
                const DIRECTION_PRIORITY: [SplitDirection; 4] = [
                    SplitDirection::Down,
                    SplitDirection::Up,
                    SplitDirection::Right,
                    SplitDirection::Left,
                ];
                for dir in DIRECTION_PRIORITY {
                    if workspace.find_pane_in_direction(dir, cx).is_some() {
                        workspace.swap_pane_in_direction(dir, cx);
                        workspace.activate_pane_in_direction(dir.opposite(), window, cx);
                        break;
                    }
                }
            }))
            .on_action(cx.listener(|workspace, _: &MovePaneLeft, _, cx| {
                workspace.move_pane_to_border(SplitDirection::Left, cx)
            }))
            .on_action(cx.listener(|workspace, _: &MovePaneRight, _, cx| {
                workspace.move_pane_to_border(SplitDirection::Right, cx)
            }))
            .on_action(cx.listener(|workspace, _: &MovePaneUp, _, cx| {
                workspace.move_pane_to_border(SplitDirection::Up, cx)
            }))
            .on_action(cx.listener(|workspace, _: &MovePaneDown, _, cx| {
                workspace.move_pane_to_border(SplitDirection::Down, cx)
            }))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ClearAllNotifications, _, cx| {
                    workspace.clear_all_notifications(cx);
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ClearNavigationHistory, window, cx| {
                    workspace.clear_navigation_history(window, cx);
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &SuppressNotification, _, cx| {
                    if let Some((notification_id, _)) = workspace.notifications.pop() {
                        workspace.suppress_notification(&notification_id, cx);
                    }
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ToggleWorktreeSecurity, window, cx| {
                    workspace.show_worktree_trust_security_modal(true, window, cx);
                },
            ))
            .on_action(
                cx.listener(|_: &mut Workspace, _: &ClearTrustedWorktrees, _, cx| {
                    if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
                        trusted_worktrees.update(cx, |trusted_worktrees, _| {
                            trusted_worktrees.clear_trusted_paths()
                        });
                        let db = WorkspaceDb::global(cx);
                        cx.spawn(async move |_, cx| {
                            if db.clear_trusted_worktrees().await.log_err().is_some() {
                                cx.update(|cx| reload(cx));
                            }
                        })
                        .detach();
                    }
                }),
            )
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ReopenClosedItem, window, cx| {
                    workspace.reopen_closed_item(window, cx).detach();
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ResetPaneSizes, window, cx| {
                    workspace.reset_pane_sizes(window, cx);
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ToggleAgentPane, window, cx| {
                    workspace.toggle_panel_pane_visibility(PaneKind::Agent, window, cx);
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &ToggleProjectPane, window, cx| {
                    workspace.toggle_panel_pane_visibility(PaneKind::Project, window, cx);
                },
            ))
            .on_action(cx.listener(Workspace::toggle_centered_layout))
            .on_action(cx.listener(
                |workspace: &mut Workspace, action: &pane::ActivateNextItem, window, cx| {
                    if let Some(active_dock) = workspace.active_dock(window, cx) {
                        let dock = active_dock.read(cx);
                        if let Some(active_panel) = dock.active_panel() {
                            if active_panel.pane(cx).is_none() {
                                let mut recent_pane: Option<Entity<Pane>> = None;
                                let mut recent_timestamp = 0;
                                for pane_handle in workspace.panes() {
                                    let pane = pane_handle.read(cx);
                                    for entry in pane.activation_history() {
                                        if entry.timestamp > recent_timestamp {
                                            recent_timestamp = entry.timestamp;
                                            recent_pane = Some(pane_handle.clone());
                                        }
                                    }
                                }

                                if let Some(pane) = recent_pane {
                                    let wrap_around = action.wrap_around;
                                    pane.update(cx, |pane, cx| {
                                        let current_index = pane.active_item_index();
                                        let items_len = pane.items_len();
                                        if items_len > 0 {
                                            let next_index = if current_index + 1 < items_len {
                                                current_index + 1
                                            } else if wrap_around {
                                                0
                                            } else {
                                                return;
                                            };
                                            pane.activate_item(
                                                next_index, false, false, window, cx,
                                            );
                                        }
                                    });
                                    return;
                                }
                            }
                        }
                    }
                    cx.propagate();
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, action: &pane::ActivatePreviousItem, window, cx| {
                    if let Some(active_dock) = workspace.active_dock(window, cx) {
                        let dock = active_dock.read(cx);
                        if let Some(active_panel) = dock.active_panel() {
                            if active_panel.pane(cx).is_none() {
                                let mut recent_pane: Option<Entity<Pane>> = None;
                                let mut recent_timestamp = 0;
                                for pane_handle in workspace.panes() {
                                    let pane = pane_handle.read(cx);
                                    for entry in pane.activation_history() {
                                        if entry.timestamp > recent_timestamp {
                                            recent_timestamp = entry.timestamp;
                                            recent_pane = Some(pane_handle.clone());
                                        }
                                    }
                                }

                                if let Some(pane) = recent_pane {
                                    let wrap_around = action.wrap_around;
                                    pane.update(cx, |pane, cx| {
                                        let current_index = pane.active_item_index();
                                        let items_len = pane.items_len();
                                        if items_len > 0 {
                                            let prev_index = if current_index > 0 {
                                                current_index - 1
                                            } else if wrap_around {
                                                items_len.saturating_sub(1)
                                            } else {
                                                return;
                                            };
                                            pane.activate_item(
                                                prev_index, false, false, window, cx,
                                            );
                                        }
                                    });
                                    return;
                                }
                            }
                        }
                    }
                    cx.propagate();
                },
            ))
            .on_action(cx.listener(
                |workspace: &mut Workspace, action: &pane::CloseActiveItem, window, cx| {
                    if let Some(active_dock) = workspace.active_dock(window, cx) {
                        let dock = active_dock.read(cx);
                        if let Some(active_panel) = dock.active_panel() {
                            if active_panel.pane(cx).is_none() {
                                let active_pane = workspace.active_pane().clone();
                                active_pane.update(cx, |pane, cx| {
                                    pane.close_active_item(action, window, cx)
                                        .detach_and_log_err(cx);
                                });
                                return;
                            }
                        }
                    }
                    cx.propagate();
                },
            ))
            .on_action(
                cx.listener(|workspace, _: &ToggleReadOnlyFile, window, cx| {
                    let pane = workspace.active_pane().clone();
                    if let Some(item) = pane.read(cx).active_item() {
                        item.toggle_read_only(window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|workspace, _: &FocusCenterPane, window, cx| {
                workspace.focus_center_pane(window, cx);
            }))
            .on_action(cx.listener(Workspace::clear_bookmarks))
            .on_action(cx.listener(Workspace::cancel))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_random_database_id(&mut self) {
        self.database_id = Some(WorkspaceId(Uuid::new_v4().as_u64_pair().0 as i64));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        use node_runtime::NodeRuntime;
        use session::Session;

        let client = project.read(cx).client();
        let user_store = project.read(cx).user_store();
        let workspace_store = cx.new(|cx| WorkspaceStore::new(client.clone(), cx));
        let session = cx.new(|cx| AppSession::new(Session::test(), cx));
        window.activate_window();
        let app_state = Arc::new(AppState {
            languages: project.read(cx).languages().clone(),
            workspace_store,
            client,
            user_store,
            fs: project.read(cx).fs().clone(),
            build_window_options: |_, _| Default::default(),
            node_runtime: NodeRuntime::unavailable(),
            session,
        });
        let workspace = Self::new(Default::default(), project, app_state, window, cx);
        workspace
            .active_pane
            .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        workspace
    }

    pub fn register_action<A: Action>(
        &mut self,
        callback: impl Fn(&mut Self, &A, &mut Window, &mut Context<Self>) + 'static,
    ) -> &mut Self {
        let callback = Arc::new(callback);

        self.workspace_actions.push(Box::new(move |div, _, _, cx| {
            let callback = callback.clone();
            div.on_action(cx.listener(move |workspace, event, window, cx| {
                (callback)(workspace, event, window, cx)
            }))
        }));
        self
    }
    pub fn register_action_renderer(
        &mut self,
        callback: impl Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div + 'static,
    ) -> &mut Self {
        self.workspace_actions.push(Box::new(callback));
        self
    }

    fn add_workspace_actions_listeners(
        &self,
        mut div: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        for action in self.workspace_actions.iter() {
            div = (action)(div, self, window, cx)
        }
        div
    }

    pub fn has_active_modal(&self, _: &mut Window, cx: &mut App) -> bool {
        self.modal_layer.read(cx).has_active_modal()
    }

    pub fn active_modal<V: ManagedView + 'static>(&self, cx: &App) -> Option<Entity<V>> {
        self.modal_layer.read(cx).active_modal()
    }

    /// Toggles a modal of type `V`. If a modal of the same type is currently active,
    /// it will be hidden. If a different modal is active, it will be replaced with the new one.
    /// If no modal is active, the new modal will be shown.
    ///
    /// If closing the current modal fails (e.g., due to `on_before_dismiss` returning
    /// `DismissDecision::Dismiss(false)` or `DismissDecision::Pending`), the new modal
    /// will not be shown.
    pub fn toggle_modal<V: ModalView, B>(&mut self, window: &mut Window, cx: &mut App, build: B)
    where
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        self.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.toggle_modal(window, cx, build)
        })
    }

    pub fn hide_modal(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx))
    }

    fn reopen_last_picker(
        &mut self,
        _: &ReopenLastPicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // When triggered from within another modal (e.g. the command palette), that
        // modal's dismissal is asynchronous, so defer the reveal until it has closed;
        // otherwise a modal would still be active and the reveal would be a no-op.
        cx.defer_in(window, |workspace, window, cx| {
            workspace.modal_layer.update(cx, |modal_layer, cx| {
                modal_layer.reveal_stashed_modal(window, cx);
            });
        });
    }

    pub fn toggle_status_toast<V: ToastView>(&mut self, entity: Entity<V>, cx: &mut App) {
        self.toast_layer
            .update(cx, |toast_layer, cx| toast_layer.toggle_toast(cx, entity))
    }

    pub fn toggle_centered_layout(
        &mut self,
        _: &ToggleCenteredLayout,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.centered_layout = !self.centered_layout;
        if let Some(database_id) = self.database_id() {
            let db = WorkspaceDb::global(cx);
            let centered_layout = self.centered_layout;
            cx.background_spawn(async move {
                db.set_centered_layout(database_id, centered_layout).await
            })
            .detach_and_log_err(cx);
        }
        cx.notify();
    }

    pub fn clear_bookmarks(&mut self, _: &ClearBookmarks, _: &mut Window, cx: &mut Context<Self>) {
        self.project()
            .read(cx)
            .bookmark_store()
            .update(cx, |bookmark_store, cx| {
                bookmark_store.clear_bookmarks(cx);
            });
    }

    pub fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if cx.stop_active_drag(window) {
        } else if let Some((notification_id, _)) = self.notifications.pop() {
            dismiss_app_notification(&notification_id, cx);
        } else {
            cx.propagate();
        }
    }

    fn toggle_edit_predictions_all_files(
        &mut self,
        _: &ToggleEditPrediction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fs = self.project().read(cx).fs().clone();
        let show_edit_predictions = all_language_settings(None, cx).show_edit_predictions(None, cx);
        update_settings_file(fs, cx, move |file, _| {
            file.project.all_languages.defaults.show_edit_predictions = Some(!show_edit_predictions)
        });
    }

    fn toggle_theme_mode(&mut self, _: &ToggleMode, _window: &mut Window, cx: &mut Context<Self>) {
        let current_mode = ThemeSettings::get_global(cx).theme.mode();
        let next_mode = match current_mode {
            Some(theme_settings::ThemeAppearanceMode::Light) => {
                theme_settings::ThemeAppearanceMode::Dark
            }
            Some(theme_settings::ThemeAppearanceMode::Dark) => {
                theme_settings::ThemeAppearanceMode::Light
            }
            Some(theme_settings::ThemeAppearanceMode::System) | None => {
                match cx.theme().appearance() {
                    theme::Appearance::Light => theme_settings::ThemeAppearanceMode::Dark,
                    theme::Appearance::Dark => theme_settings::ThemeAppearanceMode::Light,
                }
            }
        };

        let fs = self.project().read(cx).fs().clone();
        settings::update_settings_file(fs, cx, move |settings, _cx| {
            theme_settings::set_mode(settings, next_mode);
        });
    }

    pub fn show_worktree_trust_security_modal(
        &mut self,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(security_modal) = self.active_modal::<SecurityModal>(cx) {
            if toggle {
                security_modal.update(cx, |security_modal, cx| {
                    security_modal.dismiss(cx);
                })
            } else {
                security_modal.update(cx, |security_modal, cx| {
                    security_modal.refresh_restricted_paths(cx);
                });
            }
        } else {
            let has_restricted_worktrees = TrustedWorktrees::has_restricted_worktrees(
                &self.project().read(cx).worktree_store(),
                cx,
            );
            if has_restricted_worktrees {
                let project = self.project().read(cx);
                let remote_host = project
                    .remote_connection_options(cx)
                    .map(RemoteHostLocation::from);
                let worktree_store = project.worktree_store().downgrade();
                self.toggle_modal(window, cx, |window, cx| {
                    SecurityModal::new(worktree_store, remote_host, window, cx)
                });
            }
        }
    }
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
