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
mod loading_draft_rendering;
#[cfg(test)]
mod message_editing_tests;
mod navigation_state;
mod notification;
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
mod settings_events;
mod thread_events;
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

    #[gpui::test]
    async fn test_notification_for_stop_event(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        cx.deactivate_window();

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some())
        );
    }

    #[gpui::test]
    async fn test_no_notification_when_queued_message_will_be_auto_sent(cx: &mut TestAppContext) {
        init_test(cx);

        let connection = StubAgentConnection::new();
        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
        add_to_workspace(conversation_view.clone(), cx);

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("first", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        let session_id = conversation_view.read_with(cx, |view, cx| {
            view.active_thread()
                .unwrap()
                .read(cx)
                .thread
                .read(cx)
                .session_id()
                .clone()
        });

        active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
            thread.add_to_queue(
                vec![acp::ContentBlock::Text(acp::TextContent::new(
                    "queued".to_string(),
                ))],
                vec![],
                window,
                cx,
            );
        });

        cx.deactivate_window();
        cx.run_until_parked();

        cx.update(|_, cx| {
            connection.send_update(
                session_id.clone(),
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                    "first response".into(),
                )),
                cx,
            );
            connection.end_turn(session_id, acp::StopReason::EndTurn);
        });

        cx.run_until_parked();

        assert_eq!(
            cx.windows()
                .iter()
                .filter(|window| window.downcast::<AgentNotification>().is_some())
                .count(),
            0,
            "No notification should fire when a queued message will be auto-sent on Stopped"
        );
    }

    #[gpui::test]
    async fn test_queued_message_steer_defaults_off_and_toggles(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;
        add_to_workspace(conversation_view.clone(), cx);

        let id = active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
            thread.add_to_queue(
                vec![acp::ContentBlock::Text(acp::TextContent::new(
                    "queued".to_string(),
                ))],
                vec![],
                window,
                cx,
            );
            thread.message_queue.first_id().unwrap()
        });
        cx.run_until_parked();

        // Default: steering is off, so the message waits for end-of-generation
        // rather than interrupting the agent at the next boundary.
        active_thread(&conversation_view, cx).read_with(cx, |thread, _cx| {
            assert!(
                !thread.message_queue.front_wants_steer(),
                "steering should default off"
            );
        });

        active_thread(&conversation_view, cx).update(cx, |thread, _cx| {
            thread.message_queue.toggle_steer(id);
        });
        active_thread(&conversation_view, cx).read_with(cx, |thread, _cx| {
            assert!(
                thread.message_queue.front_wants_steer(),
                "steering should be on after toggling"
            );
        });
    }

    #[gpui::test]
    async fn test_queue_resumes_after_stop_and_new_message(cx: &mut TestAppContext) {
        init_test(cx);

        let connection = StubAgentConnection::new();
        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
        add_to_workspace(conversation_view.clone(), cx);

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("first", window, cx);
        });
        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));
        cx.run_until_parked();

        // Queue a follow-up while the agent is generating.
        active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
            thread.add_to_queue(
                vec![acp::ContentBlock::Text(acp::TextContent::new(
                    "queued".to_string(),
                ))],
                vec![],
                window,
                cx,
            );
        });

        // User stops generation: the queued message must NOT be sent.
        active_thread(&conversation_view, cx)
            .update_in(cx, |thread, _window, cx| thread.cancel_generation(cx));
        cx.run_until_parked();

        let queue_len = active_thread(&conversation_view, cx)
            .read_with(cx, |thread, _cx| thread.message_queue.len());
        assert_eq!(queue_len, 1, "stopping must not send the queued message");

        // User sends a new message, which should resume queue auto-processing.
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("second", window, cx);
        });
        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));
        cx.run_until_parked();

        let session_id = conversation_view.read_with(cx, |view, cx| {
            view.active_thread()
                .unwrap()
                .read(cx)
                .thread
                .read(cx)
                .session_id()
                .clone()
        });

        // When this generation completes, the queued message should be picked
        // up automatically (regression test for the "frozen queue" bug).
        connection.end_turn(session_id, acp::StopReason::EndTurn);
        cx.run_until_parked();

        let queue_len = active_thread(&conversation_view, cx)
            .read_with(cx, |thread, _cx| thread.message_queue.len());
        assert_eq!(
            queue_len, 0,
            "queued message should be auto-sent after the user re-engages"
        );
    }

    #[gpui::test]
    async fn test_notification_for_error(cx: &mut TestAppContext) {
        init_test(cx);

        let server = FakeAcpAgentServer::new();
        let (conversation_view, cx) = setup_conversation_view(server.clone(), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        cx.deactivate_window();
        server.fail_next_prompt();

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some())
        );
    }

    #[gpui::test]
    async fn test_acp_server_exit_transitions_conversation_to_load_error_without_panic(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let server = FakeAcpAgentServer::new();
        let close_session_count = server.close_session_count();
        let (conversation_view, cx) = setup_conversation_view(server.clone(), cx).await;

        cx.run_until_parked();

        server.simulate_server_exit();
        cx.run_until_parked();

        conversation_view.read_with(cx, |view, _cx| {
            assert!(
                matches!(view.server_state, ServerState::LoadError { .. }),
                "Conversation should transition to LoadError when an ACP thread exits"
            );
        });
        assert_eq!(
            close_session_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "ConversationView should close the ACP session after a thread exit"
        );
    }

    #[gpui::test]
    async fn test_resume_without_history_adds_notice(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(ResumeOnlyAgentConnection)),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    Some(acp::SessionId::new("resume-session")),
                    None,
                    None,
                    None,
                    None,
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

        conversation_view.read_with(cx, |view, cx| {
            let state = view.active_thread().unwrap();
            assert!(state.read(cx).resumed_without_history);
            assert_eq!(state.read(cx).list_state.item_count(), 0);
        });
    }

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

    #[gpui::test]
    async fn test_restored_threads_keep_available_commands(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    Some(acp::SessionId::new("restored-session")),
                    None,
                    None,
                    None,
                    None,
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

        let message_editor = message_editor(&conversation_view, cx);
        let editor =
            message_editor.update(cx, |message_editor, _cx| message_editor.editor().clone());
        let placeholder = editor.update(cx, |editor, cx| editor.placeholder_text(cx));

        active_thread(&conversation_view, cx).read_with(cx, |view, _cx| {
            let available_commands = view
                .session_capabilities
                .read()
                .available_commands()
                .to_vec();
            assert_eq!(available_commands.len(), 1);
            assert_eq!(available_commands[0].name.as_str(), "help");
            assert_eq!(available_commands[0].description.as_str(), "Get help");
        });

        assert_eq!(placeholder, Some("Ask anything".to_string()));

        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("/help", window, cx);
        });

        let contents_result = message_editor
            .update(cx, |editor, cx| editor.contents(false, cx))
            .await;

        assert!(contents_result.is_ok());
    }

    #[gpui::test]
    async fn test_resume_thread_uses_session_cwd_when_inside_project(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                "subdir": {
                    "file.txt": "hello"
                }
            }),
        )
        .await;
        let project = Project::test(fs, [Path::new("/project")], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let connection = CwdCapturingConnection::new();
        let captured_cwd = connection.captured_work_dirs.clone();

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let _conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(connection)),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    Some(acp::SessionId::new("session-1")),
                    None,
                    Some(PathList::new(&[PathBuf::from("/project/subdir")])),
                    None,
                    None,
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

        assert_eq!(
            captured_cwd.lock().as_ref().unwrap(),
            &PathList::new(&[Path::new("/project/subdir")]),
            "Should use session cwd when it's inside the project"
        );
    }

    #[gpui::test]
    async fn test_refusal_handling(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::new(RefusalAgentConnection), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Do something harmful", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        // Check that the refusal error is set
        conversation_view.read_with(cx, |thread_view, cx| {
            let state = thread_view.active_thread().unwrap();
            assert!(
                matches!(state.read(cx).thread_error, Some(ThreadError::Refusal)),
                "Expected refusal error to be set"
            );
        });
    }

    #[gpui::test]
    async fn test_connect_failure_transitions_to_load_error(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) = setup_conversation_view(FailingAgentServer, cx).await;

        conversation_view.read_with(cx, |view, cx| {
            let title = view.title(cx);
            assert_eq!(
                title.as_ref(),
                "Error Loading Codex CLI",
                "Tab title should show the agent name with an error prefix"
            );
            match &view.server_state {
                ServerState::LoadError {
                    error: LoadError::Other(msg),
                    ..
                } => {
                    assert!(
                        msg.contains("Invalid gzip header"),
                        "Error callout should contain the underlying extraction error, got: {msg}"
                    );
                }
                other => panic!(
                    "Expected LoadError::Other, got: {}",
                    match other {
                        ServerState::Loading { .. } => "Loading (stuck!)",
                        ServerState::LoadError { .. } => "LoadError (wrong variant)",
                        ServerState::Connected(_) => "Connected",
                    }
                ),
            }
        });
    }

    #[gpui::test]
    async fn test_reset_preserves_session_id_after_load_error(cx: &mut TestAppContext) {
        use crate::thread_metadata_store::{ThreadId, ThreadMetadata};
        use chrono::Utc;
        use project::{AgentId as ProjectAgentId, WorktreePaths};
        use std::sync::atomic::Ordering;

        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        // Simulate a previous run that persisted metadata for this session.
        let resume_session_id = acp::SessionId::new("persistent-session");
        let stored_title: SharedString = "Persistent chat".into();
        cx.update(|_window, cx| {
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.save(
                    ThreadMetadata {
                        thread_id: ThreadId::new(),
                        session_id: Some(resume_session_id.clone()),
                        agent_id: ProjectAgentId::new("Flaky"),
                        title: Some(stored_title.clone()),
                        title_override: None,
                        updated_at: Utc::now(),
                        created_at: Some(Utc::now()),
                        interacted_at: None,
                        worktree_paths: WorktreePaths::from_folder_paths(&PathList::default()),
                        remote_connection: None,
                        archived: false,
                    },
                    cx,
                );
            });
        });

        let connection = StubAgentConnection::new().with_supports_load_session(true);
        let (server, fail) = FlakyAgentServer::new(connection);

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(server),
                    connection_store,
                    Agent::Custom { id: "Flaky".into() },
                    Some(resume_session_id.clone()),
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();

        // The first connect() fails, so we land in LoadError.
        conversation_view.read_with(cx, |view, _cx| {
            assert!(
                matches!(view.server_state, ServerState::LoadError { .. }),
                "expected LoadError after failed initial connect"
            );
            assert_eq!(
                view.root_session_id.as_ref(),
                Some(&resume_session_id),
                "root_session_id should still hold the original id while in LoadError"
            );
        });

        // Now let the agent come online and emit AgentServersUpdated. This is
        // the moment the bug would have stomped on root_session_id.
        fail.store(false, Ordering::SeqCst);
        project.update(cx, |project, cx| {
            project
                .agent_server_store()
                .update(cx, |_store, cx| cx.emit(project::AgentServersUpdated));
        });
        cx.run_until_parked();

        // The retry should have resumed the ORIGINAL session, not created a
        // brand-new one.
        conversation_view.read_with(cx, |view, cx| {
            let connected = view
                .as_connected()
                .expect("should be Connected after flaky server comes online");
            let active_id = connected
                .active_id
                .as_ref()
                .expect("Connected state should have an active_id");
            assert_eq!(
                active_id, &resume_session_id,
                "reset() must resume the original session id, not call new_session()"
            );
            let active_thread = view
                .active_thread()
                .expect("should have an active thread view");
            let thread_session = active_thread.read(cx).thread.read(cx).session_id().clone();
            assert_eq!(
                thread_session, resume_session_id,
                "the live AcpThread should hold the resumed session id"
            );
        });
    }

    #[gpui::test]
    async fn test_auth_required_on_initial_connect(cx: &mut TestAppContext) {
        init_test(cx);

        let connection = AuthGatedAgentConnection::new();
        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::new(connection), cx).await;

        // When new_session returns AuthRequired, the server should transition
        // to Connected + Unauthenticated rather than getting stuck in Loading.
        conversation_view.read_with(cx, |view, _cx| {
            let connected = view
                .as_connected()
                .expect("Should be in Connected state even though auth is required");
            assert!(
                !connected.auth_state.is_ok(),
                "Auth state should be Unauthenticated"
            );
            assert!(
                !view.supports_logout(),
                "Logout should be hidden while unauthenticated"
            );
            assert!(
                connected.active_id.is_none(),
                "There should be no active thread since no session was created"
            );
            assert!(
                connected.threads.is_empty(),
                "There should be no threads since no session was created"
            );
        });

        conversation_view.read_with(cx, |view, _cx| {
            assert!(
                view.active_thread().is_none(),
                "active_thread() should be None when unauthenticated without a session"
            );
        });

        // Authenticate using the real authenticate flow on ConnectionView.
        // This calls connection.authenticate(), which flips the internal flag,
        // then on success triggers reset() -> new_session() which now succeeds.
        conversation_view.update_in(cx, |view, window, cx| {
            view.authenticate(
                acp::AuthMethodId::new(AuthGatedAgentConnection::AUTH_METHOD_ID),
                window,
                cx,
            );
        });
        cx.run_until_parked();

        // After auth, the server should have an active thread in the Ok state.
        conversation_view.read_with(cx, |view, cx| {
            let connected = view
                .as_connected()
                .expect("Should still be in Connected state after auth");
            assert!(connected.auth_state.is_ok(), "Auth state should be Ok");
            assert!(
                view.supports_logout(),
                "Logout should be available after authentication"
            );
            assert!(
                connected.active_id.is_some(),
                "There should be an active thread after successful auth"
            );
            assert_eq!(
                connected.threads.len(),
                1,
                "There should be exactly one thread"
            );

            let active = view
                .active_thread()
                .expect("active_thread() should return the new thread");
            assert!(
                active.read(cx).thread_error.is_none(),
                "The new thread should have no errors"
            );
        });

        conversation_view.update_in(cx, |view, window, cx| view.logout(window, cx));
        cx.run_until_parked();

        conversation_view.read_with(cx, |view, _cx| {
            let connected = view
                .as_connected()
                .expect("Should still be in Connected state after logout");
            assert!(
                !connected.auth_state.is_ok(),
                "Auth state should be Unauthenticated after logout"
            );
            assert!(
                !view.supports_logout(),
                "Logout should be hidden after logout"
            );
        });
    }

    #[gpui::test]
    async fn test_notification_for_tool_authorization(cx: &mut TestAppContext) {
        init_test(cx);

        let tool_call_id = acp::ToolCallId::new("1");
        let tool_call = acp::ToolCall::new(tool_call_id.clone(), "Label")
            .kind(acp::ToolKind::Edit)
            .content(vec!["hi".into()]);
        let connection =
            StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
                tool_call_id,
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    "1",
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
            )]));

        connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::new(connection), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        cx.deactivate_window();

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some())
        );
    }

    #[gpui::test]
    async fn test_notification_when_panel_hidden(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;

        add_to_workspace(conversation_view.clone(), cx);

        let message_editor = message_editor(&conversation_view, cx);

        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        // Window is active (don't deactivate), but panel will be hidden
        // Note: In the test environment, the panel is not actually added to the dock,
        // so is_agent_panel_hidden will return true

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        // Should show notification because window is active but panel is hidden
        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification when panel is hidden"
        );
    }

    #[gpui::test]
    async fn test_notification_still_works_when_window_inactive(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        // Deactivate window - should show notification regardless of setting
        cx.deactivate_window();

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        // Should still show notification when window is inactive (existing behavior)
        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification when window is inactive"
        );
    }

    #[gpui::test]
    async fn test_notification_when_different_conversation_is_active_in_visible_panel(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());

        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn Fs>::set_global(fs.clone(), cx);
        });

        let project = Project::test(fs, [], cx).await;
        let multi_workspace_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| crate::AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            workspace.focus_panel::<crate::AgentPanel>(window, cx);
            panel
        });

        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::default_response()),
                window,
                cx,
            );
        });

        cx.run_until_parked();

        panel.read_with(cx, |panel, cx| {
            assert!(crate::AgentPanel::is_visible(&workspace, cx));
            assert!(panel.active_conversation_view().is_some());
        });

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::default_response()),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });

        cx.run_until_parked();

        panel.read_with(cx, |panel, _cx| {
            assert_ne!(
                panel
                    .active_conversation_view()
                    .map(|view| view.entity_id()),
                Some(conversation_view.entity_id()),
                "The visible panel should still be showing a different conversation"
            );
        });

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification when a different conversation is active in the visible panel"
        );
    }

    #[gpui::test]
    async fn test_no_notification_when_sidebar_open_but_different_thread_focused(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());

        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn Fs>::set_global(fs.clone(), cx);
        });

        let project = Project::test(fs, [], cx).await;
        let multi_workspace_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);
        register_test_sidebar(true, cx);

        // Open the sidebar so that sidebar_open() returns true.
        multi_workspace_handle
            .update(cx, |mw, _window, cx| {
                mw.open_sidebar(cx);
            })
            .unwrap();

        cx.run_until_parked();

        assert!(
            multi_workspace_handle
                .read_with(cx, |mw, _cx| mw.sidebar_open())
                .unwrap(),
            "Sidebar should be open"
        );

        // Create a conversation view that is NOT the active one in the panel.
        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::default_response()),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });

        cx.run_until_parked();

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            !cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected no notification when the sidebar is open, even if focused on another thread"
        );
    }

    #[gpui::test]
    async fn test_notification_when_sidebar_open_but_thread_list_hidden(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());

        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn Fs>::set_global(fs.clone(), cx);
        });

        let project = Project::test(fs, [], cx).await;
        let multi_workspace_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);
        register_test_sidebar(false, cx);
        multi_workspace_handle
            .update(cx, |mw, _window, cx| {
                mw.open_sidebar(cx);
            })
            .unwrap();
        cx.run_until_parked();

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::default_response()),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));
        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification when the sidebar is open but the thread list is hidden"
        );
    }

    #[gpui::test]
    async fn test_notification_dismissed_when_sidebar_opens(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());

        cx.update(|cx| {
            cx.update_flags(true, vec!["agent-v2".to_string()]);
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn Fs>::set_global(fs.clone(), cx);
        });

        let project = Project::test(fs, [], cx).await;
        let multi_workspace_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

        let workspace = multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);
        register_test_sidebar(true, cx);

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::default_response()),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });

        cx.run_until_parked();

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert_eq!(
            cx.windows()
                .iter()
                .filter(|window| window.downcast::<AgentNotification>().is_some())
                .count(),
            1,
            "Expected a notification while the thread is not visible"
        );

        multi_workspace_handle
            .update(cx, |mw, _window, cx| {
                mw.open_sidebar(cx);
            })
            .unwrap();

        cx.run_until_parked();

        assert_eq!(
            cx.windows()
                .iter()
                .filter(|window| window.downcast::<AgentNotification>().is_some())
                .count(),
            0,
            "Notification should auto-dismiss when the sidebar opens and makes the thread visible"
        );
    }

    #[gpui::test]
    async fn test_notification_when_workspace_is_background_in_multi_workspace(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        // Enable multi-workspace feature flag and init globals needed by AgentPanel
        let fs = FakeFs::new(cx.executor());

        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn Fs>::set_global(fs.clone(), cx);
        });

        let project1 = Project::test(fs.clone(), [], cx).await;

        // Create a MultiWorkspace window with one workspace
        let multi_workspace_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project1.clone(), window, cx));

        // Get workspace 1 (the initial workspace)
        let workspace1 = multi_workspace_handle
            .read_with(cx, |mw, _cx| mw.workspace().clone())
            .unwrap();

        let cx = &mut VisualTestContext::from_window(multi_workspace_handle.into(), cx);

        let panel = workspace1.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| crate::AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);

            // Open the dock and activate the agent panel so it's visible
            workspace.focus_panel::<crate::AgentPanel>(window, cx);
            panel
        });

        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.open_external_thread_with_server(
                Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
                window,
                cx,
            );
        });

        cx.run_until_parked();

        cx.read(|cx| {
            assert!(
                crate::AgentPanel::is_visible(&workspace1, cx),
                "AgentPanel should be visible in workspace1's dock"
            );
        });

        // Set up thread view in workspace 1
        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project1.clone(), cx)));

        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(RestoredAvailableCommandsConnection)),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace1.downgrade(),
                    project1.clone(),
                    Some(thread_store),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });
        cx.run_until_parked();

        let root_session_id = conversation_view
            .read_with(cx, |view, cx| {
                view.root_thread_view()
                    .map(|thread| thread.read(cx).thread.read(cx).session_id().clone())
            })
            .expect("Conversation view should have a root thread");

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        // Create a second workspace and switch to it.
        // This makes workspace1 the "background" workspace.
        let project2 = Project::test(fs, [], cx).await;
        multi_workspace_handle
            .update(cx, |mw, window, cx| {
                mw.test_add_workspace(project2, window, cx);
            })
            .unwrap();

        cx.run_until_parked();

        // Verify workspace1 is no longer the active workspace
        multi_workspace_handle
            .read_with(cx, |mw, _cx| {
                assert_ne!(mw.workspace(), &workspace1);
            })
            .unwrap();

        // Window is active, agent panel is visible in workspace1, but workspace1
        // is in the background. The notification should show because the user
        // can't actually see the agent panel.
        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification when workspace is in background within MultiWorkspace"
        );

        // Also verify: clicking "View Panel" should switch to workspace1.
        cx.windows()
            .iter()
            .find_map(|window| window.downcast::<AgentNotification>())
            .unwrap()
            .update(cx, |window, _, cx| window.accept(cx))
            .unwrap();

        cx.run_until_parked();

        multi_workspace_handle
            .read_with(cx, |mw, _cx| {
                assert_eq!(
                    mw.workspace(),
                    &workspace1,
                    "Expected workspace1 to become the active workspace after accepting notification"
                );
            })
            .unwrap();

        panel.read_with(cx, |panel, cx| {
            let active_session_id = panel
                .active_agent_thread(cx)
                .map(|thread| thread.read(cx).session_id().clone());
            assert_eq!(
                active_session_id,
                Some(root_session_id),
                "Expected accepting the notification to load the notified thread in AgentPanel"
            );
        });
    }

    #[gpui::test]
    async fn test_notification_respects_never_setting(cx: &mut TestAppContext) {
        init_test(cx);

        // Set notify_when_agent_waiting to Never
        cx.update(|cx| {
            AgentSettings::override_global(
                AgentSettings {
                    notify_when_agent_waiting: NotifyWhenAgentWaiting::Never,
                    ..AgentSettings::get_global(cx).clone()
                },
                cx,
            );
        });

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        // Window is active

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        // Should NOT show notification because notify_when_agent_waiting is Never
        assert!(
            !cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected no notification when notify_when_agent_waiting is Never"
        );
    }

    #[gpui::test]
    async fn test_notification_closed_when_thread_view_dropped(cx: &mut TestAppContext) {
        init_test(cx);

        let (conversation_view, cx) =
            setup_conversation_view(StubAgentServer::default_response(), cx).await;

        let weak_view = conversation_view.downgrade();

        let message_editor = message_editor(&conversation_view, cx);
        message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Hello", window, cx);
        });

        cx.deactivate_window();

        active_thread(&conversation_view, cx)
            .update_in(cx, |view, window, cx| view.send(window, cx));

        cx.run_until_parked();

        // Verify notification is shown
        assert!(
            cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Expected notification to be shown"
        );

        // Drop the thread view (simulating navigation to a new thread)
        drop(conversation_view);
        drop(message_editor);
        // Trigger an update to flush effects, which will call release_dropped_entities
        cx.update(|_window, _cx| {});
        cx.run_until_parked();

        // Verify the entity was actually released
        assert!(
            !weak_view.is_upgradable(),
            "Thread view entity should be released after dropping"
        );

        // The notification should be automatically closed via on_release
        assert!(
            !cx.windows()
                .iter()
                .any(|window| window.downcast::<AgentNotification>().is_some()),
            "Notification should be closed when thread view is dropped"
        );
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

    #[gpui::test]
    async fn test_rewind_views(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                "test1.txt": "old content 1",
                "test2.txt": "old content 2"
            }),
        )
        .await;
        let project = Project::test(fs, [Path::new("/project")], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let connection = Rc::new(StubAgentConnection::new());
        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(connection.as_ref().clone())),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store.clone()),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });

        cx.run_until_parked();

        let thread = conversation_view
            .read_with(cx, |view, cx| {
                view.active_thread().map(|r| r.read(cx).thread.clone())
            })
            .unwrap();

        // First user message
        connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
            acp::ToolCall::new("tool1", "Edit file 1")
                .kind(acp::ToolKind::Edit)
                .status(acp::ToolCallStatus::Completed)
                .content(vec![acp::ToolCallContent::Diff(
                    acp::Diff::new("/project/test1.txt", "new content 1").old_text("old content 1"),
                )]),
        )]);

        thread
            .update(cx, |thread, cx| thread.send_raw("Give me a diff", cx))
            .await
            .unwrap();
        cx.run_until_parked();

        thread.read_with(cx, |thread, _cx| {
            assert_eq!(thread.entries().len(), 2);
        });

        conversation_view.read_with(cx, |view, cx| {
            let entry_view_state = view
                .active_thread()
                .map(|active| active.read(cx).entry_view_state.clone())
                .unwrap();
            entry_view_state.read_with(cx, |entry_view_state, _| {
                assert!(
                    entry_view_state
                        .entry(0)
                        .unwrap()
                        .message_editor()
                        .is_some()
                );
                assert!(entry_view_state.entry(1).unwrap().has_content());
            });
        });

        // Second user message
        connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
            acp::ToolCall::new("tool2", "Edit file 2")
                .kind(acp::ToolKind::Edit)
                .status(acp::ToolCallStatus::Completed)
                .content(vec![acp::ToolCallContent::Diff(
                    acp::Diff::new("/project/test2.txt", "new content 2").old_text("old content 2"),
                )]),
        )]);

        thread
            .update(cx, |thread, cx| thread.send_raw("Another one", cx))
            .await
            .unwrap();
        cx.run_until_parked();

        let second_user_message_id = thread.read_with(cx, |thread, _| {
            assert_eq!(thread.entries().len(), 4);
            let AgentThreadEntry::UserMessage(user_message) = &thread.entries()[2] else {
                panic!();
            };
            user_message.client_id.clone().unwrap()
        });

        conversation_view.read_with(cx, |view, cx| {
            let entry_view_state = view
                .active_thread()
                .unwrap()
                .read(cx)
                .entry_view_state
                .clone();
            entry_view_state.read_with(cx, |entry_view_state, _| {
                assert!(
                    entry_view_state
                        .entry(0)
                        .unwrap()
                        .message_editor()
                        .is_some()
                );
                assert!(entry_view_state.entry(1).unwrap().has_content());
                assert!(
                    entry_view_state
                        .entry(2)
                        .unwrap()
                        .message_editor()
                        .is_some()
                );
                assert!(entry_view_state.entry(3).unwrap().has_content());
            });
        });

        // Rewind to first message
        thread
            .update(cx, |thread, cx| thread.rewind(second_user_message_id, cx))
            .await
            .unwrap();

        cx.run_until_parked();

        thread.read_with(cx, |thread, _| {
            assert_eq!(thread.entries().len(), 2);
        });

        conversation_view.read_with(cx, |view, cx| {
            let active = view.active_thread().unwrap();
            active
                .read(cx)
                .entry_view_state
                .read_with(cx, |entry_view_state, _| {
                    assert!(
                        entry_view_state
                            .entry(0)
                            .unwrap()
                            .message_editor()
                            .is_some()
                    );
                    assert!(entry_view_state.entry(1).unwrap().has_content());

                    // Old views should be dropped
                    assert!(entry_view_state.entry(2).is_none());
                    assert!(entry_view_state.entry(3).is_none());
                });
        });
    }

    #[gpui::test]
    async fn test_regenerate_keeps_pending_subagent_edits(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/project",
            json!({
                "file.txt": "original content"
            }),
        )
        .await;
        let project = Project::test(fs, [Path::new("/project")], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

        let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
        let connection_store =
            cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

        let connection = Rc::new(StubAgentConnection::new());
        let conversation_view = cx.update(|window, cx| {
            cx.new(|cx| {
                ConversationView::new(
                    Rc::new(StubAgentServer::new(connection.as_ref().clone())),
                    connection_store,
                    Agent::Custom { id: "Test".into() },
                    None,
                    None,
                    None,
                    None,
                    None,
                    workspace.downgrade(),
                    project.clone(),
                    Some(thread_store.clone()),
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                )
            })
        });

        cx.run_until_parked();

        let thread = conversation_view
            .read_with(cx, |view, cx| {
                view.active_thread().map(|r| r.read(cx).thread.clone())
            })
            .unwrap();

        // First turn: a subagent tool call. Subagent edits never appear as
        // diffs in the parent thread's entries; they are only forwarded to the
        // parent's action log through the linked-log mechanism.
        connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
            acp::ToolCall::new("spawn1", "Subagent task")
                .kind(acp::ToolKind::Other)
                .status(acp::ToolCallStatus::Completed)
                .meta(acp_thread::meta_with_tool_name("spawn_agent")),
        )]);

        thread
            .update(cx, |thread, cx| thread.send_raw("Use a subagent", cx))
            .await
            .unwrap();
        cx.run_until_parked();

        // Simulate the subagent editing a file: edits performed through a
        // child action log are forwarded to the parent thread's action log,
        // just like `Thread::new_subagent` wires it up.
        let parent_action_log = thread.read_with(cx, |thread, _| thread.action_log().clone());
        let subagent_action_log = cx.update(|_, cx| {
            cx.new(|_| {
                ActionLog::new(project.clone()).with_linked_action_log(parent_action_log.clone())
            })
        });

        let buffer = project
            .update(cx, |project, cx| {
                let path = project.find_project_path("file.txt", cx).unwrap();
                project.open_buffer(path, cx)
            })
            .await
            .unwrap();
        cx.update(|_, cx| {
            subagent_action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
            buffer.update(cx, |buffer, cx| {
                buffer.set_text("edited by subagent", cx);
            });
            subagent_action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
        });
        cx.run_until_parked();

        parent_action_log.read_with(cx, |log, cx| {
            assert_eq!(
                log.changed_buffers(cx).count(),
                1,
                "the subagent edit should be pending review in the parent's action log"
            );
        });

        // Second turn: a plain follow-up.
        connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("Response".into()),
        )]);
        thread
            .update(cx, |thread, cx| thread.send_raw("Follow-up", cx))
            .await
            .unwrap();
        cx.run_until_parked();

        let follow_up_ix = thread.read_with(cx, |thread, cx| {
            thread
                .entries()
                .iter()
                .position(|entry| entry.to_markdown(cx) == "## User\n\nFollow-up\n\n")
                .unwrap()
        });

        // Edit and regenerate the follow-up message.
        let user_message_editor = conversation_view.read_with(cx, |view, cx| {
            view.active_thread()
                .unwrap()
                .read(cx)
                .entry_view_state
                .read(cx)
                .entry(follow_up_ix)
                .unwrap()
                .message_editor()
                .unwrap()
                .clone()
        });
        user_message_editor.update_in(cx, |editor, window, cx| {
            editor.set_text("Edited follow-up", window, cx);
        });

        connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("New response".into()),
        )]);
        active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| {
            view.regenerate(follow_up_ix, user_message_editor.clone(), window, cx);
        });
        cx.run_until_parked();

        // The thread should have been rewound and the edited message resent.
        thread.read_with(cx, |thread, cx| {
            let entries = thread.entries();
            assert_eq!(entries.len(), 4);
            assert_eq!(
                entries[2].to_markdown(cx),
                "## User\n\nEdited follow-up\n\n"
            );
        });

        // The subagent's edits predate the regenerated prompt, so they must be
        // auto-kept rather than rejected by the rewind.
        buffer.read_with(cx, |buffer, _| {
            assert_eq!(
                buffer.text(),
                "edited by subagent",
                "pending subagent edits should be kept when regenerating a later prompt"
            );
        });
        parent_action_log.read_with(cx, |log, cx| {
            assert_eq!(
                log.changed_buffers(cx).count(),
                0,
                "the subagent edit should have been auto-kept"
            );
        });
    }
}
