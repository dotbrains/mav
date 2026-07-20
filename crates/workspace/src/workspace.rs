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
mod workspace_channel;
mod workspace_docks;
pub mod workspace_error;
mod workspace_event;
mod workspace_focus_targets;
mod workspace_global_open;
mod workspace_id;
mod workspace_keystrokes;
mod workspace_location_helpers;
mod workspace_navigation;
mod workspace_notifications;
mod workspace_open_items;
mod workspace_open_options;
mod workspace_open_prompt;
mod workspace_project_join;
mod workspace_providers;
mod workspace_registries;
mod workspace_reload;
mod workspace_remote_open;
mod workspace_render;
mod workspace_restore;
mod workspace_settings;
mod workspace_store;
mod workspace_types;
mod workspace_window_lookup;

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

    pub fn close_global(cx: &mut App) {
        cx.defer(|cx| {
            cx.windows().iter().find(|window| {
                window
                    .update(cx, |_, window, _| {
                        if window.is_window_active() {
                            //This can only get called when the window's project connection has been lost
                            //so we don't need to prompt the user for anything and instead just close the window
                            window.remove_window();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            });
        });
    }

    pub fn prepare_to_close(
        &mut self,
        close_intent: CloseIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let active_call = self.active_global_call();

        cx.spawn_in(window, async move |this, cx| {
            this.update(cx, |this, _| {
                if close_intent == CloseIntent::CloseWindow {
                    this.removing = true;
                }
            })?;

            let workspace_count = cx.update(|_window, cx| {
                cx.windows()
                    .iter()
                    .filter(|window| window.downcast::<MultiWorkspace>().is_some())
                    .count()
            })?;

            #[cfg(target_os = "macos")]
            let save_last_workspace = false;

            // On Linux and Windows, closing the last window should restore the last workspace.
            #[cfg(not(target_os = "macos"))]
            let save_last_workspace = {
                let remaining_workspaces = cx.update(|_window, cx| {
                    cx.windows()
                        .iter()
                        .filter_map(|window| window.downcast::<MultiWorkspace>())
                        .filter_map(|multi_workspace| {
                            multi_workspace
                                .update(cx, |multi_workspace, _, cx| {
                                    multi_workspace.workspace().read(cx).removing
                                })
                                .ok()
                        })
                        .filter(|removing| !removing)
                        .count()
                })?;

                close_intent != CloseIntent::ReplaceWindow && remaining_workspaces == 0
            };

            if let Some(active_call) = active_call
                && workspace_count == 1
                && cx
                    .update(|_window, cx| active_call.0.is_in_room(cx))
                    .unwrap_or(false)
            {
                if close_intent == CloseIntent::CloseWindow {
                    this.update(cx, |_, cx| cx.emit(Event::Activate))?;
                    let answer = cx.update(|window, cx| {
                        window.prompt(
                            PromptLevel::Warning,
                            "Do you want to leave the current call?",
                            None,
                            &["Close window and hang up", "Cancel"],
                            cx,
                        )
                    })?;

                    if answer.await.log_err() == Some(1) {
                        return anyhow::Ok(false);
                    } else {
                        if let Ok(task) = cx.update(|_window, cx| active_call.0.hang_up(cx)) {
                            task.await.log_err();
                        }
                    }
                }
                if close_intent == CloseIntent::ReplaceWindow {
                    _ = cx.update(|_window, cx| {
                        let multi_workspace = cx
                            .windows()
                            .iter()
                            .filter_map(|window| window.downcast::<MultiWorkspace>())
                            .next()
                            .unwrap();
                        let project = multi_workspace
                            .read(cx)?
                            .workspace()
                            .read(cx)
                            .project
                            .clone();
                        if project.read(cx).is_shared() {
                            active_call.0.unshare_project(project, cx)?;
                        }
                        Ok::<_, anyhow::Error>(())
                    });
                }
            }

            // Hot-exit silently writes dirty buffers to the DB; only allow it
            // if the workspace will be reachable again, either via session
            // restore or by reopening its folder paths. Otherwise prompt, so
            // we don't orphan the buffers.
            let allow_hot_exit_serialization = close_intent == CloseIntent::Quit
                || save_last_workspace
                || this
                    .read_with(cx, |workspace, cx| {
                        workspace
                            .project
                            .read(cx)
                            .visible_worktrees(cx)
                            .next()
                            .is_some()
                    })
                    .unwrap_or(false);
            let save_result = this
                .update_in(cx, |this, window, cx| {
                    this.save_all_internal(
                        SaveIntent::Close,
                        allow_hot_exit_serialization,
                        window,
                        cx,
                    )
                })?
                .await;

            // If we're not quitting, but closing, we remove the workspace from
            // the current session.
            if close_intent != CloseIntent::Quit
                && !save_last_workspace
                && save_result.as_ref().is_ok_and(|&res| res)
            {
                this.update_in(cx, |this, window, cx| this.remove_from_session(window, cx))?
                    .await;
            }

            save_result
        })
    }

    fn save_all(&mut self, action: &SaveAll, window: &mut Window, cx: &mut Context<Self>) {
        self.save_all_internal(
            action.save_intent.unwrap_or(SaveIntent::SaveAll),
            true,
            window,
            cx,
        )
        .detach_and_log_err(cx);
    }

    fn send_keystrokes(
        &mut self,
        action: &SendKeystrokes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystrokes: Vec<Keystroke> = action
            .0
            .split(' ')
            .flat_map(|k| Keystroke::parse(k).log_err())
            .map(|k| {
                cx.keyboard_mapper()
                    .map_key_equivalent(k, false)
                    .inner()
                    .clone()
            })
            .collect();
        let _ = self.send_keystrokes_impl(keystrokes, window, cx);
    }

    pub fn send_keystrokes_impl(
        &mut self,
        keystrokes: Vec<Keystroke>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Shared<Task<()>> {
        let mut state = self.dispatching_keystrokes.borrow_mut();
        if !state.dispatched.insert(keystrokes.clone()) {
            cx.propagate();
            return state.task.clone().unwrap();
        }

        state.queue.extend(keystrokes);

        let keystrokes = self.dispatching_keystrokes.clone();
        if state.task.is_none() {
            state.task = Some(
                window
                    .spawn(cx, async move |cx| {
                        // limit to 100 keystrokes to avoid infinite recursion.
                        for _ in 0..100 {
                            let keystroke = {
                                let mut state = keystrokes.borrow_mut();
                                let Some(keystroke) = state.queue.pop_front() else {
                                    state.dispatched.clear();
                                    state.task.take();
                                    return;
                                };
                                keystroke
                            };
                            let focus_changed = cx
                                .update(|window, cx| {
                                    let focused = window.focused(cx);
                                    window.dispatch_keystroke(keystroke.clone(), cx);
                                    if window.focused(cx) != focused {
                                        // dispatch_keystroke may cause the focus to change.
                                        // draw's side effect is to schedule the FocusChanged events in the current flush effect cycle
                                        // And we need that to happen before the next keystroke to keep vim mode happy...
                                        // (Note that the tests always do this implicitly, so you must manually test with something like:
                                        //   "bindings": { "g z": ["workspace::SendKeystrokes", ": j <enter> u"]}
                                        // )
                                        window.draw(cx).clear();
                                        return true;
                                    }
                                    false
                                })
                                .unwrap_or(false);

                            if focus_changed {
                                futures_lite::future::yield_now().await;
                            }
                        }

                        *keystrokes.borrow_mut() = Default::default();
                        log::error!("over 100 keystrokes passed to send_keystrokes");
                    })
                    .shared(),
            );
        }
        state.task.clone().unwrap()
    }

    /// Prompts the user to save or discard each dirty item, returning
    /// `true` if they confirmed (saved/discarded everything) or `false`
    /// if they cancelled. Used before removing worktree roots during
    /// thread archival.
    pub fn prompt_to_save_or_discard_dirty_items(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        self.save_all_internal(SaveIntent::Close, true, window, cx)
    }

    fn save_all_internal(
        &mut self,
        mut save_intent: SaveIntent,
        allow_hot_exit_serialization: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        if self.project.read(cx).is_disconnected(cx) {
            return Task::ready(Ok(true));
        }
        let dirty_items = self
            .panes
            .iter()
            .flat_map(|pane| {
                pane.read(cx).items().filter_map(|item| {
                    if item.is_dirty(cx) {
                        item.tab_content_text(0, cx);
                        Some((pane.downgrade(), item.boxed_clone()))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        let project = self.project.clone();
        cx.spawn_in(window, async move |workspace, cx| {
            let dirty_items = if save_intent == SaveIntent::Close && !dirty_items.is_empty() {
                let mut serialize_tasks = Vec::new();
                let mut remaining_dirty_items = Vec::new();
                if allow_hot_exit_serialization {
                    workspace.update_in(cx, |workspace, window, cx| {
                        for (pane, item) in dirty_items {
                            if let Some(task) = item
                                .to_serializable_item_handle(cx)
                                .and_then(|handle| handle.serialize(workspace, true, window, cx))
                            {
                                serialize_tasks.push((pane, item, task));
                            } else {
                                remaining_dirty_items.push((pane, item));
                            }
                        }
                    })?;

                    for (pane, item, task) in serialize_tasks {
                        if task.await.log_err().is_none() {
                            remaining_dirty_items.push((pane, item));
                        }
                    }
                } else {
                    remaining_dirty_items = dirty_items;
                }

                if !remaining_dirty_items.is_empty() {
                    workspace.update(cx, |_, cx| cx.emit(Event::Activate))?;
                }

                if remaining_dirty_items.len() > 1 {
                    let answer = workspace.update_in(cx, |_, window, cx| {
                        cx.emit(Event::Activate);
                        let detail = Pane::file_names_for_prompt(
                            &mut remaining_dirty_items.iter().map(|(_, handle)| handle),
                            cx,
                        );
                        window.prompt(
                            PromptLevel::Warning,
                            "Do you want to save all changes in the following files?",
                            Some(&detail),
                            &["Save all", "Discard all", "Cancel"],
                            cx,
                        )
                    })?;
                    match answer.await.log_err() {
                        Some(0) => save_intent = SaveIntent::SaveAll,
                        Some(1) => save_intent = SaveIntent::Skip,
                        Some(2) => return Ok(false),
                        _ => {}
                    }
                }

                remaining_dirty_items
            } else {
                dirty_items
            };

            for (pane, item) in dirty_items {
                let (singleton, project_entry_ids) = cx.update(|_, cx| {
                    (
                        item.buffer_kind(cx) == ItemBufferKind::Singleton,
                        item.project_entry_ids(cx),
                    )
                })?;
                if (singleton || !project_entry_ids.is_empty())
                    && !Pane::save_item(project.clone(), &pane, &*item, save_intent, cx).await?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    pub fn open_workspace_for_paths(
        &mut self,
        // replace_current_window: bool,
        mut open_mode: OpenMode,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Workspace>>> {
        let requesting_window = window.window_handle().downcast::<MultiWorkspace>();
        let is_remote = self.project.read(cx).is_via_collab();
        let has_worktree = self.project.read(cx).worktrees(cx).next().is_some();
        let has_dirty_items = self.items(cx).any(|item| item.is_dirty(cx));

        let workspace_is_empty = !is_remote && !has_worktree && !has_dirty_items;
        if workspace_is_empty {
            open_mode = OpenMode::Activate;
        }

        let app_state = self.app_state.clone();

        cx.spawn(async move |_, cx| {
            let OpenResult { workspace, .. } = cx
                .update(|cx| {
                    open_paths(
                        &paths,
                        app_state,
                        OpenOptions {
                            requesting_window,
                            open_mode,
                            workspace_matching: if open_mode == OpenMode::NewWindow {
                                WorkspaceMatching::None
                            } else {
                                WorkspaceMatching::default()
                            },
                            ..Default::default()
                        },
                        cx,
                    )
                })
                .await?;
            Ok(workspace)
        })
    }

    #[allow(clippy::type_complexity)]
    pub fn open_paths(
        &mut self,
        mut abs_paths: Vec<PathBuf>,
        options: OpenOptions,
        pane: Option<WeakEntity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Vec<Option<anyhow::Result<Box<dyn ItemHandle>>>>> {
        let fs = self.app_state.fs.clone();

        let caller_ordered_abs_paths = abs_paths.clone();

        // Sort the paths to ensure we add worktrees for parents before their children.
        abs_paths.sort_unstable();
        cx.spawn_in(window, async move |this, cx| {
            let mut tasks = Vec::with_capacity(abs_paths.len());

            for abs_path in &abs_paths {
                let visible = match options.visible.as_ref().unwrap_or(&OpenVisible::None) {
                    OpenVisible::All => Some(true),
                    OpenVisible::None => Some(false),
                    OpenVisible::OnlyFiles => match fs.metadata(abs_path).await.log_err() {
                        Some(Some(metadata)) => Some(!metadata.is_dir),
                        Some(None) => Some(true),
                        None => None,
                    },
                    OpenVisible::OnlyDirectories => match fs.metadata(abs_path).await.log_err() {
                        Some(Some(metadata)) => Some(metadata.is_dir),
                        Some(None) => Some(false),
                        None => None,
                    },
                };
                let project_path = match visible {
                    Some(visible) => match this
                        .update(cx, |this, cx| {
                            Workspace::project_path_for_path(
                                this.project.clone(),
                                abs_path,
                                visible,
                                cx,
                            )
                        })
                        .log_err()
                    {
                        Some(project_path) => project_path.await.log_err(),
                        None => None,
                    },
                    None => None,
                };

                let this = this.clone();
                let abs_path: Arc<Path> = SanitizedPath::new(&abs_path).as_path().into();
                let fs = fs.clone();
                let pane = pane.clone();
                let task = cx.spawn(async move |cx| {
                    let (worktree, project_path) = project_path?;
                    let (entry_is_directory, worktree_is_local) =
                        worktree.read_with(cx, |worktree, _| {
                            let entry = if project_path.path.as_unix_str().is_empty() {
                                worktree.root_entry()
                            } else {
                                worktree.entry_for_path(&project_path.path)
                            };
                            (entry.map(|entry| entry.is_dir()), worktree.is_local())
                        });
                    let is_directory = match entry_is_directory {
                        Some(is_directory) => is_directory,
                        None if worktree_is_local => fs.is_dir(&abs_path).await,
                        None => false,
                    };

                    if is_directory {
                        // Opening a directory should not race to update the active entry.
                        // We'll select/reveal a deterministic final entry after all paths finish opening.
                        None
                    } else {
                        Some(
                            this.update_in(cx, |this, window, cx| {
                                this.open_path_in_tabbed_pane(
                                    project_path,
                                    pane,
                                    options.focus.unwrap_or(true),
                                    window,
                                    cx,
                                )
                            })
                            .ok()?
                            .await,
                        )
                    }
                });
                tasks.push(task);
            }

            let results = futures::future::join_all(tasks).await;

            // Determine the winner using the fake/abstract FS metadata, not `Path::is_dir`.
            let mut winner: Option<(PathBuf, bool)> = None;
            for abs_path in caller_ordered_abs_paths.into_iter().rev() {
                if let Some(Some(metadata)) = fs.metadata(&abs_path).await.log_err() {
                    if !metadata.is_dir {
                        winner = Some((abs_path, false));
                        break;
                    }
                    if winner.is_none() {
                        winner = Some((abs_path, true));
                    }
                } else if winner.is_none() {
                    winner = Some((abs_path, false));
                }
            }

            // Compute the winner entry id on the foreground thread and emit once, after all
            // paths finish opening. This avoids races between concurrently-opening paths
            // (directories in particular) and makes the resulting project panel selection
            // deterministic.
            if let Some((winner_abs_path, winner_is_dir)) = winner {
                'emit_winner: {
                    let winner_abs_path: Arc<Path> =
                        SanitizedPath::new(&winner_abs_path).as_path().into();

                    let visible = match options.visible.as_ref().unwrap_or(&OpenVisible::None) {
                        OpenVisible::All => true,
                        OpenVisible::None => false,
                        OpenVisible::OnlyFiles => !winner_is_dir,
                        OpenVisible::OnlyDirectories => winner_is_dir,
                    };

                    let Some(worktree_task) = this
                        .update(cx, |workspace, cx| {
                            workspace.project.update(cx, |project, cx| {
                                project.find_or_create_worktree(
                                    winner_abs_path.as_ref(),
                                    visible,
                                    cx,
                                )
                            })
                        })
                        .ok()
                    else {
                        break 'emit_winner;
                    };

                    let Ok((worktree, _)) = worktree_task.await else {
                        break 'emit_winner;
                    };

                    let Ok(Some(entry_id)) = this.update(cx, |_, cx| {
                        let worktree = worktree.read(cx);
                        let worktree_abs_path = worktree.abs_path();
                        let entry = if winner_abs_path.as_ref() == worktree_abs_path.as_ref() {
                            worktree.root_entry()
                        } else {
                            winner_abs_path
                                .strip_prefix(worktree_abs_path.as_ref())
                                .ok()
                                .and_then(|relative_path| {
                                    let relative_path =
                                        RelPath::new(relative_path, PathStyle::local())
                                            .log_err()?;
                                    worktree.entry_for_path(&relative_path)
                                })
                        }?;
                        Some(entry.id)
                    }) else {
                        break 'emit_winner;
                    };

                    this.update(cx, |workspace, cx| {
                        workspace.project.update(cx, |_, cx| {
                            cx.emit(project::Event::ActiveEntryChanged(Some(entry_id)));
                        });
                    })
                    .ok();
                }
            }

            results
        })
    }

    pub fn open_resolved_path(
        &mut self,
        path: ResolvedPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        match path {
            ResolvedPath::ProjectPath { project_path, .. } => {
                self.open_path_in_tabbed_pane(project_path, None, true, window, cx)
            }
            ResolvedPath::AbsPath { path, .. } => self.open_abs_path(
                PathBuf::from(path),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            ),
        }
    }

    pub fn absolute_path_of_worktree(
        &self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        self.project
            .read(cx)
            .worktree_for_id(worktree_id, cx)
            // TODO: use `abs_path` or `root_dir`
            .map(|wt| wt.read(cx).abs_path().as_ref().to_path_buf())
    }

    pub fn add_folder_to_project(
        &mut self,
        _: &AddFolderToProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.project.read(cx);
        if project.is_via_collab() {
            self.show_error("You cannot add folders to someone else's project", cx);
            return;
        }
        let paths = self.prompt_for_open_path(
            PathPromptOptions {
                files: false,
                directories: true,
                multiple: true,
                prompt: None,
            },
            DirectoryLister::Project(self.project.clone()),
            window,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Some(paths) = paths.await.log_err().flatten() {
                let results = this
                    .update_in(cx, |this, window, cx| {
                        this.open_paths(
                            paths,
                            OpenOptions {
                                visible: Some(OpenVisible::All),
                                ..Default::default()
                            },
                            None,
                            window,
                            cx,
                        )
                    })?
                    .await;
                for result in results.into_iter().flatten() {
                    result.log_err();
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn project_path_for_path(
        project: Entity<Project>,
        abs_path: &Path,
        visible: bool,
        cx: &mut App,
    ) -> Task<Result<(Entity<Worktree>, ProjectPath)>> {
        let entry = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, visible, cx)
        });
        cx.spawn(async move |cx| {
            let (worktree, path) = entry.await?;
            let worktree_id = worktree.read_with(cx, |t, _| t.id());
            Ok((worktree, ProjectPath { worktree_id, path }))
        })
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

    pub fn is_dock_at_position_open(&self, position: DockPosition, cx: &mut Context<Self>) -> bool {
        self.dock_at_position(position).read(cx).is_open()
    }

    pub fn toggle_dock(
        &mut self,
        dock_side: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut focus_center = false;
        let mut reveal_dock = false;

        let other_is_zoomed = self.zoomed.is_some() && self.zoomed_position != Some(dock_side);
        let was_visible = self.is_dock_at_position_open(dock_side, cx) && !other_is_zoomed;

        if let Some(panel) = self.dock_at_position(dock_side).read(cx).active_panel() {
            telemetry::event!(
                "Panel Button Clicked",
                name = panel.persistent_name(),
                toggle_state = !was_visible
            );
        }
        let dock = self.dock_at_position(dock_side);
        dock.update(cx, |dock, cx| {
            dock.set_open(!was_visible, window, cx);

            if dock.active_panel().is_none() {
                let Some(panel_ix) = dock
                    .first_enabled_panel_idx(cx)
                    .log_with_level(log::Level::Info)
                else {
                    return;
                };
                dock.activate_panel(panel_ix, window, cx);
            }

            if let Some(active_panel) = dock.active_panel() {
                if was_visible {
                    if active_panel
                        .panel_focus_handle(cx)
                        .contains_focused(window, cx)
                    {
                        focus_center = true;
                    }
                } else {
                    let focus_handle = &active_panel.panel_focus_handle(cx);
                    window.focus(focus_handle, cx);
                    reveal_dock = true;
                }
            }
        });

        if reveal_dock {
            self.dismiss_zoomed_items_to_reveal(Some(dock_side), window, cx);
        }

        if focus_center {
            self.active_pane
                .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
        }

        cx.notify();
        self.serialize_workspace(window, cx);
    }

    fn active_dock(&self, window: &Window, cx: &Context<Self>) -> Option<&Entity<Dock>> {
        self.all_docks().into_iter().find(|&dock| {
            dock.read(cx).is_open() && dock.focus_handle(cx).contains_focused(window, cx)
        })
    }

    /// Transfer focus to the panel of the given type.
    pub fn focus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<T>> {
        if let Some(panel) = self.activate_panel_item::<T>(true, window, cx) {
            return panel.to_any().downcast().ok();
        }

        let panel = self.focus_or_unfocus_panel::<T>(window, cx, &mut |_, _, _| true)?;
        panel.to_any().downcast().ok()
    }

    /// Focus the panel of the given type if it isn't already focused. If it is
    /// already focused, then transfer focus back to the workspace center.
    /// When the `close_panel_on_toggle` setting is enabled, also closes the
    /// panel when transferring focus back to the center.
    pub fn toggle_panel_focus<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some((_, _, panel)) = self.panel_item_for::<T>(cx) {
            let did_focus_panel = !panel.panel_focus_handle(cx).contains_focused(window, cx);
            if did_focus_panel {
                self.activate_panel_item::<T>(true, window, cx);
            } else if let Some(pane) = self.last_tabbed_pane(cx) {
                pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }

            telemetry::event!(
                "Panel Button Clicked",
                name = T::persistent_name(),
                toggle_state = did_focus_panel
            );

            return did_focus_panel;
        }

        let mut did_focus_panel = false;
        self.focus_or_unfocus_panel::<T>(window, cx, &mut |panel, window, cx| {
            did_focus_panel = !panel.panel_focus_handle(cx).contains_focused(window, cx);
            did_focus_panel
        });

        if !did_focus_panel && WorkspaceSettings::get_global(cx).close_panel_on_toggle {
            self.close_panel::<T>(window, cx);
        }

        telemetry::event!(
            "Panel Button Clicked",
            name = T::persistent_name(),
            toggle_state = did_focus_panel
        );

        did_focus_panel
    }

    pub fn focus_center_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.active_item(cx) {
            item.item_focus_handle(cx).focus(window, cx);
        } else {
            log::error!("Could not find a focus target when switching focus to the center panes",);
        }
    }

    pub fn activate_panel_for_proto_id(
        &mut self,
        panel_id: PanelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        if let Some((pane, ix, panel)) = self.panel_item_for_proto_id(panel_id, cx) {
            pane.update(cx, |pane, cx| {
                pane.activate_item(ix, true, true, window, cx);
            });
            panel.panel_focus_handle(cx).focus(window, cx);
            cx.notify();
            self.serialize_workspace(window, cx);
            return Some(panel);
        }

        let mut panel = None;
        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_proto_id(panel_id) {
                panel = dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);
                    dock.set_open(true, window, cx);
                    dock.active_panel().cloned()
                });
                break;
            }
        }

        if panel.is_some() {
            cx.notify();
            self.serialize_workspace(window, cx);
        }

        panel
    }

    /// Focus or unfocus the given panel type, depending on the given callback.
    fn focus_or_unfocus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        should_focus: &mut dyn FnMut(&dyn PanelHandle, &mut Window, &mut Context<Dock>) -> bool,
    ) -> Option<Arc<dyn PanelHandle>> {
        let mut result_panel = None;
        let mut serialize = false;
        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                let mut focus_center = false;
                let panel = dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);

                    let panel = dock.active_panel().cloned();
                    if let Some(panel) = panel.as_ref() {
                        if should_focus(&**panel, window, cx) {
                            dock.set_open(true, window, cx);
                            panel.panel_focus_handle(cx).focus(window, cx);
                        } else {
                            focus_center = true;
                        }
                    }
                    panel
                });

                if focus_center {
                    self.active_pane
                        .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
                }

                result_panel = panel;
                serialize = true;
                break;
            }
        }

        if serialize {
            self.serialize_workspace(window, cx);
        }

        cx.notify();
        result_panel
    }

    /// Open the panel of the given type
    pub fn open_panel<T: Panel>(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_panel_item::<T>(false, window, cx).is_some() {
            return;
        }

        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);
                    dock.set_open(true, window, cx);
                });
            }
        }
    }

    /// Open the panel of the given type, dismissing any zoomed items that
    /// would obscure it (e.g. a zoomed terminal).
    pub fn reveal_panel<T: Panel>(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_panel_item::<T>(false, window, cx).is_some() {
            return;
        }

        let dock_position = self.all_docks().iter().find_map(|dock| {
            let dock = dock.read(cx);
            dock.panel_index_for_type::<T>().map(|_| dock.position())
        });
        self.dismiss_zoomed_items_to_reveal(dock_position, window, cx);
        self.open_panel::<T>(window, cx);
    }

    pub fn close_panel<T: Panel>(&self, window: &mut Window, cx: &mut Context<Self>) {
        for dock in self.all_docks().iter() {
            dock.update(cx, |dock, cx| {
                if dock.panel::<T>().is_some() {
                    dock.set_open(false, window, cx)
                }
            })
        }
    }

    pub fn panel<T: Panel>(&self, cx: &App) -> Option<Entity<T>> {
        self.all_docks()
            .iter()
            .find_map(|dock| dock.read(cx).panel::<T>())
    }

    fn panel_item_for<T: Panel>(
        &self,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                item.is_panel::<T>()
                    .then(|| (pane.clone(), ix, item.panel()))
            })
        })
    }

    fn panel_item_for_id(
        &self,
        panel_id: EntityId,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                (item.panel_id() == panel_id).then(|| (pane.clone(), ix, item.panel()))
            })
        })
    }

    fn panel_item_for_proto_id(
        &self,
        panel_id: PanelId,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                let panel = item.panel();
                (panel.remote_id() == Some(panel_id)).then(|| (pane.clone(), ix, panel))
            })
        })
    }

    pub(crate) fn activate_panel_item_for_id(
        &mut self,
        panel_id: EntityId,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        let (pane, ix, panel) = self.panel_item_for_id(panel_id, cx)?;
        let was_hidden = pane.update(cx, |pane, cx| {
            let was_hidden = !pane.is_visible();
            pane.set_visible(true, cx);
            pane.activate_item(ix, true, focus, window, cx);
            was_hidden
        });
        if was_hidden {
            self.center.mark_positions(cx);
        }
        if focus {
            panel.panel_focus_handle(cx).focus(window, cx);
        }
        if was_hidden {
            self.serialize_workspace(window, cx);
        }
        Some(panel)
    }

    fn activate_panel_item<T: Panel>(
        &mut self,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        let (pane, ix, panel) = self.panel_item_for::<T>(cx)?;
        let was_hidden = pane.update(cx, |pane, cx| {
            let was_hidden = !pane.is_visible();
            pane.set_visible(true, cx);
            pane.activate_item(ix, true, focus, window, cx);
            was_hidden
        });
        if was_hidden {
            self.center.mark_positions(cx);
        }
        if focus {
            panel.panel_focus_handle(cx).focus(window, cx);
        }
        if was_hidden {
            self.serialize_workspace(window, cx);
        }
        Some(panel)
    }

    fn dismiss_zoomed_items_to_reveal(
        &mut self,
        dock_to_reveal: Option<DockPosition>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // If a center pane is zoomed, unzoom it.
        for pane in &self.panes {
            if pane != &self.active_pane || dock_to_reveal.is_some() {
                pane.update(cx, |pane, cx| pane.set_zoomed(false, cx));
            }
        }

        // If another dock is zoomed, hide it.
        let mut focus_center = false;
        for dock in self.all_docks() {
            dock.update(cx, |dock, cx| {
                if Some(dock.position()) != dock_to_reveal
                    && let Some(panel) = dock.active_panel()
                    && panel.is_zoomed(window, cx)
                {
                    focus_center |= panel.panel_focus_handle(cx).contains_focused(window, cx);
                    dock.set_open(false, window, cx);
                }
            });
        }

        if focus_center {
            self.active_pane
                .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
        }

        if self.zoomed_position != dock_to_reveal {
            self.zoomed = None;
            self.zoomed_position = None;
            cx.emit(Event::ZoomChanged);
        }

        cx.notify();
    }

    fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        self.add_pane_with_kind(PaneKind::Tabs, true, window, cx)
    }

    fn add_pane_with_kind(
        &mut self,
        pane_kind: PaneKind,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let pane = cx.new(|cx| {
            let mut pane = Pane::new(
                self.weak_handle(),
                self.project.clone(),
                self.pane_history_timestamp.clone(),
                None,
                NewFile.boxed_clone(),
                pane_kind.is_tabbed(),
                window,
                cx,
            );
            pane.set_can_split(Some(Arc::new(|_, _, _, _| true)));
            match pane_kind {
                PaneKind::Tabs => {}
                PaneKind::Project => configure_project_pane(&mut pane, cx),
                PaneKind::Agent => configure_agent_pane(&mut pane, cx),
            }
            pane
        });
        cx.subscribe_in(&pane, window, Self::handle_pane_event)
            .detach();
        self.panes.push(pane.clone());

        if focus {
            window.focus(&pane.focus_handle(cx), cx);
        }

        cx.emit(Event::PaneAdded(pane.clone()));
        pane
    }

    fn last_tabbed_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        self.last_active_center_pane
            .as_ref()
            .and_then(|pane| pane.upgrade())
            .filter(|pane| {
                pane.read(cx).is_tabbed()
                    && pane.read(cx).is_visible()
                    && self.pane_is_in_center(pane)
            })
            .or_else(|| {
                self.panes
                    .iter()
                    .find(|pane| {
                        pane.read(cx).is_tabbed()
                            && pane.read(cx).is_visible()
                            && self.pane_is_in_center(pane)
                    })
                    .cloned()
            })
    }

    fn ensure_tabbed_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        if let Some(pane) = self.last_tabbed_pane(cx) {
            return pane;
        }

        if let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.read(cx).pane_kind() == PaneKind::Tabs)
            .cloned()
        {
            pane.update(cx, |pane, cx| pane.set_visible(true, cx));
            self.center.mark_positions(cx);
            return pane;
        }

        let split_target = self
            .panel_pane_for_kind(PaneKind::Project, cx)
            .or_else(|| self.panel_pane_for_kind(PaneKind::Agent, cx))
            .unwrap_or_else(|| self.center.first_pane());
        let split_direction = match split_target.read(cx).pane_kind() {
            PaneKind::Project => SplitDirection::Left,
            PaneKind::Agent | PaneKind::Tabs => SplitDirection::Right,
        };

        let pane = self.add_pane_with_kind(PaneKind::Tabs, false, window, cx);
        self.center.split(&split_target, &pane, split_direction, cx);
        cx.notify();
        pane
    }

    fn ensure_visible_center_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabbed_pane = self
            .last_tabbed_pane(cx)
            .or_else(|| {
                let pane = self
                    .center
                    .panes()
                    .into_iter()
                    .find(|pane| pane.read(cx).pane_kind() == PaneKind::Tabs)
                    .cloned()?;
                pane.update(cx, |pane, cx| pane.set_visible(true, cx));
                Some(pane)
            })
            .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx));

        if !self.pane_is_in_center(&self.active_pane) || !self.active_pane.read(cx).is_visible() {
            self.set_active_pane(&tabbed_pane, window, cx);
            tabbed_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        }

        self.center.mark_positions(cx);
        cx.notify();
    }

    fn existing_tabbed_pane(
        &self,
        pane: Option<WeakEntity<Pane>>,
        cx: &App,
    ) -> Option<Entity<Pane>> {
        pane.and_then(|pane| pane.upgrade())
            .filter(|pane| pane.read(cx).is_tabbed() && pane.read(cx).is_visible())
            .or_else(|| self.last_tabbed_pane(cx))
    }

    fn last_focusable_center_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        self.last_active_center_pane
            .as_ref()
            .and_then(|pane| pane.upgrade())
            .filter(|pane| {
                pane.read(cx).is_visible()
                    && pane.read(cx).active_item().is_some()
                    && self.pane_is_in_center(pane)
            })
            .or_else(|| {
                self.center
                    .panes()
                    .into_iter()
                    .find(|pane| {
                        pane.read(cx).is_visible() && pane.read(cx).active_item().is_some()
                    })
                    .cloned()
            })
    }

    fn pane_is_in_center(&self, pane: &Entity<Pane>) -> bool {
        self.center
            .panes()
            .into_iter()
            .any(|center_pane| center_pane == pane)
    }

    pub fn panel_pane_for_kind(&self, pane_kind: PaneKind, cx: &App) -> Option<Entity<Pane>> {
        self.center
            .panes()
            .into_iter()
            .find(|pane| pane.read(cx).pane_kind() == pane_kind)
            .cloned()
    }

    pub fn panel_pane_visible(&self, pane_kind: PaneKind, cx: &App) -> bool {
        self.panel_pane_for_kind(pane_kind, cx)
            .is_some_and(|pane| pane.read(cx).is_visible())
    }

    pub fn panel_pane_visible_except(
        &self,
        pane_kind: PaneKind,
        excluded_pane: &Entity<Pane>,
        cx: &App,
    ) -> bool {
        self.center
            .panes()
            .into_iter()
            .find(|pane| *pane != excluded_pane && pane.read(cx).pane_kind() == pane_kind)
            .is_some_and(|pane| pane.read(cx).is_visible())
    }

    pub fn panel_pane_should_reserve_traffic_light_space(
        &self,
        pane_kind: PaneKind,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.panel_pane_for_kind(pane_kind, cx)
            .is_some_and(|pane| pane.read(cx).should_reserve_traffic_light_space(window, cx))
    }

    pub fn toggle_panel_pane_visibility(
        &mut self,
        pane_kind: PaneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = if let Some(pane) = self.panel_pane_for_kind(pane_kind, cx) {
            pane
        } else {
            match pane_kind {
                PaneKind::Project => self.ensure_panel_pane(PanelPaneKind::Project, window, cx),
                PaneKind::Agent => self.ensure_panel_pane(PanelPaneKind::Agent, window, cx),
                PaneKind::Tabs => self.ensure_tabbed_pane(window, cx),
            }
        };

        let visible = pane.read(cx).is_visible();
        let fallback_pane = if visible {
            self.last_tabbed_pane(cx).or_else(|| {
                self.center
                    .panes()
                    .into_iter()
                    .find(|candidate| *candidate != &pane && candidate.read(cx).is_visible())
                    .cloned()
            })
        } else {
            None
        };
        let fallback_pane = if visible && fallback_pane.is_none() {
            Some(self.ensure_tabbed_pane(window, cx))
        } else {
            fallback_pane
        };

        pane.update(cx, |pane, cx| pane.set_visible(!visible, cx));
        self.center.mark_positions(cx);

        if visible {
            if self.active_pane == pane
                && let Some(fallback_pane) = fallback_pane
            {
                self.set_active_pane(&fallback_pane, window, cx);
                fallback_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }
        } else {
            self.set_active_pane(&pane, window, cx);
            pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        }

        self.serialize_workspace(window, cx);
        cx.notify();
    }

    fn ensure_panel_pane(
        &mut self,
        panel_pane_kind: PanelPaneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let pane_kind = panel_pane_kind.pane_kind();
        let existing_pane = self
            .panes
            .iter()
            .find(|pane| pane.read(cx).pane_kind() == pane_kind && self.pane_is_in_center(pane))
            .cloned();
        if let Some(pane) = existing_pane {
            return pane;
        }

        let split_direction = match panel_pane_kind {
            PanelPaneKind::Project => SplitDirection::Right,
            PanelPaneKind::Agent => SplitDirection::Left,
        };
        let split_target = self
            .last_tabbed_pane(cx)
            .unwrap_or_else(|| self.center.first_pane());

        let pane = self.add_pane_with_kind(pane_kind, false, window, cx);
        self.center.split(&split_target, &pane, split_direction, cx);
        cx.notify();
        pane
    }

    fn add_panel_to_panel_pane<T: Panel>(
        &mut self,
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel_handle: Arc<dyn PanelHandle> = Arc::new(panel.clone());
        let activate = panel.read(cx).starts_open(window, cx);
        self.add_panel_handle_to_panel_pane(panel_handle, activate, window, cx);
    }

    fn add_panel_handle_to_panel_pane(
        &mut self,
        panel_handle: Arc<dyn PanelHandle>,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel_pane_kind) = PanelPaneKind::for_panel_key(panel_handle.panel_key()) else {
            return;
        };

        let panel_id = panel_handle.panel_id();
        let pane = self.ensure_panel_pane(panel_pane_kind, window, cx);
        pane.update(cx, |pane, cx| {
            let existing_index = pane.items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                (item.read(cx).panel_id() == panel_id).then_some(ix)
            });
            if let Some(existing_index) = existing_index {
                if activate {
                    pane.activate_item(existing_index, true, false, window, cx);
                }
                return;
            }

            let panel_priority = panel_handle.activation_priority(cx);
            let destination_index = pane.items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let existing_priority = item.read(cx).panel().activation_priority(cx);
                (existing_priority > panel_priority).then_some(ix)
            });
            let activate = pane.items_len() == 0 || activate;
            let panel_item = cx.new(|_| PanelItem::new(panel_handle.clone()));
            pane.add_item_inner(
                Box::new(panel_item),
                false,
                false,
                activate,
                destination_index,
                window,
                cx,
            );
        });
    }

    fn sync_panel_panes_from_docks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.enforce_singleton_panel_panes(window, cx);

        let mut panels = Vec::new();
        let mut active_panel_ids_by_pane_kind = HashMap::default();
        for dock in self.all_docks() {
            let dock = dock.read(cx);
            if let Some(active_panel) = dock.active_panel()
                && let Some(panel_pane_kind) =
                    PanelPaneKind::for_panel_key(active_panel.panel_key())
            {
                active_panel_ids_by_pane_kind
                    .entry(panel_pane_kind)
                    .or_insert_with(|| active_panel.panel_id());
            }

            for panel in dock.panel_handles() {
                panels.push(panel);
            }
        }

        for panel in panels {
            let activate = PanelPaneKind::for_panel_key(panel.panel_key())
                .and_then(|panel_pane_kind| active_panel_ids_by_pane_kind.get(&panel_pane_kind))
                .is_some_and(|active_panel_id| *active_panel_id == panel.panel_id());
            self.add_panel_handle_to_panel_pane(panel, activate, window, cx);
        }

        self.enforce_singleton_panel_panes(window, cx);
    }

    fn enforce_singleton_panel_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for pane_kind in [PaneKind::Agent, PaneKind::Project] {
            let mut panes = self
                .center
                .panes()
                .into_iter()
                .filter(|pane| pane.read(cx).pane_kind() == pane_kind)
                .cloned()
                .collect::<Vec<_>>();
            if panes.len() <= 1 {
                continue;
            }

            let keep_pane = panes.remove(0);
            for duplicate_pane in panes {
                self.merge_panel_pane_items(&duplicate_pane, &keep_pane, window, cx);
                self.remove_pane(duplicate_pane, Some(keep_pane.clone()), window, cx);
            }
        }
    }

    fn merge_panel_pane_items(
        &mut self,
        source_pane: &Entity<Pane>,
        target_pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_panel_id = source_pane
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<PanelItem>())
            .map(|panel_item| panel_item.read(cx).panel_id());
        let panel_items = source_pane
            .read(cx)
            .items()
            .filter_map(|item| {
                let panel_item = item.downcast::<PanelItem>()?;
                Some((panel_item.read(cx).panel_id(), item.clone()))
            })
            .collect::<Vec<_>>();

        target_pane.update(cx, |target_pane, cx| {
            for (panel_id, item) in panel_items {
                let existing_index =
                    target_pane
                        .items()
                        .enumerate()
                        .find_map(|(index, existing_item)| {
                            existing_item
                                .downcast::<PanelItem>()
                                .is_some_and(|panel_item| {
                                    panel_item.read(cx).panel_id() == panel_id
                                })
                                .then_some(index)
                        });
                if let Some(existing_index) = existing_index {
                    if active_panel_id == Some(panel_id) {
                        target_pane.activate_item(existing_index, true, false, window, cx);
                    }
                    continue;
                }

                let activate = target_pane.items_len() == 0 || active_panel_id == Some(panel_id);
                target_pane.add_item_inner(item, false, false, activate, None, window, cx);
            }
        });
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

    pub fn auto_watch_state(&self) -> &AutoWatch {
        &self.auto_watch
    }

    fn next_watched_peer(&self, cx: &App) -> Option<PeerId> {
        self.active_call()
            .and_then(|call| call.peer_ids_with_video_tracks(cx).first().copied())
    }

    pub fn toggle_auto_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.auto_watch.enabled() {
            self.auto_watch = AutoWatch::Off;
            cx.notify();
            return;
        }

        let active_pane = self.active_pane.clone();
        self.unfollow_in_pane(&active_pane, window, cx);

        let local_is_sharing = self
            .active_call()
            .map_or(false, |call| call.is_sharing_screen(cx));

        if local_is_sharing {
            self.auto_watch = AutoWatch::Paused;
        } else {
            let watched_peer = self.next_watched_peer(cx);
            self.auto_watch = AutoWatch::Active { watched_peer };

            if let Some(peer_id) = watched_peer {
                self.open_shared_screen(peer_id, window, cx);
            }
        }

        cx.notify();
    }

    fn handle_auto_watch_video_tracks_changed(
        &mut self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AutoWatch::Active { watched_peer } = self.auto_watch else {
            return;
        };

        let peer_is_sharing = self.active_call().map_or(false, |call| {
            call.peer_ids_with_video_tracks(cx).contains(&peer_id)
        });
        let should_watch_peer = peer_is_sharing && watched_peer.is_none();
        let watched_peer_stopped_sharing = watched_peer == Some(peer_id) && !peer_is_sharing;

        if should_watch_peer || watched_peer_stopped_sharing {
            let next_watched_peer = if should_watch_peer {
                Some(peer_id)
            } else {
                self.next_watched_peer(cx)
            };

            self.auto_watch = AutoWatch::Active {
                watched_peer: next_watched_peer,
            };

            if let Some(next_watched_peer) = next_watched_peer {
                self.open_shared_screen(next_watched_peer, window, cx);
            }
        }
    }

    fn handle_auto_watch_local_share_stopped(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AutoWatch::Paused = self.auto_watch else {
            return;
        };

        let watched_peer = self.next_watched_peer(cx);
        self.auto_watch = AutoWatch::Active { watched_peer };

        if let Some(peer_id) = watched_peer {
            self.open_shared_screen(peer_id, window, cx);
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

    fn collaborator_left(&mut self, peer_id: PeerId, window: &mut Window, cx: &mut Context<Self>) {
        self.follower_states.retain(|leader_id, state| {
            if *leader_id == CollaboratorId::PeerId(peer_id) {
                for item in state.items_by_leader_view_id.values() {
                    item.view.set_leader_id(None, window, cx);
                }
                false
            } else {
                true
            }
        });
        cx.notify();
    }

    pub fn start_following(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let leader_id = leader_id.into();
        let pane = self.active_pane().clone();

        self.last_leaders_by_pane
            .insert(pane.downgrade(), leader_id);
        self.unfollow(leader_id, window, cx);
        self.unfollow_in_pane(&pane, window, cx);
        self.auto_watch = AutoWatch::Off;
        self.follower_states.insert(
            leader_id,
            FollowerState {
                center_pane: pane.clone(),
                dock_pane: None,
                active_view_id: None,
                items_by_leader_view_id: Default::default(),
            },
        );
        cx.notify();

        match leader_id {
            CollaboratorId::PeerId(leader_peer_id) => {
                let room_id = self.active_call()?.room_id(cx)?;
                let project_id = self.project.read(cx).remote_id();
                let request = self.app_state.client.request(proto::Follow {
                    room_id,
                    project_id,
                    leader_id: Some(leader_peer_id),
                });

                Some(cx.spawn_in(window, async move |this, cx| {
                    let response = request.await?;
                    this.update(cx, |this, _| {
                        let state = this
                            .follower_states
                            .get_mut(&leader_id)
                            .context("following interrupted")?;
                        state.active_view_id = response
                            .active_view
                            .as_ref()
                            .and_then(|view| ViewId::from_proto(view.id.clone()?).ok());
                        anyhow::Ok(())
                    })??;
                    if let Some(view) = response.active_view {
                        Self::add_view_from_leader(this.clone(), leader_peer_id, &view, cx).await?;
                    }
                    this.update_in(cx, |this, window, cx| {
                        this.leader_updated(leader_id, window, cx)
                    })?;
                    Ok(())
                }))
            }
            CollaboratorId::Agent => {
                self.leader_updated(leader_id, window, cx)?;
                Some(Task::ready(Ok(())))
            }
        }
    }

    pub fn follow_next_collaborator(
        &mut self,
        _: &FollowNextCollaborator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collaborators = self.project.read(cx).collaborators();
        let next_leader_id = if let Some(leader_id) = self.leader_for_pane(&self.active_pane) {
            let mut collaborators = collaborators.keys().copied();
            for peer_id in collaborators.by_ref() {
                if CollaboratorId::PeerId(peer_id) == leader_id {
                    break;
                }
            }
            collaborators.next().map(CollaboratorId::PeerId)
        } else if let Some(last_leader_id) =
            self.last_leaders_by_pane.get(&self.active_pane.downgrade())
        {
            match last_leader_id {
                CollaboratorId::PeerId(peer_id) => {
                    if collaborators.contains_key(peer_id) {
                        Some(*last_leader_id)
                    } else {
                        None
                    }
                }
                CollaboratorId::Agent => Some(CollaboratorId::Agent),
            }
        } else {
            None
        };

        let pane = self.active_pane.clone();
        let Some(leader_id) = next_leader_id.or_else(|| {
            Some(CollaboratorId::PeerId(
                collaborators.keys().copied().next()?,
            ))
        }) else {
            return;
        };
        if self.unfollow_in_pane(&pane, window, cx) == Some(leader_id) {
            return;
        }
        if let Some(task) = self.start_following(leader_id, window, cx) {
            task.detach_and_log_err(cx)
        }
    }

    pub fn follow(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leader_id = leader_id.into();

        if let CollaboratorId::PeerId(peer_id) = leader_id {
            let Some(active_call) = GlobalAnyActiveCall::try_global(cx) else {
                return;
            };
            let Some(remote_participant) =
                active_call.0.remote_participant_for_peer_id(peer_id, cx)
            else {
                return;
            };

            let project = self.project.read(cx);

            let other_project_id = match remote_participant.location {
                ParticipantLocation::External => None,
                ParticipantLocation::UnsharedProject => None,
                ParticipantLocation::SharedProject { project_id } => {
                    if Some(project_id) == project.remote_id() {
                        None
                    } else {
                        Some(project_id)
                    }
                }
            };

            // if they are active in another project, follow there.
            if let Some(project_id) = other_project_id {
                let app_state = self.app_state.clone();
                crate::join_in_room_project(
                    project_id,
                    remote_participant.user.legacy_id,
                    app_state,
                    cx,
                )
                .detach_and_prompt_err(
                    "Failed to join project",
                    window,
                    cx,
                    |error, _, _| Some(format!("{error:#}")),
                );
            }
        }

        // if you're already following, find the right pane and focus it.
        if let Some(follower_state) = self.follower_states.get(&leader_id) {
            window.focus(&follower_state.pane().focus_handle(cx), cx);

            return;
        }

        // Otherwise, follow.
        if let Some(task) = self.start_following(leader_id, window, cx) {
            task.detach_and_log_err(cx)
        }
    }

    pub fn unfollow(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        cx.notify();

        let leader_id = leader_id.into();
        let state = self.follower_states.remove(&leader_id)?;
        for (_, item) in state.items_by_leader_view_id {
            item.view.set_leader_id(None, window, cx);
        }

        if let CollaboratorId::PeerId(leader_peer_id) = leader_id {
            let project_id = self.project.read(cx).remote_id();
            let room_id = self.active_call()?.room_id(cx)?;
            self.app_state
                .client
                .send(proto::Unfollow {
                    room_id,
                    project_id,
                    leader_id: Some(leader_peer_id),
                })
                .log_err();
        }

        Some(())
    }

    pub fn is_being_followed(&self, id: impl Into<CollaboratorId>) -> bool {
        self.follower_states.contains_key(&id.into())
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

    /// Whether this workspace may write the platform window's title and edited
    /// indicator.
    ///
    /// In a multi-workspace window several workspaces share one platform
    /// window, so only the active one is allowed to write that chrome —
    /// otherwise a background workspace's project/item events would clobber the
    /// active workspace's title. `MultiWorkspace` publishes the active
    /// workspace's id into the shared `active_workspace_id` cell, which we
    /// simply compare against our own id. A workspace with no shared cell (e.g.
    /// a plain test window) owns its window unconditionally.
    fn owns_window_chrome(&self) -> bool {
        match &self.active_workspace_id {
            Some(active_workspace_id) => active_workspace_id.get() == self.weak_self.entity_id(),
            None => true,
        }
    }

    fn update_window_title(&mut self, window: &mut Window, cx: &mut App) {
        if !self.owns_window_chrome() {
            return;
        }
        self.apply_window_title(window, cx);
    }

    fn apply_window_title(&mut self, window: &mut Window, cx: &mut App) {
        let project = self.project().read(cx);
        let mut title = String::new();

        for (i, worktree) in project.visible_worktrees(cx).enumerate() {
            let name = worktree.read(cx).root_name_str();

            if i > 0 {
                title.push_str(", ");
            }
            title.push_str(name);
        }

        if title.is_empty() {
            title = "empty project".to_string();
        }

        let active_project_path = self.active_item(cx).and_then(|item| item.project_path(cx));

        if let Some(path) = active_project_path.as_ref() {
            let filename = path.path.file_name().or_else(|| {
                Some(
                    project
                        .worktree_for_id(path.worktree_id, cx)?
                        .read(cx)
                        .root_name_str(),
                )
            });

            if let Some(filename) = filename {
                title.push_str(" — ");
                title.push_str(filename.as_ref());
            }
        }

        if project.is_via_collab() {
            title.push_str(" ↙");
        } else if project.is_shared() {
            title.push_str(" ↗");
        }

        let document_path = active_project_path
            .as_ref()
            .and_then(|path| project.absolute_path(path, cx));
        window.set_document_path(document_path.as_deref());

        if let Some(last_title) = self.last_window_title.as_ref()
            && &title == last_title
        {
            return;
        }
        window.set_window_title(&title);
        SystemWindowTabController::update_tab_title(
            cx,
            window.window_handle().window_id(),
            SharedString::from(&title),
        );
        self.last_window_title = Some(title);
    }

    fn is_window_edited(&self, cx: &App) -> bool {
        !self.project.read(cx).is_disconnected(cx) && !self.dirty_items.is_empty()
    }

    fn update_window_edited(&mut self, window: &mut Window, cx: &mut App) {
        if !self.owns_window_chrome() {
            return;
        }
        let is_edited = self.is_window_edited(cx);
        if is_edited != self.window_edited {
            self.window_edited = is_edited;
            window.set_window_edited(self.window_edited)
        }
    }

    /// Re-applies this workspace's title and edited indicator to the platform
    /// window, bypassing the change-detection caches.
    ///
    /// Several workspaces can share a single platform window (a multi-workspace
    /// window). The `last_window_title`/`window_edited` caches assume the
    /// window's title and edited state reflect *this* workspace, but after the
    /// active workspace changes those values may have been set by a different
    /// workspace. Clearing the caches forces the values to be re-applied so the
    /// window reflects the newly-active workspace.
    ///
    /// This is only ever invoked for the workspace that just became active, so
    /// it writes directly rather than going through `update_window_title` /
    /// `update_window_edited`, whose change-detection caches would otherwise
    /// suppress the write when this workspace's computed title/edited state
    /// matches what *it* last set (even though the shared window currently
    /// shows a different workspace's state).
    pub fn refresh_window_state(&mut self, window: &mut Window, cx: &mut App) {
        self.last_window_title = None;
        self.apply_window_title(window, cx);

        self.window_edited = self.is_window_edited(cx);
        window.set_window_edited(self.window_edited);
    }

    fn update_item_dirty_state(
        &mut self,
        item: &dyn ItemHandle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let is_dirty = item.is_dirty(cx);
        let item_id = item.item_id();
        let was_dirty = self.dirty_items.contains_key(&item_id);
        if is_dirty == was_dirty {
            return;
        }
        if was_dirty {
            self.dirty_items.remove(&item_id);
            self.update_window_edited(window, cx);
            return;
        }

        let workspace = self.weak_handle();
        let Some(window_handle) = window.window_handle().downcast::<MultiWorkspace>() else {
            return;
        };
        let on_release_callback = Box::new(move |cx: &mut App| {
            window_handle
                .update(cx, |_, window, cx| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.dirty_items.remove(&item_id);
                            workspace.update_window_edited(window, cx)
                        })
                        .ok();
                })
                .ok();
        });

        let s = item.on_release(cx, on_release_callback);
        self.dirty_items.insert(item_id, s);
        self.update_window_edited(window, cx);
    }

    fn render_notifications(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Div> {
        if self.notifications.is_empty() {
            None
        } else {
            Some(
                div()
                    .absolute()
                    .right_3()
                    .bottom_3()
                    .w_112()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .gap_2()
                    .children(
                        self.notifications
                            .iter()
                            .map(|(_, notification)| notification.clone().into_any()),
                    ),
            )
        }
    }

    // RPC handlers

    fn active_view_for_follower(
        &self,
        follower_project_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<proto::View> {
        let (item, panel_id) = self.active_item_for_followers(window, cx);
        let item = item?;
        let leader_id = self
            .pane_for(&*item)
            .and_then(|pane| self.leader_for_pane(&pane));
        let leader_peer_id = match leader_id {
            Some(CollaboratorId::PeerId(peer_id)) => Some(peer_id),
            Some(CollaboratorId::Agent) | None => None,
        };

        let item_handle = item.to_followable_item_handle(cx)?;
        let id = item_handle.remote_id(&self.app_state.client, window, cx)?;
        let variant = item_handle.to_state_proto(window, cx)?;

        if item_handle.is_project_item(window, cx)
            && (follower_project_id.is_none()
                || follower_project_id != self.project.read(cx).remote_id())
        {
            return None;
        }

        Some(proto::View {
            id: id.to_proto(),
            leader_id: leader_peer_id,
            variant: Some(variant),
            panel_id: panel_id.map(|id| id as i32),
        })
    }

    fn handle_follow(
        &mut self,
        follower_project_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> proto::FollowResponse {
        let active_view = self.active_view_for_follower(follower_project_id, window, cx);

        cx.notify();
        proto::FollowResponse {
            views: active_view.iter().cloned().collect(),
            active_view,
        }
    }

    fn handle_update_followers(
        &mut self,
        leader_id: PeerId,
        message: proto::UpdateFollowers,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.leader_updates_tx
            .unbounded_send((leader_id, message))
            .ok();
    }

    async fn process_leader_update(
        this: &WeakEntity<Self>,
        leader_id: PeerId,
        update: proto::UpdateFollowers,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        match update.variant.context("invalid update")? {
            proto::update_followers::Variant::CreateView(view) => {
                let view_id = ViewId::from_proto(view.id.clone().context("invalid view id")?)?;
                let should_add_view = this.update(cx, |this, _| {
                    if let Some(state) = this.follower_states.get_mut(&leader_id.into()) {
                        anyhow::Ok(!state.items_by_leader_view_id.contains_key(&view_id))
                    } else {
                        anyhow::Ok(false)
                    }
                })??;

                if should_add_view {
                    Self::add_view_from_leader(this.clone(), leader_id, &view, cx).await?
                }
            }
            proto::update_followers::Variant::UpdateActiveView(update_active_view) => {
                let should_add_view = this.update(cx, |this, _| {
                    if let Some(state) = this.follower_states.get_mut(&leader_id.into()) {
                        state.active_view_id = update_active_view
                            .view
                            .as_ref()
                            .and_then(|view| ViewId::from_proto(view.id.clone()?).ok());

                        if state.active_view_id.is_some_and(|view_id| {
                            !state.items_by_leader_view_id.contains_key(&view_id)
                        }) {
                            anyhow::Ok(true)
                        } else {
                            anyhow::Ok(false)
                        }
                    } else {
                        anyhow::Ok(false)
                    }
                })??;

                if should_add_view && let Some(view) = update_active_view.view {
                    Self::add_view_from_leader(this.clone(), leader_id, &view, cx).await?
                }
            }
            proto::update_followers::Variant::UpdateView(update_view) => {
                let variant = update_view.variant.context("missing update view variant")?;
                let id = update_view.id.context("missing update view id")?;
                let mut tasks = Vec::new();
                this.update_in(cx, |this, window, cx| {
                    let project = this.project.clone();
                    if let Some(state) = this.follower_states.get(&leader_id.into()) {
                        let view_id = ViewId::from_proto(id.clone())?;
                        if let Some(item) = state.items_by_leader_view_id.get(&view_id) {
                            tasks.push(item.view.apply_update_proto(
                                &project,
                                variant.clone(),
                                window,
                                cx,
                            ));
                        }
                    }
                    anyhow::Ok(())
                })??;
                try_join_all(tasks).await.log_err();
            }
        }
        this.update_in(cx, |this, window, cx| {
            this.leader_updated(leader_id, window, cx)
        })?;
        Ok(())
    }

    async fn add_view_from_leader(
        this: WeakEntity<Self>,
        leader_id: PeerId,
        view: &proto::View,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        let this = this.upgrade().context("workspace dropped")?;

        let Some(id) = view.id.clone() else {
            anyhow::bail!("no id for view");
        };
        let id = ViewId::from_proto(id)?;
        let panel_id = view.panel_id.and_then(proto::PanelId::from_i32);

        let pane = this.update(cx, |this, _cx| {
            let state = this
                .follower_states
                .get(&leader_id.into())
                .context("stopped following")?;
            anyhow::Ok(state.pane().clone())
        })?;
        let existing_item = pane.update_in(cx, |pane, window, cx| {
            let client = this.read(cx).client().clone();
            pane.items().find_map(|item| {
                let item = item.to_followable_item_handle(cx)?;
                if item.remote_id(&client, window, cx) == Some(id) {
                    Some(item)
                } else {
                    None
                }
            })
        })?;
        let item = if let Some(existing_item) = existing_item {
            existing_item
        } else {
            let variant = view.variant.clone();
            anyhow::ensure!(variant.is_some(), "missing view variant");

            let task = cx.update(|window, cx| {
                FollowableViewRegistry::from_state_proto(this.clone(), id, variant, window, cx)
            })?;

            let Some(task) = task else {
                anyhow::bail!(
                    "failed to construct view from leader (maybe from a different version of mav?)"
                );
            };

            let mut new_item = task.await?;
            pane.update_in(cx, |pane, window, cx| {
                let mut item_to_remove = None;
                for (ix, item) in pane.items().enumerate() {
                    if let Some(item) = item.to_followable_item_handle(cx) {
                        match new_item.dedup(item.as_ref(), window, cx) {
                            Some(item::Dedup::KeepExisting) => {
                                new_item =
                                    item.boxed_clone().to_followable_item_handle(cx).unwrap();
                                break;
                            }
                            Some(item::Dedup::ReplaceExisting) => {
                                item_to_remove = Some((ix, item.item_id()));
                                break;
                            }
                            None => {}
                        }
                    }
                }

                if let Some((ix, id)) = item_to_remove {
                    pane.remove_item(id, false, false, window, cx);
                    pane.add_item(new_item.boxed_clone(), false, false, Some(ix), window, cx);
                }
            })?;

            new_item
        };

        this.update_in(cx, |this, window, cx| {
            let state = this.follower_states.get_mut(&leader_id.into())?;
            item.set_leader_id(Some(leader_id.into()), window, cx);
            state.items_by_leader_view_id.insert(
                id,
                FollowerView {
                    view: item,
                    location: panel_id,
                },
            );

            Some(())
        })
        .context("no follower state")?;

        Ok(())
    }

    fn handle_agent_location_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(follower_state) = self.follower_states.get_mut(&CollaboratorId::Agent) else {
            return;
        };

        if let Some(agent_location) = self.project.read(cx).agent_location() {
            let buffer_entity_id = agent_location.buffer.entity_id();
            let view_id = ViewId {
                creator: CollaboratorId::Agent,
                id: buffer_entity_id.as_u64(),
            };
            follower_state.active_view_id = Some(view_id);

            let item = match follower_state.items_by_leader_view_id.entry(view_id) {
                hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
                hash_map::Entry::Vacant(entry) => {
                    let existing_view =
                        follower_state
                            .center_pane
                            .read(cx)
                            .items()
                            .find_map(|item| {
                                let item = item.to_followable_item_handle(cx)?;
                                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                                    && item.project_item_model_ids(cx).as_slice()
                                        == [buffer_entity_id]
                                {
                                    Some(item)
                                } else {
                                    None
                                }
                            });
                    let view = existing_view.or_else(|| {
                        agent_location.buffer.upgrade().and_then(|buffer| {
                            cx.update_default_global(|registry: &mut ProjectItemRegistry, cx| {
                                registry.build_item(buffer, self.project.clone(), None, window, cx)
                            })?
                            .to_followable_item_handle(cx)
                        })
                    });

                    view.map(|view| {
                        entry.insert(FollowerView {
                            view,
                            location: None,
                        })
                    })
                }
            };

            if let Some(item) = item {
                item.view
                    .set_leader_id(Some(CollaboratorId::Agent), window, cx);
                item.view
                    .update_agent_location(agent_location.position, window, cx);
            }
        } else {
            follower_state.active_view_id = None;
        }

        self.leader_updated(CollaboratorId::Agent, window, cx);
    }

    pub fn update_active_view_for_followers(&mut self, window: &mut Window, cx: &mut App) {
        let mut is_project_item = true;
        let mut update = proto::UpdateActiveView::default();
        if window.is_window_active() {
            let (active_item, panel_id) = self.active_item_for_followers(window, cx);

            if let Some(item) = active_item
                && item.item_focus_handle(cx).contains_focused(window, cx)
            {
                let leader_id = self
                    .pane_for(&*item)
                    .and_then(|pane| self.leader_for_pane(&pane));
                let leader_peer_id = match leader_id {
                    Some(CollaboratorId::PeerId(peer_id)) => Some(peer_id),
                    Some(CollaboratorId::Agent) | None => None,
                };

                if let Some(item) = item.to_followable_item_handle(cx) {
                    let id = item
                        .remote_id(&self.app_state.client, window, cx)
                        .map(|id| id.to_proto());

                    if let Some(id) = id
                        && let Some(variant) = item.to_state_proto(window, cx)
                    {
                        let view = Some(proto::View {
                            id,
                            leader_id: leader_peer_id,
                            variant: Some(variant),
                            panel_id: panel_id.map(|id| id as i32),
                        });

                        is_project_item = item.is_project_item(window, cx);
                        update = proto::UpdateActiveView { view };
                    };
                }
            }
        }

        let active_view_id = update.view.as_ref().and_then(|view| view.id.as_ref());
        if active_view_id != self.last_active_view_id.as_ref() {
            self.last_active_view_id = active_view_id.cloned();
            self.update_followers(
                is_project_item,
                proto::update_followers::Variant::UpdateActiveView(update),
                window,
                cx,
            );
        }
    }

    fn active_item_for_followers(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> (Option<Box<dyn ItemHandle>>, Option<proto::PanelId>) {
        let mut active_item = None;
        let mut panel_id = None;
        for dock in self.all_docks() {
            if dock.focus_handle(cx).contains_focused(window, cx)
                && let Some(panel) = dock.read(cx).active_panel()
                && let Some(pane) = panel.pane(cx)
                && let Some(item) = pane.read(cx).active_item()
            {
                active_item = Some(item);
                panel_id = panel.remote_id();
                break;
            }
        }

        if active_item.is_none() {
            active_item = self.active_pane().read(cx).active_item();
        }
        (active_item, panel_id)
    }

    fn update_followers(
        &self,
        project_only: bool,
        update: proto::update_followers::Variant,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<()> {
        // If this update only applies to for followers in the current project,
        // then skip it unless this project is shared. If it applies to all
        // followers, regardless of project, then set `project_id` to none,
        // indicating that it goes to all followers.
        let project_id = if project_only {
            Some(self.project.read(cx).remote_id()?)
        } else {
            None
        };
        self.app_state().workspace_store.update(cx, |store, cx| {
            store.update_followers(project_id, update, cx)
        })
    }

    pub fn leader_for_pane(&self, pane: &Entity<Pane>) -> Option<CollaboratorId> {
        self.follower_states.iter().find_map(|(leader_id, state)| {
            if state.center_pane == *pane || state.dock_pane.as_ref() == Some(pane) {
                Some(*leader_id)
            } else {
                None
            }
        })
    }

    fn leader_updated(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Box<dyn ItemHandle>> {
        cx.notify();

        let leader_id = leader_id.into();
        let (panel_id, item) = match leader_id {
            CollaboratorId::PeerId(peer_id) => self.active_item_for_peer(peer_id, window, cx)?,
            CollaboratorId::Agent => (None, self.active_item_for_agent()?),
        };

        let state = self.follower_states.get(&leader_id)?;
        let mut transfer_focus = state.center_pane.read(cx).has_focus(window, cx);
        let pane;
        if let Some(panel_id) = panel_id {
            pane = self
                .activate_panel_for_proto_id(panel_id, window, cx)?
                .pane(cx)?;
            let state = self.follower_states.get_mut(&leader_id)?;
            state.dock_pane = Some(pane.clone());
        } else {
            pane = state.center_pane.clone();
            let state = self.follower_states.get_mut(&leader_id)?;
            if let Some(dock_pane) = state.dock_pane.take() {
                transfer_focus |= dock_pane.focus_handle(cx).contains_focused(window, cx);
            }
        }

        pane.update(cx, |pane, cx| {
            let focus_active_item = pane.has_focus(window, cx) || transfer_focus;
            if let Some(index) = pane.index_for_item(item.as_ref()) {
                pane.activate_item(index, false, false, window, cx);
            } else {
                pane.add_item(item.boxed_clone(), false, false, None, window, cx)
            }

            if focus_active_item {
                pane.focus_active_item(window, cx)
            }
        });

        Some(item)
    }

    fn active_item_for_agent(&self) -> Option<Box<dyn ItemHandle>> {
        let state = self.follower_states.get(&CollaboratorId::Agent)?;
        let active_view_id = state.active_view_id?;
        Some(
            state
                .items_by_leader_view_id
                .get(&active_view_id)?
                .view
                .boxed_clone(),
        )
    }

    fn active_item_for_peer(
        &self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Option<PanelId>, Box<dyn ItemHandle>)> {
        let call = self.active_call()?;
        let participant = call.remote_participant_for_peer_id(peer_id, cx)?;
        let leader_in_this_app;
        let leader_in_this_project;
        match participant.location {
            ParticipantLocation::SharedProject { project_id } => {
                leader_in_this_app = true;
                leader_in_this_project = Some(project_id) == self.project.read(cx).remote_id();
            }
            ParticipantLocation::UnsharedProject => {
                leader_in_this_app = true;
                leader_in_this_project = false;
            }
            ParticipantLocation::External => {
                leader_in_this_app = false;
                leader_in_this_project = false;
            }
        };
        let state = self.follower_states.get(&peer_id.into())?;
        let mut item_to_activate = None;
        if let (Some(active_view_id), true) = (state.active_view_id, leader_in_this_app) {
            if let Some(item) = state.items_by_leader_view_id.get(&active_view_id)
                && (leader_in_this_project || !item.view.is_project_item(window, cx))
            {
                item_to_activate = Some((item.location, item.view.boxed_clone()));
            }
        } else if let Some(shared_screen) =
            self.shared_screen_for_peer(peer_id, &state.center_pane, window, cx)
        {
            item_to_activate = Some((None, Box::new(shared_screen)));
        }
        item_to_activate
    }

    fn shared_screen_for_peer(
        &self,
        peer_id: PeerId,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<SharedScreen>> {
        self.active_call()?
            .create_shared_screen(peer_id, pane, window, cx)
    }

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

    pub fn active_call(&self) -> Option<&dyn AnyActiveCall> {
        self.active_call.as_ref().map(|(call, _)| &*call.0)
    }

    pub fn active_global_call(&self) -> Option<GlobalAnyActiveCall> {
        self.active_call.as_ref().map(|(call, _)| call.clone())
    }

    fn on_active_call_event(
        &mut self,
        event: &ActiveCallEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActiveCallEvent::ParticipantLocationChanged { participant_id } => {
                self.leader_updated(participant_id, window, cx);
            }
            ActiveCallEvent::RemoteVideoTracksChanged { participant_id } => {
                self.leader_updated(participant_id, window, cx);
                self.handle_auto_watch_video_tracks_changed(*participant_id, window, cx);
            }
            ActiveCallEvent::LocalScreenShareStarted => {
                if let AutoWatch::Active { .. } = self.auto_watch {
                    self.auto_watch = AutoWatch::Paused;
                    cx.notify();
                }
            }
            ActiveCallEvent::LocalScreenShareStopped => {
                self.handle_auto_watch_local_share_stopped(window, cx);
            }
            ActiveCallEvent::RoomLeft => {
                if self.auto_watch.enabled() {
                    self.auto_watch = AutoWatch::Off;
                    cx.notify();
                }
            }
        }
    }

    pub fn database_id(&self) -> Option<WorkspaceId> {
        self.database_id
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_database_id(&mut self, id: WorkspaceId) {
        self.database_id = Some(id);
    }

    pub fn session_id(&self) -> Option<String> {
        self.session_id.clone()
    }

    fn save_window_bounds(&self, window: &mut Window, cx: &mut App) -> Task<()> {
        let Some(display) = window.display(cx) else {
            return Task::ready(());
        };
        let Ok(display_uuid) = display.uuid() else {
            return Task::ready(());
        };

        let window_bounds = window.inner_window_bounds();
        let database_id = self.database_id;
        let has_paths = !self.root_paths(cx).is_empty();
        let db = WorkspaceDb::global(cx);
        let kvp = db::kvp::KeyValueStore::global(cx);

        cx.background_executor().spawn(async move {
            if !has_paths {
                persistence::write_default_window_bounds(&kvp, window_bounds, display_uuid)
                    .await
                    .log_err();
            }
            if let Some(database_id) = database_id {
                db.set_window_open_status(
                    database_id,
                    SerializedWindowBounds(window_bounds),
                    display_uuid,
                )
                .await
                .log_err();
            } else {
                persistence::write_default_window_bounds(&kvp, window_bounds, display_uuid)
                    .await
                    .log_err();
            }
        })
    }

    /// Bypass the 200ms serialization throttle and write workspace state to
    /// the DB immediately. Returns a task the caller can await to ensure the
    /// write completes. Used by the quit handler so the most recent state
    /// isn't lost to a pending throttle timer when the process exits.
    pub fn flush_serialization(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self._schedule_serialize_workspace.take();
        self._serialize_workspace_task.take();
        self.bounds_save_task_queued.take();

        let bounds_task = self.save_window_bounds(window, cx);
        let serialize_task = self.serialize_workspace_internal(window, cx);
        cx.spawn(async move |_| {
            bounds_task.await;
            serialize_task.await;
        })
    }

    pub fn root_paths(&self, cx: &App) -> Vec<Arc<Path>> {
        let project = self.project().read(cx);
        project
            .visible_worktrees(cx)
            .map(|worktree| worktree.read(cx).abs_path())
            .collect::<Vec<_>>()
    }

    fn remove_panes(&mut self, member: Member, window: &mut Window, cx: &mut Context<Workspace>) {
        match member {
            Member::Axis(PaneAxis { members, .. }) => {
                for child in members.iter() {
                    self.remove_panes(child.clone(), window, cx)
                }
            }
            Member::Pane(pane) => {
                self.force_remove_pane(&pane, &None, window, cx);
            }
        }
    }

    fn remove_from_session(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self.session_id.take();
        self.serialize_workspace_internal(window, cx)
    }

    fn force_remove_pane(
        &mut self,
        pane: &Entity<Pane>,
        focus_on: &Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let removing_active_pane = self.active_pane() == pane;
        self.panes.retain(|p| p != pane);
        if let Some(focus_on) = focus_on {
            if removing_active_pane {
                self.set_active_pane(focus_on, window, cx);
            }
            focus_on.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        } else if removing_active_pane {
            let fallback_pane = self.panes.last().unwrap().clone();
            self.set_active_pane(&fallback_pane, window, cx);
            if !self.has_active_modal(window, cx) {
                fallback_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }
        }
        if self.last_active_center_pane == Some(pane.downgrade()) {
            self.last_active_center_pane = None;
        }
        cx.notify();
    }

    fn serialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._schedule_serialize_workspace.is_none() {
            self._schedule_serialize_workspace =
                Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(SERIALIZATION_THROTTLE_TIME)
                        .await;
                    this.update_in(cx, |this, window, cx| {
                        this._serialize_workspace_task =
                            Some(this.serialize_workspace_internal(window, cx));
                        this._schedule_serialize_workspace.take();
                    })
                    .log_err();
                }));
        }
    }

    fn serialize_workspace_internal(&self, window: &mut Window, cx: &mut App) -> Task<()> {
        let Some(database_id) = self.database_id() else {
            return Task::ready(());
        };

        fn serialize_pane_handle(
            pane_handle: &Entity<Pane>,
            window: &mut Window,
            cx: &mut App,
        ) -> SerializedPane {
            let (items, active, pinned_count, pane_kind, visible) = {
                let pane = pane_handle.read(cx);
                let active_item_id = pane.active_item().map(|item| item.item_id());
                (
                    pane.items()
                        .filter_map(|handle| {
                            let handle = handle.to_serializable_item_handle(cx)?;

                            Some(SerializedItem {
                                kind: Arc::from(handle.serialized_item_kind()),
                                item_id: handle.item_id().as_u64(),
                                active: Some(handle.item_id()) == active_item_id,
                                preview: pane.is_active_preview_item(handle.item_id()),
                            })
                        })
                        .collect::<Vec<_>>(),
                    pane.has_focus(window, cx),
                    pane.pinned_count(),
                    pane.pane_kind(),
                    pane.is_visible(),
                )
            };

            if pane_kind.is_tabbed() {
                SerializedPane::new(items, active, pinned_count).with_visible(visible)
            } else {
                SerializedPane::new_with_kind(items, active, pinned_count, pane_kind)
                    .with_visible(visible)
            }
        }

        fn build_serialized_pane_group(
            pane_group: &Member,
            window: &mut Window,
            cx: &mut App,
        ) -> Option<SerializedPaneGroup> {
            match pane_group {
                Member::Axis(PaneAxis {
                    axis,
                    members,
                    flexes,
                    bounding_boxes: _,
                }) => {
                    let children = members
                        .iter()
                        .filter_map(|member| build_serialized_pane_group(member, window, cx))
                        .collect::<Vec<_>>();

                    match children.len() {
                        0 => None,
                        1 => children.into_iter().next(),
                        _ => Some(SerializedPaneGroup::Group {
                            axis: SerializedAxis(*axis),
                            flexes: Some(flexes.lock().clone()),
                            children,
                        }),
                    }
                }
                Member::Pane(pane_handle) => Some(SerializedPaneGroup::Pane(
                    serialize_pane_handle(pane_handle, window, cx),
                )),
            }
        }

        fn build_serialized_docks(
            this: &Workspace,
            window: &mut Window,
            cx: &mut App,
        ) -> DockStructure {
            this.capture_dock_state(window, cx)
        }

        match self.workspace_location(cx) {
            WorkspaceLocation::Location(location, paths) => {
                let bookmarks = self.project.update(cx, |project, cx| {
                    project
                        .bookmark_store()
                        .read(cx)
                        .all_serialized_bookmarks(cx)
                });

                let breakpoints = self.project.update(cx, |project, cx| {
                    project
                        .breakpoint_store()
                        .read(cx)
                        .all_source_breakpoints(cx)
                });
                let user_toolchains = self
                    .project
                    .read(cx)
                    .user_toolchains(cx)
                    .unwrap_or_default();

                let center_group = build_serialized_pane_group(&self.center.root, window, cx)
                    .unwrap_or_else(|| SerializedPaneGroup::Pane(SerializedPane::default()));
                let docks = build_serialized_docks(self, window, cx);
                let window_bounds = Some(SerializedWindowBounds(window.window_bounds()));
                let identity_paths_hint = self.project_group_key(cx).path_list().clone();

                let serialized_workspace = SerializedWorkspace {
                    id: database_id,
                    location,
                    paths,
                    identity_paths: Some(identity_paths_hint),
                    center_group,
                    window_bounds,
                    display: Default::default(),
                    docks,
                    centered_layout: self.centered_layout,
                    session_id: self.session_id.clone(),
                    bookmarks,
                    breakpoints,
                    window_id: Some(window.window_handle().window_id().as_u64()),
                    user_toolchains,
                };

                let db = WorkspaceDb::global(cx);
                window.spawn(cx, async move |_| {
                    db.save_workspace(serialized_workspace).await;
                })
            }
            WorkspaceLocation::DetachFromSession => {
                let window_bounds = SerializedWindowBounds(window.window_bounds());
                let display = window.display(cx).and_then(|d| d.uuid().ok());
                // Save dock state for empty local workspaces
                let docks = build_serialized_docks(self, window, cx);
                let db = WorkspaceDb::global(cx);
                let kvp = db::kvp::KeyValueStore::global(cx);
                window.spawn(cx, async move |_| {
                    db.set_window_open_status(
                        database_id,
                        window_bounds,
                        display.unwrap_or_default(),
                    )
                    .await
                    .log_err();
                    db.set_session_id(database_id, None).await.log_err();
                    persistence::write_default_dock_state(&kvp, docks)
                        .await
                        .log_err();
                })
            }
            WorkspaceLocation::None => {
                // Save dock state for empty non-local workspaces
                let docks = build_serialized_docks(self, window, cx);
                let kvp = db::kvp::KeyValueStore::global(cx);
                window.spawn(cx, async move |_| {
                    persistence::write_default_dock_state(&kvp, docks)
                        .await
                        .log_err();
                })
            }
        }
    }

    fn has_any_items_open(&self, cx: &App) -> bool {
        self.panes
            .iter()
            .any(|pane| pane.read(cx).is_tabbed() && pane.read(cx).items_len() > 0)
    }

    fn workspace_location(&self, cx: &App) -> WorkspaceLocation {
        let paths = PathList::new(&self.root_paths(cx));
        if let Some(connection) = self.project.read(cx).remote_connection_options(cx) {
            WorkspaceLocation::Location(SerializedWorkspaceLocation::Remote(connection), paths)
        } else if self.project.read(cx).is_local() {
            if !paths.is_empty() || self.has_any_items_open(cx) {
                WorkspaceLocation::Location(SerializedWorkspaceLocation::Local, paths)
            } else {
                WorkspaceLocation::DetachFromSession
            }
        } else {
            WorkspaceLocation::None
        }
    }

    fn update_history(&self, cx: &mut App) {
        let Some(id) = self.database_id() else {
            return;
        };
        if !self.project.read(cx).is_local() {
            return;
        }
        if let Some(manager) = HistoryManager::global(cx) {
            let paths = PathList::new(&self.root_paths(cx));
            manager.update(cx, |this, cx| {
                this.update_history(id, HistoryManagerEntry::new(id, &paths), cx);
            });
        }
    }

    async fn serialize_items(
        this: &WeakEntity<Self>,
        items_rx: UnboundedReceiver<Box<dyn SerializableItemHandle>>,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        const CHUNK_SIZE: usize = 200;

        let mut serializable_items = items_rx.ready_chunks(CHUNK_SIZE);

        while let Some(items_received) = serializable_items.next().await {
            let unique_items =
                items_received
                    .into_iter()
                    .fold(HashMap::default(), |mut acc, item| {
                        acc.entry(item.item_id()).or_insert(item);
                        acc
                    });

            // We use into_iter() here so that the references to the items are moved into
            // the tasks and not kept alive while we're sleeping.
            for (_, item) in unique_items.into_iter() {
                if let Ok(Some(task)) = this.update_in(cx, |workspace, window, cx| {
                    item.serialize(workspace, false, window, cx)
                }) {
                    cx.background_spawn(async move { task.await.log_err() })
                        .detach();
                }
            }

            cx.background_executor()
                .timer(SERIALIZATION_THROTTLE_TIME)
                .await;
        }

        Ok(())
    }

    pub(crate) fn enqueue_item_serialization(
        &mut self,
        item: Box<dyn SerializableItemHandle>,
    ) -> Result<()> {
        self.serializable_items_tx
            .unbounded_send(item)
            .map_err(|err| anyhow!("failed to send serializable item over channel: {err}"))
    }

    pub(crate) fn load_workspace(
        serialized_workspace: SerializedWorkspace,
        paths_to_open: Vec<Option<ProjectPath>>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<Result<Vec<Option<Box<dyn ItemHandle>>>>> {
        cx.spawn_in(window, async move |workspace, cx| {
            let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;

            let mut center_group = None;
            let mut center_items = None;

            // Traverse the splits tree and add to things
            if let Some((group, active_pane, items)) = serialized_workspace
                .center_group
                .deserialize(&project, serialized_workspace.id, workspace.clone(), cx)
                .await
            {
                center_items = Some(items);
                center_group = Some((group, active_pane))
            }

            let mut items_by_project_path = HashMap::default();
            let mut item_ids_by_kind = HashMap::default();
            let mut all_deserialized_items = Vec::default();
            cx.update(|_, cx| {
                for item in center_items.unwrap_or_default().into_iter().flatten() {
                    if let Some(serializable_item_handle) = item.to_serializable_item_handle(cx) {
                        item_ids_by_kind
                            .entry(serializable_item_handle.serialized_item_kind())
                            .or_insert(Vec::new())
                            .push(item.item_id().as_u64() as ItemId);
                    }

                    if let Some(project_path) = item.project_path(cx) {
                        items_by_project_path.insert(project_path, item.clone());
                    }
                    all_deserialized_items.push(item);
                }
            })?;

            let opened_items = paths_to_open
                .into_iter()
                .map(|path_to_open| {
                    path_to_open
                        .and_then(|path_to_open| items_by_project_path.remove(&path_to_open))
                })
                .collect::<Vec<_>>();

            // Remove old panes from workspace panes list
            workspace.update_in(cx, |workspace, window, cx| {
                if let Some((center_group, active_pane)) = center_group {
                    workspace.remove_panes(workspace.center.root.clone(), window, cx);

                    // Swap workspace center group
                    workspace.center = PaneGroup::with_root(center_group);
                    workspace.center.set_is_center(true);
                    workspace.center.mark_positions(cx);

                    if let Some(active_pane) = active_pane {
                        workspace.set_active_pane(&active_pane, window, cx);
                        cx.focus_self(window);
                    } else {
                        workspace.set_active_pane(&workspace.center.first_pane(), window, cx);
                    }
                }

                let docks = serialized_workspace.docks;

                for (dock, serialized_dock) in [
                    (&mut workspace.right_dock, docks.right),
                    (&mut workspace.left_dock, docks.left),
                ]
                .iter_mut()
                {
                    dock.update(cx, |dock, cx| {
                        dock.serialized_dock = Some(serialized_dock.clone());
                        dock.restore_state(window, cx);
                    });
                }

                workspace.sync_panel_panes_from_docks(window, cx);
                workspace.ensure_visible_center_pane(window, cx);
                cx.notify();
            })?;

            project
                .update(cx, |project, cx| {
                    project.bookmark_store().update(cx, |bookmark_store, cx| {
                        bookmark_store.load_serialized_bookmarks(serialized_workspace.bookmarks, cx)
                    })
                })
                .await
                .log_err();

            let _ = project
                .update(cx, |project, cx| {
                    project
                        .breakpoint_store()
                        .update(cx, |breakpoint_store, cx| {
                            breakpoint_store
                                .with_serialized_breakpoints(serialized_workspace.breakpoints, cx)
                        })
                })
                .await;

            // Clean up all the items that have _not_ been loaded. Our ItemIds aren't stable. That means
            // after loading the items, we might have different items and in order to avoid
            // the database filling up, we delete items that haven't been loaded now.
            //
            // The items that have been loaded, have been saved after they've been added to the workspace.
            let clean_up_tasks = workspace.update_in(cx, |_, window, cx| {
                item_ids_by_kind
                    .into_iter()
                    .map(|(item_kind, loaded_items)| {
                        SerializableItemRegistry::cleanup(
                            item_kind,
                            serialized_workspace.id,
                            loaded_items,
                            window,
                            cx,
                        )
                        .log_err()
                    })
                    .collect::<Vec<_>>()
            })?;

            futures::future::join_all(clean_up_tasks).await;

            workspace
                .update_in(cx, |workspace, window, cx| {
                    // Serialize ourself to make sure our timestamps and any pane / item changes are replicated
                    workspace.serialize_workspace_internal(window, cx).detach();

                    // Ensure that we mark the window as edited if we did load dirty items
                    workspace.update_window_edited(window, cx);
                })
                .ok();

            Ok(opened_items)
        })
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

    fn adjust_padding(padding: Option<f32>) -> f32 {
        padding
            .unwrap_or(CenteredPaddingSettings::default().0)
            .clamp(
                CenteredPaddingSettings::MIN_PADDING,
                CenteredPaddingSettings::MAX_PADDING,
            )
    }

    pub fn for_window(window: &Window, cx: &App) -> Option<Entity<Workspace>> {
        window
            .root::<MultiWorkspace>()
            .flatten()
            .map(|multi_workspace| multi_workspace.read(cx).workspace().clone())
    }

    pub fn zoomed_item(&self) -> Option<&AnyWeakView> {
        self.zoomed.as_ref()
    }

    pub fn activate_next_window(&mut self, cx: &mut Context<Self>) {
        let Some(current_window_id) = cx.active_window().map(|a| a.window_id()) else {
            return;
        };
        let windows = cx.windows();
        let next_window =
            SystemWindowTabController::get_next_tab_group_window(cx, current_window_id).or_else(
                || {
                    windows
                        .iter()
                        .cycle()
                        .skip_while(|window| window.window_id() != current_window_id)
                        .nth(1)
                },
            );

        if let Some(window) = next_window {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
    }

    pub fn activate_previous_window(&mut self, cx: &mut Context<Self>) {
        let Some(current_window_id) = cx.active_window().map(|a| a.window_id()) else {
            return;
        };
        let windows = cx.windows();
        let prev_window =
            SystemWindowTabController::get_prev_tab_group_window(cx, current_window_id).or_else(
                || {
                    windows
                        .iter()
                        .rev()
                        .cycle()
                        .skip_while(|window| window.window_id() != current_window_id)
                        .nth(1)
                },
            );

        if let Some(window) = prev_window {
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
    }

    pub fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if cx.stop_active_drag(window) {
        } else if let Some((notification_id, _)) = self.notifications.pop() {
            dismiss_app_notification(&notification_id, cx);
        } else {
            cx.propagate();
        }
    }

    fn resize_left_dock(&mut self, new_size: Pixels, window: &mut Window, cx: &mut App) {
        let workspace_width = self.bounds.size.width;
        let mut size = new_size.min(workspace_width - RESIZE_HANDLE_SIZE);

        self.right_dock.read_with(cx, |right_dock, cx| {
            let right_dock_size = right_dock
                .stored_active_panel_size(window, cx)
                .unwrap_or(Pixels::ZERO);
            if right_dock_size + size > workspace_width {
                size = workspace_width - right_dock_size
            }
        });

        let flex_grow = self.dock_flex_for_size(DockPosition::Left, size, window, cx);
        self.left_dock.update(cx, |left_dock, cx| {
            if WorkspaceSettings::get_global(cx)
                .resize_all_panels_in_dock
                .contains(&DockPosition::Left)
            {
                left_dock.resize_all_panels(Some(size), flex_grow, window, cx);
            } else {
                left_dock.resize_active_panel(Some(size), flex_grow, window, cx);
            }
        });
    }

    fn resize_right_dock(&mut self, new_size: Pixels, window: &mut Window, cx: &mut App) {
        let workspace_width = self.bounds.size.width;
        let mut size = new_size.min(workspace_width - RESIZE_HANDLE_SIZE);
        self.left_dock.read_with(cx, |left_dock, cx| {
            let left_dock_size = left_dock
                .stored_active_panel_size(window, cx)
                .unwrap_or(Pixels::ZERO);
            if left_dock_size + size > workspace_width {
                size = workspace_width - left_dock_size
            }
        });
        let flex_grow = self.dock_flex_for_size(DockPosition::Right, size, window, cx);
        self.right_dock.update(cx, |right_dock, cx| {
            if WorkspaceSettings::get_global(cx)
                .resize_all_panels_in_dock
                .contains(&DockPosition::Right)
            {
                right_dock.resize_all_panels(Some(size), flex_grow, window, cx);
            } else {
                right_dock.resize_active_panel(Some(size), flex_grow, window, cx);
            }
        });
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
