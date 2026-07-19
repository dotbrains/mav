use std::rc::Rc;

use acp_thread::{AcpThread, ThreadStatus};
use agent::{NativeAgentServer, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
use agent_servers::AgentServer;
use anyhow::{Context as _, Result, anyhow};
use collections::HashMap;
use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use feature_flags::{CreateThreadToolFeatureFlag, FeatureFlagAppExt as _};
use gpui::{
    AnyElement, App, AsyncApp, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    Global, Render, SharedString, Subscription, Task, WeakEntity, Window,
};
use language_model::LanguageModelRegistry;
use project::{AgentId, Project};
use ui::{Color, Icon, IconName, IconSize, IntoElement, Label, LabelCommon, prelude::*};
use workspace::{
    ItemId, PathList, SaveIntent, SerializableItem, Workspace, WorkspaceId, delete_unloaded_items,
    item::{Item, ItemEvent, TabContentParams},
};

use crate::{
    Agent, AgentInitialContent, AgentThreadSource, ConversationView,
    agent_connection_store::AgentConnectionStore,
    agent_panel::apply_native_model_override,
    conversation_view::{ConversationTitleUpdated, RootThreadUpdated, StateChange},
    draft_prompt_store,
    thread_metadata_store::{ThreadId, ThreadMetadata, ThreadMetadataStore},
};

#[derive(Default)]
struct AgentThreadItemState {
    connection_stores: HashMap<EntityId, WeakEntity<AgentConnectionStore>>,
}

impl Global for AgentThreadItemState {}

pub struct AgentThreadItem {
    conversation_view: Entity<ConversationView>,
    window: gpui::AnyWindowHandle,
    focus_handle: FocusHandle,
    tab_title: SharedString,
    tab_is_draft: bool,
    _subscriptions: Vec<Subscription>,
}

impl AgentThreadItem {
    fn new(
        conversation_view: Entity<ConversationView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus_in(&focus_handle, window, |this: &mut Self, window, cx| {
            if this.focus_handle.is_focused(window) {
                this.conversation_view.focus_handle(cx).focus(window, cx);
            }
        })
        .detach();

        let subscriptions = vec![
            cx.subscribe(&conversation_view, |this, _, _: &StateChange, cx| {
                this.emit_tab_update(cx);
            }),
            cx.subscribe(&conversation_view, |this, _, _: &RootThreadUpdated, cx| {
                this.install_sibling_thread_host(cx);
                this.emit_tab_update(cx);
            }),
            cx.subscribe(
                &conversation_view,
                |this, _, _: &ConversationTitleUpdated, cx| {
                    this.emit_tab_update_if_title_changed(cx);
                },
            ),
            cx.observe(&ThreadMetadataStore::global(cx), |this, _store, cx| {
                this.emit_tab_update_if_title_changed(cx);
            }),
        ];

        let tab_title = conversation_view.read(cx).title(cx);
        let tab_is_draft = conversation_view.read(cx).is_draft(cx);
        let this = Self {
            conversation_view,
            window: window.window_handle(),
            focus_handle,
            tab_title,
            tab_is_draft,
            _subscriptions: subscriptions,
        };
        this.install_sibling_thread_host(cx);
        this
    }

    fn emit_tab_update(&mut self, cx: &mut Context<Self>) {
        self.tab_title = self.conversation_view.read(cx).title(cx);
        self.tab_is_draft = self.conversation_view.read(cx).is_draft(cx);
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    fn emit_tab_update_if_title_changed(&mut self, cx: &mut Context<Self>) {
        let title = self.conversation_view.read(cx).title(cx);
        let is_draft = self.conversation_view.read(cx).is_draft(cx);
        if self.tab_title == title && self.tab_is_draft == is_draft {
            return;
        }

        self.tab_title = title;
        self.tab_is_draft = is_draft;
        cx.emit(ItemEvent::UpdateTab);
        cx.notify();
    }

    pub fn conversation_view(&self) -> Entity<ConversationView> {
        self.conversation_view.clone()
    }

    pub fn thread_id(&self, cx: &App) -> ThreadId {
        self.conversation_view.read(cx).parent_id()
    }

    pub fn root_thread(&self, cx: &App) -> Option<Entity<AcpThread>> {
        self.conversation_view.read(cx).root_thread(cx)
    }

    pub fn session_id(&self, cx: &App) -> Option<acp::SessionId> {
        self.root_thread(cx)
            .map(|thread| thread.read(cx).session_id().clone())
    }

    pub fn cancel_thread(&self, cx: &mut Context<Self>) {
        self.conversation_view.update(cx, |conversation_view, cx| {
            conversation_view.cancel_generation(cx)
        });
    }

    pub fn is_draft(&self, cx: &App) -> bool {
        self.conversation_view.read(cx).is_draft(cx)
    }

    fn agent_icon(&self, cx: &App) -> IconName {
        self.conversation_view
            .read(cx)
            .root_thread_view()
            .map(|thread_view| thread_view.read(cx).agent_icon)
            .unwrap_or_else(|| {
                if self.conversation_view.read(cx).agent_key().is_native() {
                    IconName::ZedAgent
                } else {
                    IconName::Sparkle
                }
            })
    }

    fn agent_icon_from_external_svg(&self, cx: &App) -> Option<SharedString> {
        if let Some(icon_path) = self
            .conversation_view
            .read(cx)
            .root_thread_view()
            .and_then(|thread_view| thread_view.read(cx).agent_icon_from_external_svg.clone())
        {
            return Some(icon_path);
        }

        let agent_id = self.conversation_view.read(cx).agent_key().id();
        let project = self
            .conversation_view
            .read(cx)
            .workspace()
            .upgrade()?
            .read(cx)
            .project()
            .clone();
        let agent_server_store = project.read(cx).agent_server_store().clone();

        agent_server_store
            .read(cx)
            .agent_icon(&agent_id)
            .or_else(|| {
                project::AgentRegistryStore::try_global(cx).and_then(|store| {
                    store
                        .read(cx)
                        .agent(&agent_id)
                        .and_then(|agent| agent.icon_path().cloned())
                })
            })
    }

    fn install_sibling_thread_host(&self, cx: &mut Context<Self>) {
        if !cx.has_flag::<CreateThreadToolFeatureFlag>() {
            return;
        }
        let Some(native_connection) = self.conversation_view.read(cx).as_native_connection(cx)
        else {
            return;
        };
        let workspace = self.conversation_view.read(cx).workspace();
        let host = Rc::new(AgentThreadSiblingHost {
            workspace,
            window: self.window,
        }) as Rc<dyn agent::SiblingThreadHost>;
        native_connection.0.update(cx, |native_agent, _cx| {
            native_agent.set_sibling_thread_host(host);
        });
    }

    pub fn active_thread_info(&self, cx: &App) -> Option<AgentThreadInfo> {
        let conversation_view = self.conversation_view.read(cx);
        if conversation_view.is_draft(cx) {
            return None;
        }
        let has_pending_tool_call = conversation_view.root_thread_has_pending_tool_call(cx);
        let thread_id = conversation_view.parent_id();
        let thread_view = conversation_view.root_thread_view()?;
        let thread_view = thread_view.read(cx);
        let thread = thread_view.thread.read(cx);
        let title = conversation_view.title(cx);

        let status = if has_pending_tool_call {
            AgentThreadStatus::WaitingForConfirmation
        } else if thread.had_error() {
            AgentThreadStatus::Error
        } else {
            match thread.status() {
                ThreadStatus::Generating => AgentThreadStatus::Running,
                ThreadStatus::Idle => AgentThreadStatus::Completed,
            }
        };

        Some(AgentThreadInfo {
            thread_id,
            session_id: thread.session_id().clone(),
            title,
            status,
            icon: thread_view.agent_icon,
            icon_from_external_svg: thread_view.agent_icon_from_external_svg.clone(),
            is_title_generating: thread_view
                .as_native_thread(cx)
                .is_some_and(|native_thread| native_thread.read(cx).is_generating_title()),
            diff_stats: thread.action_log().read(cx).diff_stats(cx),
        })
    }
}

impl EventEmitter<ItemEvent> for AgentThreadItem {}

impl Focusable for AgentThreadItem {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for AgentThreadItem {
    type Event = ItemEvent;

    fn include_in_nav_history() -> bool {
        false
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let color = if params.selected {
            Color::Default
        } else {
            Color::Muted
        };

        Label::new(self.tab_content_text(0, cx))
            .color(color)
            .into_any_element()
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.conversation_view.read(cx).title(cx)
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        let icon = if self.is_draft(cx) {
            IconName::Robot
        } else {
            self.agent_icon(cx)
        };

        Some(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
    }

    fn tab_icon_element(&self, _window: &Window, cx: &App) -> Option<AnyElement> {
        if self.is_draft(cx) {
            return None;
        }

        self.agent_icon_from_external_svg(cx).map(|icon_path| {
            Icon::from_external_svg(icon_path)
                .size(IconSize::Small)
                .color(Color::Muted)
                .into_any_element()
        })
    }

    fn tab_close_icon(&self, _cx: &App) -> IconName {
        IconName::Close
    }

    fn tab_close_tooltip_text(&self) -> &'static str {
        if self.tab_is_draft {
            "Close Thread"
        } else {
            "Archive Thread"
        }
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        Some(self.conversation_view.read(cx).title(cx))
    }

    fn to_item_events(event: &ItemEvent, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }

    fn on_close(&mut self, save_intent: SaveIntent, cx: &mut Context<Self>) -> Task<Result<bool>> {
        if save_intent == SaveIntent::Close {
            let thread_id = self.conversation_view.read(cx).parent_id();
            if self.conversation_view.read(cx).is_draft(cx) {
                ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                    store.delete(thread_id, cx);
                });
            } else {
                ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                    store.archive(thread_id, None, cx);
                });
            }
        }
        Task::ready(Ok(true))
    }

    fn show_toolbar(&self) -> bool {
        false
    }
}

impl Render for AgentThreadItem {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .child(self.conversation_view.clone())
    }
}

impl SerializableItem for AgentThreadItem {
    fn serialized_item_kind() -> &'static str {
        "AgentThread"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let db = AgentThreadItemDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "agent_thread_items", &db, cx)
    }

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let db = AgentThreadItemDb::global(cx);
        let thread_id = match db.get_thread_id(item_id, workspace_id) {
            Ok(Some(thread_id)) => thread_id,
            Ok(None) => return Task::ready(Err(anyhow!("missing serialized agent thread item"))),
            Err(error) => return Task::ready(Err(error)),
        };
        let reload = ThreadMetadataStore::global(cx).read(cx).reload_task();

        window.spawn(cx, async move |cx| {
            reload.await;
            cx.update(|window, cx| -> Result<Entity<Self>> {
                let workspace = workspace.upgrade().context("workspace dropped")?;
                let metadata = ThreadMetadataStore::global(cx)
                    .read(cx)
                    .entry(thread_id)
                    .cloned()
                    .context("agent thread metadata missing")?;
                Ok(workspace.update(cx, |workspace, cx| {
                    build_agent_thread_item(
                        workspace,
                        project,
                        &metadata,
                        None,
                        AgentThreadSource::Sidebar,
                        window,
                        cx,
                    )
                }))
            })?
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let thread_id = self.conversation_view.read(cx).parent_id();
        let db = AgentThreadItemDb::global(cx);
        Some(cx.background_spawn(async move {
            db.save_thread_id(item_id, workspace_id, thread_id).await
        }))
    }

    fn should_serialize(&self, _event: &Self::Event) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct AgentThreadInfo {
    pub thread_id: ThreadId,
    pub session_id: acp::SessionId,
    pub title: SharedString,
    pub status: AgentThreadStatus,
    pub icon: IconName,
    pub icon_from_external_svg: Option<SharedString>,
    pub is_title_generating: bool,
    pub diff_stats: action_log::DiffStats,
}

pub use ui::AgentThreadStatus;

pub fn open_agent_thread_in_workspace(
    workspace: &Entity<Workspace>,
    metadata: &ThreadMetadata,
    focus: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AgentThreadItem> {
    workspace.update(cx, |workspace, cx| {
        open_agent_thread(workspace, metadata, focus, window, cx)
    })
}

pub fn create_agent_thread_in_workspace(
    workspace: &Entity<Workspace>,
    focus: bool,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<AgentThreadItem>> {
    workspace.update(cx, |workspace, cx| {
        create_agent_thread(
            workspace,
            Agent::NativeAgent,
            None,
            None,
            None,
            focus,
            None,
            AgentThreadSource::Sidebar,
            window,
            cx,
        )
    })
}

pub(crate) fn create_agent_thread(
    workspace: &mut Workspace,
    agent: Agent,
    title: Option<SharedString>,
    work_dirs: Option<PathList>,
    initial_content: Option<AgentInitialContent>,
    focus: bool,
    model_override: Option<String>,
    source: AgentThreadSource,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Option<Entity<AgentThreadItem>> {
    if workspace.root_paths(cx).is_empty() {
        return None;
    }

    let should_reuse_draft = title.is_none()
        && work_dirs.is_none()
        && initial_content.is_none()
        && model_override.is_none();
    if should_reuse_draft {
        let existing_draft = workspace
            .items_of_type::<AgentThreadItem>(cx)
            .find(|item| item.read(cx).is_draft(cx));

        if let Some(existing_draft) = existing_draft {
            workspace.activate_item(&existing_draft, true, focus, window, cx);
            return Some(existing_draft);
        }
    }

    let item = build_agent_thread_item_for_options(
        workspace,
        agent,
        None,
        None,
        work_dirs,
        title,
        initial_content,
        model_override,
        source,
        window,
        cx,
    );
    workspace.add_item_to_active_pane(Box::new(item.clone()), None, focus, window, cx);
    Some(item)
}

fn open_agent_thread(
    workspace: &mut Workspace,
    metadata: &ThreadMetadata,
    focus: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<AgentThreadItem> {
    let existing = workspace
        .items_of_type::<AgentThreadItem>(cx)
        .collect::<Vec<_>>()
        .into_iter()
        .find(|item| item.read(cx).thread_id(cx) == metadata.thread_id);
    if let Some(existing) = existing {
        workspace.activate_item(&existing, true, focus, window, cx);
        return existing;
    }

    ThreadMetadataStore::global(cx).update(cx, |store, cx| {
        store.unarchive(metadata.thread_id, cx);
    });

    let item = build_agent_thread_item(
        workspace,
        workspace.project().clone(),
        metadata,
        None,
        AgentThreadSource::Sidebar,
        window,
        cx,
    );
    workspace.add_item_to_active_pane(Box::new(item.clone()), None, focus, window, cx);
    item
}

fn build_agent_thread_item(
    workspace: &mut Workspace,
    _project: Entity<Project>,
    metadata: &ThreadMetadata,
    model_override: Option<String>,
    source: AgentThreadSource,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<AgentThreadItem> {
    let is_draft = metadata.is_draft();
    let initial_content = is_draft
        .then(|| draft_prompt_store::read(metadata.thread_id, cx))
        .flatten()
        .map(|blocks| AgentInitialContent::ContentBlock {
            blocks,
            auto_submit: false,
        });

    build_agent_thread_item_for_options(
        workspace,
        Agent::from(metadata.agent_id.clone()),
        None,
        Some(metadata.thread_id),
        Some(metadata.folder_paths().clone()),
        metadata.title(),
        initial_content,
        model_override,
        source,
        window,
        cx,
    )
}

fn build_agent_thread_item_for_options(
    workspace: &mut Workspace,
    agent: Agent,
    server_override: Option<Rc<dyn AgentServer>>,
    thread_id: Option<ThreadId>,
    work_dirs: Option<PathList>,
    title: Option<SharedString>,
    initial_content: Option<AgentInitialContent>,
    model_override: Option<String>,
    source: AgentThreadSource,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<AgentThreadItem> {
    let project = workspace.project().clone();
    let workspace_handle = workspace.weak_handle();
    let fs = workspace.app_state().fs.clone();
    let thread_store = ThreadStore::global(cx);
    let resume_session_id = thread_id.and_then(|thread_id| {
        ThreadMetadataStore::try_global(cx).and_then(|store| {
            store
                .read(cx)
                .entry(thread_id)
                .and_then(|m| m.session_id.clone())
        })
    });

    let server = server_override.unwrap_or_else(|| agent.server(fs, thread_store.clone()));
    let thread_store_for_view = server
        .clone()
        .downcast::<NativeAgentServer>()
        .is_some()
        .then(|| thread_store.clone());
    let connection_store = connection_store_for_project(&project, cx);

    let conversation_view = cx.new(|cx| {
        ConversationView::new(
            server,
            connection_store,
            agent,
            resume_session_id,
            thread_id,
            work_dirs,
            title,
            initial_content,
            workspace_handle,
            project,
            thread_store_for_view,
            source,
            window,
            cx,
        )
    });

    if let Some(model) = model_override {
        let applied = std::cell::Cell::new(false);
        cx.subscribe(
            &conversation_view,
            move |_item, view, _event: &RootThreadUpdated, cx| {
                if applied.get() {
                    return;
                }
                let Some(native_thread) = view.read(cx).as_native_thread(cx) else {
                    return;
                };
                apply_native_model_override(&native_thread, &model, cx);
                applied.set(true);
            },
        )
        .detach();
    }

    cx.new(|cx| AgentThreadItem::new(conversation_view, window, cx))
}

pub fn connection_store_for_project(
    project: &Entity<Project>,
    cx: &mut App,
) -> Entity<AgentConnectionStore> {
    let project_id = project.entity_id();
    if let Some(connection_store) = cx
        .default_global::<AgentThreadItemState>()
        .connection_stores
        .get(&project_id)
        .and_then(WeakEntity::upgrade)
    {
        return connection_store;
    }

    let connection_store = cx.new(|cx| AgentConnectionStore::new(project.clone(), cx));
    cx.default_global::<AgentThreadItemState>()
        .connection_stores
        .insert(project_id, connection_store.downgrade());
    connection_store
}

struct AgentThreadSiblingHost {
    workspace: WeakEntity<Workspace>,
    window: gpui::AnyWindowHandle,
}

impl agent::SiblingThreadHost for AgentThreadSiblingHost {
    fn create_sibling_thread(
        &self,
        request: agent::SiblingThreadRequest,
        cx: &mut AsyncApp,
    ) -> Task<Result<agent::SiblingThreadInfo>> {
        let workspace = self.workspace.clone();
        let window = self.window;
        cx.spawn(async move |cx| {
            let agent = resolve_requested_agent(&workspace, request.agent_id.as_deref(), cx)?;
            let initial_content = AgentInitialContent::ContentBlock {
                blocks: vec![acp::ContentBlock::Text(acp::TextContent::new(
                    request.prompt.clone(),
                ))],
                auto_submit: true,
            };
            let title: SharedString = request.title.clone();
            let mut worktree_warning = None;

            let target_workspace = if request.use_new_worktree {
                let workspace = workspace
                    .upgrade()
                    .ok_or_else(|| anyhow!("Source workspace is no longer available"))?;
                let branch_target = match request.base_ref.as_ref() {
                    Some(ref_name) => mav_actions::NewWorktreeBranchTarget::ExistingBranch {
                        name: ref_name.clone(),
                    },
                    None => mav_actions::NewWorktreeBranchTarget::CurrentBranch,
                };
                let action = mav_actions::CreateWorktree {
                    worktree_name: request.worktree_name.clone(),
                    branch_target,
                };
                let creation = window.update(cx, |_root, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        git_ui::worktree_service::create_worktree_workspace(
                            workspace, &action, window, None, cx,
                        )
                    })
                })?;
                let created = creation
                    .await
                    .context("failed to create worktree workspace")?;
                if created.consolidated_worktrees {
                    worktree_warning = Some(
                        "The project contained multiple worktrees backed by the same git \
                         repository, so they were consolidated into a single new worktree. \
                         The new thread's worktree is based on one of them and may not \
                         reflect the exact state of the others."
                            .to_string(),
                    );
                }
                created.workspace
            } else {
                workspace
                    .upgrade()
                    .ok_or_else(|| anyhow!("Source workspace is no longer available"))?
            };

            let resolved_agent = agent.clone();
            window.update(cx, |_root, window, cx| {
                target_workspace.update(cx, |workspace, cx| {
                    create_agent_thread(
                        workspace,
                        agent,
                        Some(title.clone()),
                        None,
                        Some(initial_content),
                        false,
                        request.model.clone(),
                        AgentThreadSource::Sidebar,
                        window,
                        cx,
                    );
                });
            })?;

            Ok(agent::SiblingThreadInfo {
                title,
                agent_id: resolved_agent.id().0.to_string(),
                model: request.model,
                warning: worktree_warning,
            })
        })
    }

    fn list_available_agents(&self, cx: &mut App) -> Result<agent::AvailableAgents> {
        let workspace = self
            .workspace
            .upgrade()
            .ok_or_else(|| anyhow!("Source workspace is no longer available"))?;
        let project = workspace.read(cx).project().clone();
        let mut agents = Vec::new();

        let native_models = {
            let registry = LanguageModelRegistry::read_global(cx);
            let default = registry.default_model();
            let mut models = Vec::new();
            for provider in registry.providers() {
                if !provider.is_authenticated(cx) {
                    continue;
                }
                let provider_id = provider.id();
                for model in provider.provided_models(cx) {
                    let id = format!("{}/{}", provider_id.0, model.id().0);
                    let is_default = default
                        .as_ref()
                        .map(|configured| {
                            configured.provider.id() == provider_id
                                && configured.model.id() == model.id()
                        })
                        .unwrap_or(false);
                    models.push(agent::AvailableModel {
                        id,
                        name: model.name().0,
                        is_default,
                    });
                }
            }
            models
        };
        agents.push(agent::AvailableAgent {
            id: agent::ZED_AGENT_ID.to_string(),
            name: Agent::NativeAgent.label(),
            is_native: true,
            models: native_models,
        });

        let agent_server_store = project.read(cx).agent_server_store().clone();
        let store = agent_server_store.read(cx);
        for agent_id in store.external_agents() {
            let display = store
                .agent_display_name(agent_id)
                .unwrap_or_else(|| agent_id.0.clone());
            agents.push(agent::AvailableAgent {
                id: agent_id.0.to_string(),
                name: display,
                is_native: false,
                models: Vec::new(),
            });
        }

        Ok(agent::AvailableAgents { agents })
    }
}

fn resolve_requested_agent(
    workspace: &WeakEntity<Workspace>,
    agent_id: Option<&str>,
    cx: &mut AsyncApp,
) -> Result<Agent> {
    match agent_id {
        None => Ok(Agent::NativeAgent),
        Some(id) if id == agent::ZED_AGENT_ID.as_ref() => Ok(Agent::NativeAgent),
        Some(id) => {
            let known = workspace
                .read_with(cx, |workspace, cx| {
                    let store = workspace.project().read(cx).agent_server_store().clone();
                    store
                        .read(cx)
                        .external_agents()
                        .any(|known_id| known_id.0.as_ref() == id)
                })
                .unwrap_or(false);
            if !known {
                return Err(anyhow!(
                    "Unknown agent id {id:?}. Call `list_agents_and_models` \
                     to see the agents available for `create_thread`."
                ));
            }
            Ok(Agent::Custom {
                id: AgentId(id.to_string().into()),
            })
        }
    }
}

struct AgentThreadItemDb(ThreadSafeConnection);

impl Domain for AgentThreadItemDb {
    const NAME: &'static str = stringify!(AgentThreadItemDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE agent_thread_items(
            workspace_id INTEGER,
            item_id INTEGER UNIQUE,
            thread_id BLOB NOT NULL,

            PRIMARY KEY(workspace_id, item_id),
            FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
            ON DELETE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(AgentThreadItemDb, [workspace::WorkspaceDb]);

impl AgentThreadItemDb {
    async fn save_thread_id(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<()> {
        self.write(move |connection| {
            let sql_stmt = sql!(
                INSERT OR REPLACE INTO agent_thread_items(item_id, workspace_id, thread_id)
                VALUES (?, ?, ?)
            );
            let mut query = connection.exec_bound::<(ItemId, WorkspaceId, ThreadId)>(sql_stmt)?;
            query((item_id, workspace_id, thread_id))
                .with_context(|| format!("failed to save agent thread item: {sql_stmt}"))
        })
        .await
    }

    query! {
        fn get_thread_id(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<ThreadId>> {
            SELECT thread_id FROM agent_thread_items WHERE item_id = ? AND workspace_id = ?
        }
    }
}
