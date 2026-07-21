use acp_thread::{
    AcpThread, AcpThreadEvent, AgentThreadEntry, AssistantMessage, AssistantMessageChunk,
    AuthRequired, ClientUserMessageId, LoadError, MaxOutputTokensError, MentionUri,
    PermissionOptionChoice, PermissionOptions, PermissionPattern, RetryStatus,
    SelectedPermissionOutcome, ThreadStatus, ToolCall, ToolCallContent, ToolCallStatus,
};
use acp_thread::{AgentConnection, Plan};
use action_log::{ActionLog, ActionLogTelemetry, DiffStats};
use agent::{NativeAgentServer, NoModelConfiguredError, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
#[cfg(test)]
use agent_servers::AgentServerDelegate;
use agent_servers::{AgentServer, GEMINI_TERMINAL_AUTH_METHOD_ID};
use agent_settings::{AgentProfileId, AgentSettings};
use anyhow::{Result, anyhow};
#[cfg(feature = "audio")]
use audio::{Audio, Sound};
use buffer_diff::BufferDiff;
use client::mav_urls;
use collections::{HashMap, HashSet, IndexMap};
use editor::scroll::Autoscroll;
use editor::{
    Editor, EditorEvent, EditorMode, MultiBuffer, PathKey, SelectionEffects, SizingBehavior,
};
use file_icons::FileIcons;
use fs::Fs;
use futures::FutureExt as _;
use gpui::{
    Action, Animation, AnimationExt, AnyView, App, ClickEvent, ClipboardItem, CursorStyle,
    ElementId, Empty, Entity, EventEmitter, FocusHandle, Focusable, Hsla, ListOffset, ListState,
    ObjectFit, PlatformDisplay, ScrollHandle, SharedString, StyledText, Subscription, Task,
    TextRun, TextStyle, WeakEntity, Window, WindowHandle, div, ease_in_out, img, linear_color_stop,
    linear_gradient, list, pulsating_between,
};
use itertools::Itertools;
use language::{Buffer, Language, Rope};
use language_model::{LanguageModelCompletionError, LanguageModelRegistry};
use markdown::{
    CodeBlockRenderer, CopyButtonVisibility, Markdown, MarkdownElement, MarkdownFont, MarkdownStyle,
};
use parking_lot::{Mutex, RwLock};
use project::{
    AgentId, AgentRegistryStore, AgentServerStore, Project, ProjectEntryId, ProjectPath,
};

use crate::message_editor::SessionCapabilities;
use crate::{AgentThreadSource, DEFAULT_THREAD_TITLE, resolve_agent_image};
use lru::LruCache;
use mav_actions::agent::{Chat, ToggleModelSelector};
use rope::Point;
use settings::{NotifyWhenAgentWaiting, Settings as _, SettingsStore};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use std::{rc::Rc, time::Duration};
use terminal_view::terminal_panel::TerminalPanel;
use text::Anchor;
use theme_settings::{AgentBufferFontSize, AgentUiFontSize};
use ui::{
    Callout, CircularProgress, CommonAnimationExt, ContextMenu, ContextMenuEntry, CopyButton,
    DecoratedIcon, DiffStat, Disclosure, Divider, DividerColor, IconDecoration, IconDecorationKind,
    KeyBinding, PopoverMenu, PopoverMenuHandle, TintColor, Tooltip, WithScrollbar, prelude::*,
    right_click_menu,
};
use util::{
    ResultExt, debug_panic, defer,
    paths::{PathStyle, PathWithPosition},
    rel_path::RelPath,
    size::format_file_size,
    time::duration_alt_display,
};
use workspace::{
    CollaboratorId, MultiWorkspace, NewTerminal, PathList, Workspace, path_link::sanitize_path_text,
};

use super::config_options::ConfigOptionsView;
use super::entry_view_state::EntryViewState;
use crate::ModeSelector;
use crate::ModelSelectorPopover;
use crate::agent_connection_store::{
    AgentConnectedState, AgentConnectionEntryEvent, AgentConnectionStore,
};
use crate::agent_diff::AgentDiff;
use crate::completion_provider::{AgentContextSelection, AvailableSkill};
use crate::entry_view_state::{EntryViewEvent, ViewEvent};
use crate::message_editor::{InputAttempt, MessageEditor, MessageEditorEvent};
use crate::profile_selector::{ProfileProvider, ProfileSelector};

use crate::thread_metadata_store::{ThreadId, ThreadMetadata, ThreadMetadataStore};
use crate::ui::{AgentNotification, AgentNotificationEvent};
use crate::{
    Agent, AgentDiffPane, AgentInitialContent, AgentPanel, AgentPanelEvent, AllowAlways, AllowOnce,
    AuthorizeToolCall, ClearMessageQueue, CycleFavoriteModels, CycleModeSelector,
    CycleThinkingEffort, EditFirstQueuedMessage, ExpandMessageEditor, Follow, KeepAll, NewThread,
    OpenAddContextMenu, OpenAgentDiff, RejectAll, RejectOnce, RemoveFirstQueuedMessage,
    ScrollOutputLineDown, ScrollOutputLineUp, ScrollOutputPageDown, ScrollOutputPageUp,
    ScrollOutputToBottom, ScrollOutputToNextMessage, ScrollOutputToPreviousMessage,
    ScrollOutputToTop, SendImmediately, SendNextQueuedMessage, ThreadTitleRegenerationResult,
    ToggleFastMode, ToggleProfileSelector, ToggleSteerFirstQueuedMessage, ToggleThinkingEffortMenu,
    ToggleThinkingMode, UndoLastReject,
};

const STOPWATCH_THRESHOLD: Duration = Duration::from_secs(30);
const TOKEN_THRESHOLD: u64 = 250;

pub(crate) const DRAFT_PROMPT_PERSIST_DEBOUNCE: Duration = Duration::from_millis(250);

mod message_queue;
mod thread_error;
mod thread_search_bar;
mod thread_view;
mod thread_view_builder;
mod view_helpers;
pub use message_queue::*;
pub(crate) use thread_error::ThreadError;
use thread_error::ThreadFeedback;
pub use thread_view::*;
use view_helpers::{
    loading_contents_spinner, native_available_skills, placeholder_text, plan_label_markdown_style,
};
mod auth_flow;
#[cfg(test)]
mod auth_flow_tests;
mod auth_rendering;
#[cfg(test)]
mod close_session_tests;
#[cfg(test)]
mod conversation_core_tests;
#[cfg(test)]
mod conversation_misc_tests;
#[cfg(test)]
mod conversation_permission_tests;
mod conversation_state;
use conversation_state::{Conversation, affects_thread_metadata};
#[cfg(test)]
use conversation_state::{permission_option_for_action, resolve_outcome_from_selection};
mod draft_state;
mod editor_insertion;
mod markdown_resolution;
use markdown_resolution::{AgentCodeSpanResolver, render_agent_markdown};
#[cfg(test)]
mod generation_control_tests;
mod load_error_rendering;
#[cfg(test)]
mod load_error_tests;
mod loading_draft_rendering;
#[cfg(test)]
mod message_editing_tests;
mod navigation_state;
mod notification;
#[cfg(test)]
mod notification_basic_tests;
#[cfg(test)]
mod notification_lifecycle_tests;
#[cfg(test)]
mod notification_sidebar_tests;
#[cfg(test)]
mod notification_visibility_tests;
#[cfg(test)]
mod permission_action_tests;
#[cfg(test)]
mod permission_button_tests;
#[cfg(test)]
mod permission_resolution_tests;
#[cfg(test)]
mod permission_row_tests;
#[cfg(test)]
mod queued_message_tests;
mod rendering;
mod server_state;
use server_state::{AuthState, ConnectedServerState, LoadingDraft, LoadingView, ServerState};
#[cfg(test)]
mod selection_insertion_tests;
mod session_lifecycle;
#[cfg(test)]
mod session_restore_tests;
mod settings_events;
mod thread_events;
#[cfg(test)]
mod thread_generation_tests;
#[cfg(test)]
mod thread_search_highlight_tests;
#[cfg(test)]
mod thread_search_navigation_tests;
#[cfg(test)]
mod thread_search_scroll_tests;
#[cfg(test)]
mod thread_search_tests;

impl ProfileProvider for Entity<agent::Thread> {
    fn profile_id(&self, cx: &App) -> AgentProfileId {
        self.read(cx).profile().clone()
    }

    fn set_profile(&self, profile_id: AgentProfileId, cx: &mut App) {
        self.update(cx, |thread, cx| {
            // Apply the profile and let the thread swap to its default model.
            thread.set_profile(profile_id, cx);
        });
    }

    fn profiles_supported(&self, cx: &App) -> bool {
        self.read(cx)
            .model()
            .is_some_and(|model| model.supports_tools())
    }

    fn model_selected(&self, cx: &App) -> bool {
        self.read(cx).model().is_some()
    }
}

pub(crate) struct RootThreadUpdated;

impl EventEmitter<RootThreadUpdated> for ConversationView {}

pub(crate) struct ConversationTitleUpdated;

impl EventEmitter<ConversationTitleUpdated> for ConversationView {}

pub struct StateChange;

impl EventEmitter<StateChange> for ConversationView {}

pub enum AcpServerViewEvent {
    ActiveThreadChanged,
}

impl EventEmitter<AcpServerViewEvent> for ConversationView {}

pub struct ConversationView {
    agent: Rc<dyn AgentServer>,
    connection_store: Entity<AgentConnectionStore>,
    connection_key: Agent,
    agent_server_store: Entity<AgentServerStore>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    thread_store: Option<Entity<ThreadStore>>,
    pub(crate) thread_id: ThreadId,
    pub(crate) root_session_id: Option<acp::SessionId>,
    started_as_draft: bool,
    server_state: ServerState,
    focus_handle: FocusHandle,
    notifications: Vec<WindowHandle<AgentNotification>>,
    notification_subscriptions: HashMap<WindowHandle<AgentNotification>, Vec<Subscription>>,
    auth_task: Option<Task<()>>,
    loading_status: Option<SharedString>,
    /// When settings change, use this to see if the theme has changed (which
    /// causes mermaid diagrams to re-render).
    last_theme_id: Option<String>,
    draft_prompt_persist_task: Option<Task<()>>,
    /// Cache + worktree snapshot for resolving paths in markdown code spans.
    /// Shared with the child [`ThreadView`] when one is constructed.
    pub(crate) code_span_resolver: AgentCodeSpanResolver,
    _subscriptions: Vec<Subscription>,
}

impl ConversationView {
    pub fn new(
        agent: Rc<dyn AgentServer>,
        connection_store: Entity<AgentConnectionStore>,
        connection_key: Agent,
        resume_session_id: Option<acp::SessionId>,
        thread_id: Option<ThreadId>,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        initial_content: Option<AgentInitialContent>,
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        thread_store: Option<Entity<ThreadStore>>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let agent_server_store = project.read(cx).agent_server_store().clone();
        let code_span_resolver = AgentCodeSpanResolver::new(&project.downgrade(), cx);
        let mut subscriptions = vec![
            cx.observe_global_in::<SettingsStore>(window, Self::agent_ui_font_size_changed),
            cx.observe_global_in::<SettingsStore>(window, Self::invalidate_mermaid_caches),
            cx.observe_global_in::<AgentUiFontSize>(window, Self::agent_ui_font_size_changed),
            cx.observe_global_in::<AgentBufferFontSize>(window, Self::agent_ui_font_size_changed),
            cx.subscribe_in(
                &agent_server_store,
                window,
                Self::handle_agent_servers_updated,
            ),
        ];
        subscriptions.push(cx.subscribe(&project, {
            let resolver = code_span_resolver.clone();
            move |_this: &mut Self, _project, event: &project::Event, cx| {
                if matches!(
                    event,
                    project::Event::WorktreeAdded(_)
                        | project::Event::WorktreeRemoved(_)
                        | project::Event::WorktreeUpdatedEntries(_, _)
                ) {
                    resolver.clear_cache();
                    cx.notify();
                }
            }
        }));

        cx.on_release(|this, cx| {
            if let Some(connected) = this.as_connected() {
                connected.close_all_sessions(cx).detach();
            }
            for window in this.notifications.drain(..) {
                window
                    .update(cx, |_, window, _| {
                        window.remove_window();
                    })
                    .ok();
            }
        })
        .detach();

        let thread_id = thread_id.unwrap_or_else(ThreadId::new);
        let started_as_draft = resume_session_id.is_none();
        if started_as_draft {
            Self::save_provisional_draft_metadata(thread_id, &connection_key, &project, cx);
        }

        Self {
            agent: agent.clone(),
            connection_store: connection_store.clone(),
            connection_key: connection_key.clone(),
            agent_server_store,
            workspace: workspace.clone(),
            project: project.clone(),
            thread_store: thread_store.clone(),
            thread_id,
            root_session_id: resume_session_id.clone(),
            started_as_draft,
            server_state: Self::initial_state(
                agent.clone(),
                connection_store,
                connection_key,
                resume_session_id,
                thread_id,
                work_dirs,
                title,
                project,
                workspace.clone(),
                thread_store.clone(),
                initial_content,
                source,
                window,
                cx,
            ),
            notifications: Vec::new(),
            notification_subscriptions: HashMap::default(),
            auth_task: None,
            loading_status: None,
            last_theme_id: Some(cx.theme().id.clone()),
            draft_prompt_persist_task: None,
            code_span_resolver,
            _subscriptions: subscriptions,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn agent_key(&self) -> &Agent {
        &self.connection_key
    }

    fn metadata_title(&self, cx: &App) -> Option<SharedString> {
        ThreadMetadataStore::try_global(cx)
            .and_then(|store| store.read(cx).entry(self.thread_id).and_then(|m| m.title()))
    }

    pub fn title(&self, cx: &App) -> SharedString {
        match &self.server_state {
            ServerState::Connected(view) => view.active_view().map_or_else(
                || DEFAULT_THREAD_TITLE.into(),
                |view| {
                    if self.is_draft(cx) {
                        DEFAULT_THREAD_TITLE.into()
                    } else {
                        let thread = view.read(cx).thread.clone();
                        let thread = thread.read(cx);
                        self.metadata_title(cx)
                            .or_else(|| thread.title())
                            .unwrap_or_else(|| DEFAULT_THREAD_TITLE.into())
                    }
                },
            ),
            ServerState::Loading { draft: Some(_), .. } => DEFAULT_THREAD_TITLE.into(),
            ServerState::Loading { .. } => self
                .loading_status
                .clone()
                .unwrap_or_else(|| "Loading…".into()),
            ServerState::LoadError { error, .. } => match error {
                LoadError::Unsupported { .. } => {
                    format!("Upgrade {}", self.agent.agent_id()).into()
                }
                LoadError::FailedToInstall(_) => {
                    format!("Failed to Install {}", self.agent.agent_id()).into()
                }
                LoadError::Exited { .. } => format!("{} Exited", self.agent.agent_id()).into(),
                LoadError::Other(_) => format!("Error Loading {}", self.agent.agent_id()).into(),
            },
        }
    }

    pub fn cancel_generation(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.active_thread() {
            active.update(cx, |active, cx| {
                active.cancel_generation(cx);
            });
        }
    }

    pub fn parent_id(&self) -> ThreadId {
        self.thread_id
    }

    pub(crate) fn workspace(&self) -> WeakEntity<Workspace> {
        self.workspace.clone()
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.server_state, ServerState::Loading { .. })
    }

    fn schedule_draft_prompt_persist(&mut self, cx: &mut Context<Self>) {
        let thread_id = self.thread_id;
        self.draft_prompt_persist_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(DRAFT_PROMPT_PERSIST_DEBOUNCE)
                .await;
            let persist = this.update(cx, |this, cx| {
                if !this.is_draft(cx) {
                    return None;
                }
                let thread = this.root_thread(cx)?;
                let thread = thread.read(cx);
                let snapshot: Vec<acp::ContentBlock> = thread
                    .draft_prompt()
                    .map(|p| p.to_vec())
                    .unwrap_or_default();
                Some(if snapshot.is_empty() {
                    crate::draft_prompt_store::delete(thread_id, cx)
                } else {
                    crate::draft_prompt_store::write(thread_id, &snapshot, cx)
                })
            });
            if let Ok(Some(persist)) = persist {
                persist.await.log_err();
            }
        }));
    }

    pub fn has_user_submitted_prompt(&self, cx: &App) -> bool {
        self.root_thread_view()
            .is_some_and(|active| active.read(cx).has_user_submitted_prompt(cx))
    }

    pub fn regenerate_thread_title(&self, cx: &mut App) -> ThreadTitleRegenerationResult {
        let Some(thread) = self.as_native_thread(cx) else {
            return ThreadTitleRegenerationResult::NotOpen;
        };
        let thread_id = self.parent_id();
        thread.update(cx, |thread, cx| {
            if thread.is_generating_title() {
                ThreadTitleRegenerationResult::AlreadyGenerating
            } else if thread.summarization_model().is_none() {
                ThreadTitleRegenerationResult::NoModel
            } else if thread.regenerate_title_with_callback(cx, move |title, cx| {
                ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                    store.set_generated_title(thread_id, title, cx);
                });
            }) {
                ThreadTitleRegenerationResult::Started
            } else {
                ThreadTitleRegenerationResult::AlreadyGenerating
            }
        })
    }
}

impl Focusable for ConversationView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if let Some(message_editor) = self.loading_draft_editor() {
            return message_editor.focus_handle(cx);
        }
        match self.active_thread() {
            Some(thread) => thread.read(cx).focus_handle(cx),
            None => self.focus_handle.clone(),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ConversationView {
    /// Expands a tool call so its content is visible.
    /// This is primarily useful for visual testing.
    pub fn expand_tool_call(&mut self, tool_call_id: acp::ToolCallId, cx: &mut Context<Self>) {
        if let Some(active) = self.active_thread() {
            active.update(cx, |active, cx| {
                active.entry_view_state.update(cx, |state, _cx| {
                    state.expand_tool_call(tool_call_id);
                });
            });
            cx.notify();
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_updated_at(&mut self, updated_at: Instant, cx: &mut Context<Self>) {
        let Some(connected) = self.as_connected_mut() else {
            return;
        };

        connected.conversation.update(cx, |conversation, _cx| {
            conversation.updated_at = Some(updated_at);
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use acp_thread::StubAgentConnection;
    use action_log::ActionLog;
    use agent::{AgentTool, EditFileTool, FetchTool, TerminalTool, ToolPermissionContext};
    use agent_servers::FakeAcpAgentServer;
    use editor::MultiBufferOffset;
    use editor::actions::Paste;
    use feature_flags::FeatureFlagAppExt as _;
    use fs::FakeFs;
    use gpui::{ClipboardItem, EventEmitter, TestAppContext, VisualTestContext, point, size};
    use parking_lot::Mutex;
    use project::Project;
    use serde_json::json;
    use settings::SettingsStore;
    use std::any::Any;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::Arc;
    use workspace::{Item, MultiWorkspace};

    use crate::agent_panel;
    use crate::completion_provider::AgentContextSource;
    use crate::test_support::register_test_sidebar;
    use crate::thread_metadata_store::ThreadMetadataStore;

    use super::*;

    #[derive(Clone)]
    struct RestoredAvailableCommandsConnection;

    impl AgentConnection for RestoredAvailableCommandsConnection {
        fn agent_id(&self) -> AgentId {
            AgentId::new("restored-available-commands")
        }

        fn telemetry_id(&self) -> SharedString {
            "restored-available-commands".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            _work_dirs: PathList,
            cx: &mut App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            let thread = build_test_thread(
                self,
                project,
                "RestoredAvailableCommandsConnection",
                acp::SessionId::new("new-session"),
                cx,
            );
            Task::ready(Ok(thread))
        }

        fn supports_load_session(&self) -> bool {
            true
        }

        fn load_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            _work_dirs: PathList,
            _title: Option<SharedString>,
            cx: &mut App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            let thread = build_test_thread(
                self,
                project,
                "RestoredAvailableCommandsConnection",
                session_id,
                cx,
            );

            thread
                .update(cx, |thread, cx| {
                    thread.handle_session_update(
                        acp::SessionUpdate::AvailableCommandsUpdate(
                            acp::AvailableCommandsUpdate::new(vec![acp::AvailableCommand::new(
                                "help", "Get help",
                            )]),
                        ),
                        cx,
                    )
                })
                .expect("available commands update should succeed");

            Task::ready(Ok(thread))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(
            &self,
            _method_id: acp::AuthMethodId,
            _cx: &mut App,
        ) -> Task<gpui::Result<()>> {
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            _params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<gpui::Result<acp::PromptResponse>> {
            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    async fn setup_conversation_view(
        agent: impl AgentServer + 'static,
        cx: &mut TestAppContext,
    ) -> (Entity<ConversationView>, &mut VisualTestContext) {
        setup_conversation_view_with_initial_content_opt(agent, None, cx).await
    }

    async fn setup_conversation_view_with_initial_content(
        agent: impl AgentServer + 'static,
        initial_content: AgentInitialContent,
        cx: &mut TestAppContext,
    ) -> (Entity<ConversationView>, &mut VisualTestContext) {
        setup_conversation_view_with_initial_content_opt(agent, Some(initial_content), cx).await
    }

    async fn setup_conversation_view_with_initial_content_opt(
        agent: impl AgentServer + 'static,
        initial_content: Option<AgentInitialContent>,
        cx: &mut TestAppContext,
    ) -> (Entity<ConversationView>, &mut VisualTestContext) {
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let agent_key = Agent::Custom { id: "Test".into() };

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(agent),
                    connection_store.clone(),
                    agent_key.clone(),
                    None,
                    None,
                    None,
                    None,
                    initial_content,
                    workspace.downgrade(),
                    project,
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();

        (conversation_view, cx)
    }

    fn add_to_workspace(conversation_view: Entity<ConversationView>, cx: &mut VisualTestContext) {
        let workspace =
            conversation_view.read_with(cx, |thread_view, _cx| thread_view.workspace.clone());

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.add_item_to_active_pane(
                    Box::new(cx.new(|_| ThreadViewItem(conversation_view.clone()))),
                    None,
                    true,
                    window,
                    cx,
                );
            })
            .unwrap();
    }

    struct ThreadViewItem(Entity<ConversationView>);

    impl Item for ThreadViewItem {
        type Event = ();

        fn include_in_nav_history() -> bool {
            false
        }

        fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
            "Test".into()
        }
    }

    impl EventEmitter<()> for ThreadViewItem {}

    impl Focusable for ThreadViewItem {
        fn focus_handle(&self, cx: &App) -> FocusHandle {
            self.0.read(cx).focus_handle(cx)
        }
    }

    impl Render for ThreadViewItem {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // Render the title editor in the element tree too. In the real app
            // it is part of the agent panel
            let title_editor = self
                .0
                .read(cx)
                .active_thread()
                .map(|t| t.read(cx).title_editor.clone());

            v_flex().children(title_editor).child(self.0.clone())
        }
    }

    pub(crate) struct StubAgentServer<C> {
        connection: C,
    }

    impl<C> StubAgentServer<C> {
        pub(crate) fn new(connection: C) -> Self {
            Self { connection }
        }
    }

    impl StubAgentServer<StubAgentConnection> {
        pub(crate) fn default_response() -> Self {
            let conn = StubAgentConnection::new();
            conn.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
                acp::ContentChunk::new("Default response".into()),
            )]);
            Self::new(conn)
        }
    }

    impl<C> AgentServer for StubAgentServer<C>
    where
        C: 'static + AgentConnection + Send + Clone,
    {
        fn logo(&self) -> ui::IconName {
            ui::IconName::MavAgent
        }

        fn agent_id(&self) -> AgentId {
            "Test".into()
        }

        fn connect(
            &self,
            _delegate: AgentServerDelegate,
            _project: Entity<Project>,
            _cx: &mut App,
        ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
            Task::ready(Ok(Rc::new(self.connection.clone())))
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    struct FailingAgentServer;

    impl AgentServer for FailingAgentServer {
        fn logo(&self) -> ui::IconName {
            ui::IconName::AiOpenAi
        }

        fn agent_id(&self) -> AgentId {
            AgentId::new("Codex CLI")
        }

        fn connect(
            &self,
            _delegate: AgentServerDelegate,
            _project: Entity<Project>,
            _cx: &mut App,
        ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
            Task::ready(Err(anyhow!(
                "extracting downloaded asset for \
                 https://github.com/mav-industries/codex-acp/releases/download/v0.9.4/\
                 codex-acp-0.9.4-aarch64-pc-windows-msvc.zip: \
                 failed to iterate over archive: Invalid gzip header"
            )))
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    /// Agent server whose `connect()` fails while `fail` is `true` and
    /// returns the wrapped connection otherwise. Used to simulate the
    /// race where an external agent isn't yet registered at startup.
    pub(crate) struct FlakyAgentServer {
        connection: StubAgentConnection,
        fail: Arc<std::sync::atomic::AtomicBool>,
    }

    impl FlakyAgentServer {
        pub(crate) fn new(
            connection: StubAgentConnection,
        ) -> (Self, Arc<std::sync::atomic::AtomicBool>) {
            let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
            (
                Self {
                    connection,
                    fail: fail.clone(),
                },
                fail,
            )
        }
    }

    impl AgentServer for FlakyAgentServer {
        fn logo(&self) -> ui::IconName {
            ui::IconName::MavAgent
        }

        fn agent_id(&self) -> AgentId {
            "Flaky".into()
        }

        fn connect(
            &self,
            _delegate: AgentServerDelegate,
            _project: Entity<Project>,
            _cx: &mut App,
        ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                Task::ready(Err(anyhow!(
                    "Custom agent server `Flaky` is not registered"
                )))
            } else {
                Task::ready(Ok(Rc::new(self.connection.clone())))
            }
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    fn build_test_thread(
        connection: Rc<dyn AgentConnection>,
        project: Entity<Project>,
        name: &'static str,
        session_id: acp::SessionId,
        cx: &mut App,
    ) -> Entity<AcpThread> {
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        cx.new(|cx| {
            AcpThread::new(
                None,
                Some(name.into()),
                None,
                connection,
                project,
                action_log,
                session_id,
                watch::Receiver::constant(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                ),
                cx,
            )
        })
    }

    #[derive(Clone)]
    struct ResumeOnlyAgentConnection;

    impl AgentConnection for ResumeOnlyAgentConnection {
        fn agent_id(&self) -> AgentId {
            AgentId::new("resume-only")
        }

        fn telemetry_id(&self) -> SharedString {
            "resume-only".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            _work_dirs: PathList,
            cx: &mut gpui::App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            let thread = build_test_thread(
                self,
                project,
                "ResumeOnlyAgentConnection",
                acp::SessionId::new("new-session"),
                cx,
            );
            Task::ready(Ok(thread))
        }

        fn supports_resume_session(&self) -> bool {
            true
        }

        fn resume_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            _work_dirs: PathList,
            _title: Option<SharedString>,
            cx: &mut App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            let thread =
                build_test_thread(self, project, "ResumeOnlyAgentConnection", session_id, cx);
            Task::ready(Ok(thread))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(
            &self,
            _method_id: acp::AuthMethodId,
            _cx: &mut App,
        ) -> Task<gpui::Result<()>> {
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            _params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<gpui::Result<acp::PromptResponse>> {
            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    /// Simulates an agent that requires authentication before a session can be
    /// created. `new_session` returns `AuthRequired` until `authenticate` is
    /// called with the correct method, after which sessions are created normally.
    #[derive(Clone)]
    struct AuthGatedAgentConnection {
        authenticated: Arc<Mutex<bool>>,
        auth_method: acp::AuthMethod,
    }

    impl AuthGatedAgentConnection {
        const AUTH_METHOD_ID: &str = "test-login";

        fn new() -> Self {
            Self {
                authenticated: Arc::new(Mutex::new(false)),
                auth_method: acp::AuthMethod::Agent(acp::AuthMethodAgent::new(
                    Self::AUTH_METHOD_ID,
                    "Test Login",
                )),
            }
        }
    }

    impl AgentConnection for AuthGatedAgentConnection {
        fn agent_id(&self) -> AgentId {
            AgentId::new("auth-gated")
        }

        fn telemetry_id(&self) -> SharedString {
            "auth-gated".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            work_dirs: PathList,
            cx: &mut gpui::App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            if !*self.authenticated.lock() {
                return Task::ready(Err(acp_thread::AuthRequired::new()
                    .with_description("Sign in to continue".to_string())
                    .into()));
            }

            let session_id = acp::SessionId::new("auth-gated-session");
            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            Task::ready(Ok(cx.new(|cx| {
                AcpThread::new(
                    None,
                    None,
                    Some(work_dirs),
                    self,
                    project,
                    action_log,
                    session_id,
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            })))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            std::slice::from_ref(&self.auth_method)
        }

        fn authenticate(
            &self,
            method_id: acp::AuthMethodId,
            _cx: &mut App,
        ) -> Task<gpui::Result<()>> {
            if &method_id == self.auth_method.id() {
                *self.authenticated.lock() = true;
                Task::ready(Ok(()))
            } else {
                Task::ready(Err(anyhow::anyhow!("Unknown auth method")))
            }
        }

        fn supports_logout(&self) -> bool {
            true
        }

        fn logout(&self, _cx: &mut App) -> Task<gpui::Result<()>> {
            *self.authenticated.lock() = false;
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            _params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<gpui::Result<acp::PromptResponse>> {
            unimplemented!()
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {
            unimplemented!()
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    /// Simulates a model which always returns a refusal response
    #[derive(Clone)]
    struct RefusalAgentConnection;

    impl AgentConnection for RefusalAgentConnection {
        fn agent_id(&self) -> AgentId {
            AgentId::new("refusal")
        }

        fn telemetry_id(&self) -> SharedString {
            "refusal".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            work_dirs: PathList,
            cx: &mut gpui::App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            Task::ready(Ok(cx.new(|cx| {
                let action_log = cx.new(|_| ActionLog::new(project.clone()));
                AcpThread::new(
                    None,
                    None,
                    Some(work_dirs),
                    self,
                    project,
                    action_log,
                    acp::SessionId::new("test"),
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            })))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(
            &self,
            _method_id: acp::AuthMethodId,
            _cx: &mut App,
        ) -> Task<gpui::Result<()>> {
            unimplemented!()
        }

        fn prompt(
            &self,
            _params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<gpui::Result<acp::PromptResponse>> {
            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::Refusal)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {
            unimplemented!()
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    #[derive(Clone)]
    struct CwdCapturingConnection {
        captured_work_dirs: Arc<Mutex<Option<PathList>>>,
    }

    impl CwdCapturingConnection {
        fn new() -> Self {
            Self {
                captured_work_dirs: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl AgentConnection for CwdCapturingConnection {
        fn agent_id(&self) -> AgentId {
            AgentId::new("cwd-capturing")
        }

        fn telemetry_id(&self) -> SharedString {
            "cwd-capturing".into()
        }

        fn new_session(
            self: Rc<Self>,
            project: Entity<Project>,
            work_dirs: PathList,
            cx: &mut gpui::App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            *self.captured_work_dirs.lock() = Some(work_dirs.clone());
            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            let thread = cx.new(|cx| {
                AcpThread::new(
                    None,
                    None,
                    Some(work_dirs),
                    self.clone(),
                    project,
                    action_log,
                    acp::SessionId::new("new-session"),
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            });
            Task::ready(Ok(thread))
        }

        fn supports_load_session(&self) -> bool {
            true
        }

        fn load_session(
            self: Rc<Self>,
            session_id: acp::SessionId,
            project: Entity<Project>,
            work_dirs: PathList,
            _title: Option<SharedString>,
            cx: &mut App,
        ) -> Task<gpui::Result<Entity<AcpThread>>> {
            *self.captured_work_dirs.lock() = Some(work_dirs.clone());
            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            let thread = cx.new(|cx| {
                AcpThread::new(
                    None,
                    None,
                    Some(work_dirs),
                    self.clone(),
                    project,
                    action_log,
                    session_id,
                    watch::Receiver::constant(
                        acp::PromptCapabilities::new()
                            .image(true)
                            .audio(true)
                            .embedded_context(true),
                    ),
                    cx,
                )
            });
            Task::ready(Ok(thread))
        }

        fn auth_methods(&self) -> &[acp::AuthMethod] {
            &[]
        }

        fn authenticate(
            &self,
            _method_id: acp::AuthMethodId,
            _cx: &mut App,
        ) -> Task<gpui::Result<()>> {
            Task::ready(Ok(()))
        }

        fn prompt(
            &self,
            _params: acp::PromptRequest,
            _cx: &mut App,
        ) -> Task<gpui::Result<acp::PromptResponse>> {
            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }

        fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    pub(crate) fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            // Use an isolated DB so parallel tests can't overwrite each
            // other's global keys (e.g. the last-created entry kind).
            cx.set_global(db::AppDatabase::test_new());
            ThreadMetadataStore::init_global(cx);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            agent_panel::init(cx);
            release_channel::init(semver::Version::new(0, 0, 0), cx);
            prompt_store::init(cx)
        });
    }

    fn active_thread(
        conversation_view: &Entity<ConversationView>,
        cx: &TestAppContext,
    ) -> Entity<ThreadView> {
        cx.read(|cx| {
            conversation_view
                .read(cx)
                .active_thread()
                .expect("No active thread")
                .clone()
        })
    }

    fn message_editor(
        conversation_view: &Entity<ConversationView>,
        cx: &TestAppContext,
    ) -> Entity<MessageEditor> {
        let thread = active_thread(conversation_view, cx);
        cx.read(|cx| thread.read(cx).message_editor.clone())
    }
}
