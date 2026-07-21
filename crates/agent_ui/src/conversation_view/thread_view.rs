use crate::{
    DEFAULT_THREAD_TITLE, SelectPermissionGranularity,
    agent_configuration::configure_context_server_modal::default_markdown_style,
    conversation_view::thread_search_bar::{ThreadSearchBar, ThreadSearchBarEvent},
    open_abs_path_at_point,
    thread_metadata_store::{ThreadId, ThreadMetadataStore},
};
use agent_client_protocol::schema::v1 as acp;
use std::cell::RefCell;
#[path = "thread_view/action_handlers.rs"]
mod action_handlers;
#[path = "thread_view/activity_bar.rs"]
mod activity_bar;
#[path = "thread_view/awaiting_permission.rs"]
mod awaiting_permission;
#[path = "thread_view/context_compaction.rs"]
mod context_compaction;
#[path = "thread_view/data_retention_error.rs"]
mod data_retention_error;
#[path = "thread_view/draft_agent_selector.rs"]
mod draft_agent_selector;
#[path = "thread_view/edit_actions.rs"]
mod edit_actions;
#[path = "thread_view/edited_files.rs"]
mod edited_files;
#[path = "thread_view/editor_state.rs"]
mod editor_state;
#[path = "thread_view/edits_summary.rs"]
mod edits_summary;
#[path = "thread_view/entry_rendering.rs"]
mod entry_rendering;
#[path = "thread_view/error_state.rs"]
mod error_state;
#[path = "thread_view/fast_mode_controls.rs"]
mod fast_mode_controls;
#[path = "thread_view/fast_mode_warning.rs"]
mod fast_mode_warning;
#[path = "thread_view/feedback_editor.rs"]
mod feedback_editor;
#[path = "thread_view/feedback_state.rs"]
mod feedback_state;
#[path = "thread_view/follow_state.rs"]
mod follow_state;
#[path = "thread_view/generation_actions.rs"]
mod generation_actions;
#[path = "thread_view/generation_state.rs"]
mod generation_state;
#[path = "thread_view/link_opening.rs"]
mod link_opening;
#[path = "thread_view/markdown_helpers.rs"]
mod markdown_helpers;
#[path = "thread_view/message_context_menu.rs"]
mod message_context_menu;
#[path = "thread_view/message_controls.rs"]
mod message_controls;
#[path = "thread_view/message_editor_view.rs"]
mod message_editor_view;
#[path = "thread_view/message_queue_actions.rs"]
mod message_queue_actions;
#[path = "thread_view/native_command.rs"]
mod native_command;
#[path = "thread_view/navigation.rs"]
mod navigation;
#[path = "thread_view/numbered_code_block.rs"]
mod numbered_code_block;
#[path = "thread_view/permission_actions.rs"]
mod permission_actions;
#[path = "thread_view/permission_buttons.rs"]
mod permission_buttons;
#[path = "thread_view/permission_dropdown.rs"]
mod permission_dropdown;
#[path = "thread_view/plan_rendering.rs"]
mod plan_rendering;
#[path = "thread_view/queue_rendering.rs"]
mod queue_rendering;
#[path = "thread_view/retry_status_callout.rs"]
mod retry_status_callout;
#[path = "thread_view/sandbox_authorization_details.rs"]
mod sandbox_authorization_details;
#[path = "thread_view/sandbox_policy.rs"]
mod sandbox_policy;
#[path = "thread_view/sandbox_status.rs"]
mod sandbox_status;
#[path = "thread_view/sandbox_warning.rs"]
mod sandbox_warning;
#[path = "thread_view/send_flow.rs"]
mod send_flow;
#[path = "thread_view/subagent_card.rs"]
mod subagent_card;
#[path = "thread_view/subagent_titlebar.rs"]
mod subagent_titlebar;
#[path = "thread_view/subagent_tool_call.rs"]
mod subagent_tool_call;
#[path = "thread_view/terminal_tool_call.rs"]
mod terminal_tool_call;
#[path = "thread_view/thinking_block.rs"]
mod thinking_block;
#[path = "thread_view/thinking_controls.rs"]
mod thinking_controls;
#[path = "thread_view/thread_controls.rs"]
mod thread_controls;
#[path = "thread_view/thread_error_rendering.rs"]
mod thread_error_rendering;
#[path = "thread_view/thread_warnings.rs"]
mod thread_warnings;
#[path = "thread_view/token_usage.rs"]
mod token_usage;
#[path = "thread_view/token_usage_tooltip.rs"]
mod token_usage_tooltip;
#[path = "thread_view/tool_call_body.rs"]
mod tool_call_body;
#[path = "thread_view/tool_call_content.rs"]
mod tool_call_content;
#[path = "thread_view/tool_call_dispatch.rs"]
mod tool_call_dispatch;
#[path = "thread_view/tool_call_header.rs"]
mod tool_call_header;
#[path = "thread_view/tool_call_label.rs"]
mod tool_call_label;
#[path = "thread_view/tool_call_layout.rs"]
mod tool_call_layout;
#[path = "thread_view/tool_call_output.rs"]
mod tool_call_output;
#[path = "thread_view/tool_call_support.rs"]
mod tool_call_support;
#[path = "thread_view/tool_call_view.rs"]
mod tool_call_view;
#[path = "thread_view/ui_state.rs"]
mod ui_state;
#[path = "thread_view/user_message_entry.rs"]
mod user_message_entry;
#[path = "thread_view/workspace_utils.rs"]
mod workspace_utils;

use acp_thread::{
    PlanEntry, SandboxAuthorizationDetails, SandboxFallbackAuthorizationDetails,
    SandboxNotAppliedReason,
};
use agent::{
    SandboxStatusKey, SandboxStatusRefresh, SkillLoadingIssue, SkillLoadingIssueKind,
    SkillLoadingIssuesUpdated, ThreadSandbox, VerifiedSandboxStatus,
};
use agent_settings::UserAgentsMd;
use agent_skills::MAX_SKILL_DESCRIPTION_LEN;
use cloud_api_types::{SubmitAgentThreadFeedbackBody, SubmitAgentThreadFeedbackCommentsBody};
use editor::actions::OpenExcerpts;
use sandbox::{GitSandboxPolicy, SandboxFsPolicy, SandboxNetPolicy, SandboxPolicy};

use crate::completion_provider::AvailableSkill;
use crate::message_editor::SharedSessionCapabilities;
use crate::ui::{SandboxGroup, SandboxRow, SandboxSection, SandboxStatusTooltip};

use gpui::List;
use gpui::Stateful;
use gpui::TaskExt;
use heapless::Vec as ArrayVec;
use language_model::{
    FastModeConfirmation, LanguageModel, LanguageModelEffortLevel, LanguageModelId,
    LanguageModelProvider, LanguageModelProviderId, LanguageModelRegistry, Speed,
};
use settings::{update_settings_file, update_settings_file_with_completion};
use ui::{
    ButtonLike, CalloutBorderPosition, SpinnerLabel, SpinnerVariant, SplitButton, SplitButtonStyle,
    Tab,
};
use workspace::{OpenOptions, SERIALIZATION_THROTTLE_TIME};

use super::*;
pub(crate) use fast_mode_warning::reset_fast_mode_warnings;
use feedback_state::ThreadFeedbackState;
pub(crate) use link_opening::open_link;
use native_command::{leading_native_command, strip_leading_command};
use numbered_code_block::{parse_cat_numbered_markdown_code_block, render_cat_numbered_code_block};
use sandbox_policy::{
    augment_settings_sandbox_policy, sandbox_policy_grants_nothing, sandbox_section,
};
use tool_call_layout::ToolCallLayout;
use ui_state::GeneratingSpinnerElement;
pub(crate) use ui_state::PermissionSelection;
pub use workspace_utils::open_markdown_in_workspace;
use workspace_utils::{full_path_for_empty_project_path, skill_issue_file_label};

const DATA_RETENTION_LEARN_MORE_URL: &str = "https://support.claude.com/en/articles/15425996-data-retention-practices-for-mythos-class-models";

pub enum AcpThreadViewEvent {
    Interacted,
}

impl EventEmitter<AcpThreadViewEvent> for ThreadView {}

pub struct ThreadView {
    pub(crate) root_thread_id: ThreadId,
    pub(crate) started_as_draft: bool,
    pub session_id: acp::SessionId,
    pub parent_session_id: Option<acp::SessionId>,
    pub thread: Entity<AcpThread>,
    pub(crate) conversation: Entity<super::Conversation>,
    pub server_view: WeakEntity<ConversationView>,
    pub agent_icon: IconName,
    pub agent_icon_from_external_svg: Option<SharedString>,
    pub agent_id: AgentId,
    pub focus_handle: FocusHandle,
    pub workspace: WeakEntity<Workspace>,
    pub entry_view_state: Entity<EntryViewState>,
    pub title_editor: Entity<Editor>,
    pub config_options_view: Option<Entity<ConfigOptionsView>>,
    pub mode_selector: Option<Entity<ModeSelector>>,
    pub model_selector: Option<Entity<ModelSelectorPopover>>,
    pub profile_selector: Option<Entity<ProfileSelector>>,
    pub permission_dropdown_handle: PopoverMenuHandle<ContextMenu>,
    pub thread_retry_status: Option<RetryStatus>,
    pub(super) thread_error: Option<ThreadError>,
    pub thread_error_markdown: Option<Entity<Markdown>>,
    pub token_limit_callout_dismissed: bool,
    pub last_token_limit_telemetry: Option<acp_thread::TokenUsageRatio>,
    thread_feedback: ThreadFeedbackState,
    pub list_state: ListState,
    pub session_capabilities: SharedSessionCapabilities,
    pub expanded_tool_call_raw_inputs: HashSet<acp::ToolCallId>,
    collapsed_sandbox_authorization_details: HashSet<acp::ToolCallId>,
    collapsed_sandbox_network_details: HashSet<acp::ToolCallId>,
    pub subagent_scroll_handles: RefCell<HashMap<acp::SessionId, ScrollHandle>>,
    pub edits_expanded: bool,
    pub plan_expanded: bool,
    pub queue_expanded: bool,
    pub editor_expanded: bool,
    pub should_be_following: bool,
    pub editing_message: Option<usize>,
    pub message_queue: MessageQueue,
    pub turn_fields: TurnFields,
    pub discarded_partial_edits: HashSet<acp::ToolCallId>,
    pub is_loading_contents: bool,
    pub new_server_version_available: Option<SharedString>,
    pub resumed_without_history: bool,
    pub(crate) permission_selections: HashMap<acp::ToolCallId, PermissionSelection>,
    pub _cancel_task: Option<Task<()>>,
    _save_task: Option<Task<()>>,
    _draft_resolve_task: Option<Task<()>>,
    _sandbox_status_refresh_task: Option<Task<()>>,
    pub hovered_edited_file_buttons: Option<usize>,
    pub in_flight_prompt: Option<Vec<acp::ContentBlock>>,
    pub _subscriptions: Vec<Subscription>,
    pub message_editor: Entity<MessageEditor>,
    pub draft_agent_selector_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub add_context_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub thinking_effort_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub fast_mode_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub project: WeakEntity<Project>,
    /// Cache + worktree snapshot for resolving paths in markdown code spans.
    /// Cloned from the parent `ConversationView` so the cache is shared and the
    /// snapshot stays in sync via the parent's project-event subscription.
    pub(crate) code_span_resolver: AgentCodeSpanResolver,
    pub show_external_source_prompt_warning: bool,
    pub show_codex_windows_warning: bool,
    sandbox_status: Option<VerifiedSandboxStatus>,
    sandbox_status_key: Option<SandboxStatusKey>,
    pending_sandbox_status_key: Option<SandboxStatusKey>,
    pub multi_root_callout_dismissed: bool,
    pub generating_indicator_in_list: bool,
    pub skill_loading_issues: Vec<SkillLoadingIssue>,
    /// Issues the user has explicitly dismissed. Each entry is matched against
    /// emitted issues by full equality; when an issue no longer appears in the
    /// latest replacement list (because the underlying file was fixed/removed), it's
    /// dropped from this set so a future regression of the same kind would
    /// re-show.
    dismissed_skill_loading_issues: HashSet<SkillLoadingIssue>,
    pub(crate) thread_search_bar: Option<Entity<super::thread_search_bar::ThreadSearchBar>>,
    pub(crate) thread_search_visible: bool,
}
impl Focusable for ThreadView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.parent_session_id.is_some() {
            self.focus_handle.clone()
        } else {
            self.active_editor(cx).focus_handle(cx)
        }
    }
}

#[derive(Default)]
pub struct TurnFields {
    pub _turn_timer_task: Option<Task<()>>,
    pub last_turn_duration: Option<Duration>,
    pub last_turn_tokens: Option<u64>,
    pub turn_generation: usize,
    pub turn_started_at: Option<Instant>,
    pub turn_tokens: Option<u64>,
}

impl ThreadView {
    pub(crate) fn new(
        root_thread_id: ThreadId,
        started_as_draft: bool,
        thread: Entity<AcpThread>,
        conversation: Entity<super::Conversation>,
        server_view: WeakEntity<ConversationView>,
        agent_icon: IconName,
        agent_icon_from_external_svg: Option<SharedString>,
        agent_id: AgentId,
        agent_display_name: SharedString,
        workspace: WeakEntity<Workspace>,
        entry_view_state: Entity<EntryViewState>,
        config_options_view: Option<Entity<ConfigOptionsView>>,
        mode_selector: Option<Entity<ModeSelector>>,
        model_selector: Option<Entity<ModelSelectorPopover>>,
        profile_selector: Option<Entity<ProfileSelector>>,
        list_state: ListState,
        session_capabilities: SharedSessionCapabilities,
        resumed_without_history: bool,
        project: WeakEntity<Project>,
        code_span_resolver: AgentCodeSpanResolver,
        thread_store: Option<Entity<ThreadStore>>,
        initial_content: Option<AgentInitialContent>,
        mut subscriptions: Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_id = thread.read(cx).session_id().clone();
        let parent_session_id = thread.read(cx).parent_session_id().cloned();

        let has_slash_completions = session_capabilities.read().has_slash_completions();
        let placeholder = placeholder_text(agent_display_name.as_ref(), has_slash_completions);

        let mut should_auto_submit = false;
        let mut show_external_source_prompt_warning = false;

        let message_editor = cx.new(|cx| {
            let mut editor = MessageEditor::new(
                workspace.clone(),
                project.clone(),
                thread_store,
                session_capabilities.clone(),
                agent_id.clone(),
                &placeholder,
                editor::EditorMode::AutoHeight {
                    min_lines: AgentSettings::get_global(cx).message_editor_min_lines,
                    max_lines: Some(AgentSettings::get_global(cx).set_message_editor_max_lines()),
                },
                window,
                cx,
            );
            if let Some(content) = initial_content {
                match content {
                    AgentInitialContent::ThreadSummary { session_id, title } => {
                        editor.insert_thread_summary(session_id, title, window, cx);
                    }
                    AgentInitialContent::ContentBlock {
                        blocks,
                        auto_submit,
                    } => {
                        should_auto_submit = auto_submit;
                        editor.set_message(blocks, window, cx);
                    }
                    AgentInitialContent::FromExternalSource(prompt) => {
                        show_external_source_prompt_warning = true;
                        // SECURITY: Be explicit about not auto submitting prompt from external source.
                        should_auto_submit = false;
                        editor.set_message(
                            vec![acp::ContentBlock::Text(acp::TextContent::new(
                                prompt.into_string(),
                            ))],
                            window,
                            cx,
                        );
                    }
                }
            } else if let Some(draft) = thread.read(cx).draft_prompt() {
                editor.set_message(draft.to_vec(), window, cx);
            }
            editor
        });

        let show_codex_windows_warning = cfg!(windows)
            && project.upgrade().is_some_and(|p| p.read(cx).is_local())
            && agent_id.as_ref() == "Codex";

        if let Some(project) = project.upgrade() {
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
        }

        let title_editor = {
            let metadata = ThreadMetadataStore::try_global(cx)
                .and_then(|store| store.read(cx).entry(root_thread_id).cloned());
            let initial_title = if parent_session_id.is_none() {
                metadata.as_ref().and_then(|m| m.title())
            } else {
                thread.read(cx).title()
            }
            .unwrap_or_else(|| DEFAULT_THREAD_TITLE.into());
            let editor = cx.new(|cx| {
                let mut editor = Editor::single_line(window, cx);
                editor.set_text(initial_title, window, cx);
                editor
            });
            subscriptions.push(cx.subscribe_in(&editor, window, Self::handle_title_editor_event));
            editor
        };

        subscriptions.push(cx.subscribe_in(
            &entry_view_state,
            window,
            Self::handle_entry_view_event,
        ));

        subscriptions.push(cx.subscribe_in(
            &message_editor,
            window,
            Self::handle_message_editor_event,
        ));

        // If this thread is backed by a NativeAgent, listen for skill loading
        // issues so we can surface them as banners. The agent emits a single
        // replacement-style event per project refresh, so we overwrite our
        // local list rather than appending — this also clears stale issues
        // once a user resolves them.
        if let Some(native_connection) = thread
            .read(cx)
            .connection()
            .clone()
            .downcast::<agent::NativeAgentConnection>()
        {
            let project_id = thread.read(cx).project().entity_id();
            subscriptions.push(cx.subscribe(
                &native_connection.0,
                move |this: &mut Self, _agent, event: &SkillLoadingIssuesUpdated, cx| {
                    if event.project_id != project_id {
                        return;
                    }
                    // Drop dismissals for issues that no longer appear in the emitted
                    // list — the underlying file must have been fixed or removed, so a
                    // future regression should re-show.
                    this.dismissed_skill_loading_issues
                        .retain(|dismissed| event.issues.contains(dismissed));

                    // Show only issues that haven't been dismissed.
                    this.skill_loading_issues = event
                        .issues
                        .iter()
                        .filter(|issue| !this.dismissed_skill_loading_issues.contains(issue))
                        .cloned()
                        .collect();
                    cx.notify();
                },
            ));
        }

        subscriptions.push(cx.observe(&message_editor, |this, editor, cx| {
            let is_empty = editor.read(cx).text(cx).is_empty();
            let draft_contents_task = if is_empty {
                None
            } else {
                Some(editor.update(cx, |editor, cx| editor.draft_contents(cx)))
            };
            this._draft_resolve_task = Some(cx.spawn(async move |this, cx| {
                let draft = if let Some(task) = draft_contents_task {
                    let blocks = task.await.ok().filter(|b| !b.is_empty());
                    blocks
                } else {
                    None
                };
                this.update(cx, |this, cx| {
                    this.thread.update(cx, |thread, cx| {
                        thread.set_draft_prompt(draft, cx);
                    });
                    this.schedule_save(cx);
                })
                .ok();
            }));
        }));

        let mut this = Self {
            root_thread_id,
            started_as_draft,
            session_id,
            parent_session_id,
            focus_handle: cx.focus_handle(),
            thread,
            conversation,
            server_view,
            agent_icon,
            agent_icon_from_external_svg,
            agent_id,
            workspace,
            entry_view_state,
            title_editor,
            config_options_view,
            mode_selector,
            model_selector,
            profile_selector,
            list_state,
            session_capabilities,
            resumed_without_history,
            _subscriptions: subscriptions,
            permission_dropdown_handle: PopoverMenuHandle::default(),
            thread_retry_status: None,
            thread_error: None,
            thread_error_markdown: None,
            token_limit_callout_dismissed: false,
            last_token_limit_telemetry: None,
            thread_feedback: Default::default(),
            expanded_tool_call_raw_inputs: HashSet::default(),
            collapsed_sandbox_authorization_details: HashSet::default(),
            collapsed_sandbox_network_details: HashSet::default(),
            subagent_scroll_handles: RefCell::new(HashMap::default()),
            edits_expanded: false,
            plan_expanded: false,
            queue_expanded: true,
            editor_expanded: false,
            should_be_following: false,
            editing_message: None,
            message_queue: MessageQueue::default(),
            turn_fields: TurnFields::default(),
            discarded_partial_edits: HashSet::default(),
            is_loading_contents: false,
            new_server_version_available: None,
            permission_selections: HashMap::default(),
            _cancel_task: None,
            _save_task: None,
            _draft_resolve_task: None,
            _sandbox_status_refresh_task: None,
            hovered_edited_file_buttons: None,
            in_flight_prompt: None,
            message_editor,
            draft_agent_selector_menu_handle: PopoverMenuHandle::default(),
            add_context_menu_handle: PopoverMenuHandle::default(),
            thinking_effort_menu_handle: PopoverMenuHandle::default(),
            fast_mode_menu_handle: PopoverMenuHandle::default(),
            project,
            code_span_resolver,
            show_external_source_prompt_warning,
            show_codex_windows_warning,
            sandbox_status: None,
            sandbox_status_key: None,
            pending_sandbox_status_key: None,
            multi_root_callout_dismissed: false,
            generating_indicator_in_list: false,
            skill_loading_issues: Vec::new(),
            dismissed_skill_loading_issues: HashSet::default(),
            thread_search_bar: None,
            thread_search_visible: false,
        };

        this.sync_generating_indicator(cx);
        this.sync_editor_mode_for_empty_state(cx);
        let list_state_for_scroll = this.list_state.clone();
        let thread_view = cx.entity().downgrade();

        this.list_state
            .set_scroll_handler(move |_event, _window, cx| {
                let list_state = list_state_for_scroll.clone();
                let thread_view = thread_view.clone();
                // N.B. We must defer because the scroll handler is called while the
                // ListState's RefCell is mutably borrowed. Reading logical_scroll_top()
                // directly would panic from a double borrow.
                cx.defer(move |cx| {
                    let scroll_top = list_state.logical_scroll_top();
                    let _ = thread_view.update(cx, |this, cx| {
                        if let Some(thread) = this.as_native_thread(cx) {
                            thread.update(cx, |thread, _cx| {
                                thread.set_ui_scroll_position(Some(scroll_top));
                            });
                        }
                        this.schedule_save(cx);
                    });
                });
            });

        if should_auto_submit {
            this.send(window, cx);
        }
        this
    }

    /// Schedule a throttled save of the thread state (draft prompt, scroll position, etc.).
    /// Multiple calls within `SERIALIZATION_THROTTLE_TIME` are coalesced into a single save.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SERIALIZATION_THROTTLE_TIME)
                .await;
            this.update(cx, |this, cx| {
                if let Some(thread) = this.as_native_thread(cx) {
                    thread.update(cx, |_thread, cx| cx.notify());
                }
            })
            .ok();
        }));
    }

    pub fn handle_message_editor_event(
        &mut self,
        _editor: &Entity<MessageEditor>,
        event: &MessageEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The three skill-watcher trigger points all live here:
        // - `Focus` fires when the user clicks into the input box.
        // - `SlashAutocompleteOpened` fires when the completion
        //   provider is asked for slash commands.
        // - `Send` fires when the user submits the conversation.
        // All three triggers are idempotent; firing the same one
        // repeatedly is a no-op once a scan or watch is active.
        if matches!(
            event,
            MessageEditorEvent::Focus
                | MessageEditorEvent::SlashAutocompleteOpened
                | MessageEditorEvent::Send
        ) {
            if let Some(connection) = self.as_native_connection(cx) {
                connection.ensure_skills_scan_started(cx);
                if let Some(project) = self.project.upgrade() {
                    connection.refresh_skills_for_project(project, cx);
                }
            }
        }

        match event {
            MessageEditorEvent::Send => self.send(window, cx),
            MessageEditorEvent::SendImmediately => self.interrupt_and_send(window, cx),
            MessageEditorEvent::Cancel => {
                if !self.close_thread_search(window, cx) {
                    self.cancel_generation(cx);
                }
            }
            MessageEditorEvent::Focus => {
                self.cancel_editing(&Default::default(), window, cx);
            }
            MessageEditorEvent::LostFocus => {}
            MessageEditorEvent::SlashAutocompleteOpened => {}
            MessageEditorEvent::InputAttempted { .. } => {}
            MessageEditorEvent::Edited => {}
        }
    }

    pub(crate) fn as_native_connection(
        &self,
        cx: &App,
    ) -> Option<Rc<agent::NativeAgentConnection>> {
        let acp_thread = self.thread.read(cx);
        acp_thread.connection().clone().downcast()
    }

    pub fn as_native_thread(&self, cx: &App) -> Option<Entity<agent::Thread>> {
        let acp_thread = self.thread.read(cx);
        self.as_native_connection(cx)?
            .thread(acp_thread.session_id(), cx)
    }

    /// Resolves the message editor's contents into content blocks. For profiles
    /// that do not enable any tools, directory mentions are expanded to inline
    /// file contents since the agent can't read files on its own.
    fn resolve_message_contents(
        &self,
        message_editor: &Entity<MessageEditor>,
        cx: &mut App,
    ) -> Task<Result<(Vec<acp::ContentBlock>, Vec<Entity<Buffer>>)>> {
        let expand = self.as_native_thread(cx).is_some_and(|thread| {
            let thread = thread.read(cx);
            AgentSettings::get_global(cx)
                .profiles
                .get(thread.profile())
                .is_some_and(|profile| profile.tools.is_empty())
        });
        message_editor.update(cx, |message_editor, cx| message_editor.contents(expand, cx))
    }

    pub fn current_model_id(&self, cx: &App) -> Option<String> {
        let selector = self.model_selector.as_ref()?;
        let model = selector.read(cx).active_model(cx)?;
        Some(model.id.to_string())
    }

    pub fn current_mode_id(&self, cx: &App) -> Option<Arc<str>> {
        if let Some(thread) = self.as_native_thread(cx) {
            Some(thread.read(cx).profile().0.clone())
        } else {
            let mode_selector = self.mode_selector.as_ref()?;
            Some(mode_selector.read(cx).mode().0)
        }
    }

    fn is_subagent(&self) -> bool {
        self.parent_session_id.is_some()
    }

    pub(crate) fn is_draft(&self, cx: &App) -> bool {
        self.parent_session_id.is_none()
            && self.started_as_draft
            && !self.has_user_submitted_prompt(cx)
    }

    pub(crate) fn has_user_submitted_prompt(&self, cx: &App) -> bool {
        self.in_flight_prompt.is_some()
            || self
                .thread
                .read(cx)
                .entries()
                .iter()
                .any(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
    }

    /// Returns the currently active editor, either for a message that is being
    /// edited or the editor for a new message.
    pub(crate) fn active_editor(&self, cx: &App) -> Entity<MessageEditor> {
        if let Some(index) = self.editing_message
            && let Some(editor) = self
                .entry_view_state
                .read(cx)
                .entry(index)
                .and_then(|entry| entry.message_editor())
                .cloned()
        {
            editor
        } else {
            self.message_editor.clone()
        }
    }

    pub fn has_queued_messages(&self) -> bool {
        !self.message_queue.is_empty()
    }

    // events

    pub fn handle_entry_view_event(
        &mut self,
        _: &Entity<EntryViewState>,
        event: &EntryViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &event.view_event {
            ViewEvent::NewDiff(tool_call_id) => {
                if AgentSettings::get_global(cx).expand_edit_card {
                    self.entry_view_state.update(cx, |state, _cx| {
                        state.expand_tool_call(tool_call_id.clone());
                    });
                }
            }
            ViewEvent::NewTerminal(tool_call_id) => {
                if AgentSettings::get_global(cx).expand_terminal_card {
                    self.entry_view_state.update(cx, |state, _cx| {
                        state.expand_tool_call(tool_call_id.clone());
                    });
                }
            }
            ViewEvent::TerminalMovedToBackground(tool_call_id) => {
                self.entry_view_state.update(cx, |state, _cx| {
                    state.collapse_tool_call(tool_call_id);
                });
            }
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::Focus) => {
                if let Some(AgentThreadEntry::UserMessage(user_message)) =
                    self.thread.read(cx).entries().get(event.entry_index)
                    && self.thread.read(cx).supports_truncate(cx)
                    && user_message.client_id.is_some()
                    && !self.is_subagent()
                {
                    self.editing_message = Some(event.entry_index);
                    cx.notify();
                }
            }
            ViewEvent::MessageEditorEvent(editor, MessageEditorEvent::LostFocus) => {
                if let Some(AgentThreadEntry::UserMessage(user_message)) =
                    self.thread.read(cx).entries().get(event.entry_index)
                    && self.thread.read(cx).supports_truncate(cx)
                    && user_message.client_id.is_some()
                    && !self.is_subagent()
                {
                    if editor.read(cx).text(cx).as_str() == user_message.content.to_markdown(cx) {
                        self.editing_message = None;
                        cx.notify();
                    }
                }
            }
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::SendImmediately) => {}
            ViewEvent::MessageEditorEvent(editor, MessageEditorEvent::Send) => {
                if !self.is_subagent() {
                    self.regenerate(event.entry_index, editor.clone(), window, cx);
                }
            }
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::Cancel) => {
                self.cancel_editing(&Default::default(), window, cx);
            }
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::SlashAutocompleteOpened) => {
            }
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::Edited) => {}
            ViewEvent::MessageEditorEvent(_editor, MessageEditorEvent::InputAttempted { .. }) => {}
            ViewEvent::OpenDiffLocation {
                path,
                position,
                split,
            } => {
                self.open_diff_location(path, *position, *split, window, cx);
            }
        }
    }

    fn open_diff_location(
        &self,
        path: &str,
        position: Point,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.upgrade() else {
            return;
        };
        let Some(project_path) = project.read(cx).find_project_path(path, cx) else {
            return;
        };

        let open_task = if split {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.split_path(project_path, window, cx)
                })
                .log_err()
        } else {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.open_path(project_path, None, true, window, cx)
                })
                .log_err()
        };

        let Some(open_task) = open_task else {
            return;
        };

        window
            .spawn(cx, async move |cx| {
                let item = open_task.await?;
                let Some(editor) = item.downcast::<Editor>() else {
                    return anyhow::Ok(());
                };
                editor.update_in(cx, |editor, window, cx| {
                    editor.change_selections(
                        SelectionEffects::scroll(Autoscroll::center()),
                        window,
                        cx,
                        |selections| {
                            selections.select_ranges([position..position]);
                        },
                    );
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
    }

    // turns

    pub fn start_turn(&mut self, cx: &mut Context<Self>) -> usize {
        self.turn_fields.turn_generation += 1;
        let generation = self.turn_fields.turn_generation;
        self.turn_fields.turn_started_at = Some(Instant::now());
        self.turn_fields.last_turn_duration = None;
        self.turn_fields.last_turn_tokens = None;
        self.turn_fields.turn_tokens = Some(0);
        self.turn_fields._turn_timer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
        generation
    }

    pub fn stop_turn(&mut self, generation: usize, _cx: &mut Context<Self>) {
        if self.turn_fields.turn_generation != generation {
            return;
        }
        self.turn_fields.last_turn_duration = self
            .turn_fields
            .turn_started_at
            .take()
            .map(|started| started.elapsed());
        self.turn_fields.last_turn_tokens = self.turn_fields.turn_tokens.take();
        self.turn_fields._turn_timer_task = None;
    }

    pub fn update_turn_tokens(&mut self, cx: &App) {
        if let Some(usage) = self.thread.read(cx).token_usage() {
            if let Some(tokens) = &mut self.turn_fields.turn_tokens {
                *tokens += usage.output_tokens;
                self.emit_token_limit_telemetry_if_needed(cx);
            }
        }
    }

    fn emit_token_limit_telemetry_if_needed(&mut self, cx: &App) {
        let (ratio, agent_telemetry_id, session_id) = {
            let thread_data = self.thread.read(cx);
            let Some(token_usage) = thread_data.token_usage() else {
                return;
            };
            (
                token_usage.ratio(),
                thread_data.connection().telemetry_id(),
                thread_data.session_id().clone(),
            )
        };

        let kind = match ratio {
            acp_thread::TokenUsageRatio::Normal => {
                self.last_token_limit_telemetry = None;
                return;
            }
            acp_thread::TokenUsageRatio::Warning => "warning",
            acp_thread::TokenUsageRatio::Exceeded => "exceeded",
        };

        let should_skip = self
            .last_token_limit_telemetry
            .as_ref()
            .is_some_and(|last| *last >= ratio);
        if should_skip {
            return;
        }

        self.last_token_limit_telemetry = Some(ratio);

        telemetry::event!(
            "Agent Token Limit Warning",
            agent = agent_telemetry_id,
            session_id = session_id,
            kind = kind,
        );
    }
}

impl Render for ThreadView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_messages = self.list_state.item_count() > 0;
        let list_state = self.list_state.clone();

        let conversation = v_flex()
            .when(self.resumed_without_history, |this| {
                this.child(Self::render_resume_notice(cx))
            })
            .map(|this| {
                if has_messages {
                    this.flex_1()
                        .size_full()
                        .child(self.render_entries(cx))
                        .vertical_scrollbar_for(&list_state, window, cx)
                        .into_any()
                } else {
                    this.w_full().min_h_0().flex_1().into_any()
                }
            });

        v_flex()
            .key_context("AcpThread")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &menu::Cancel, _, cx| {
                if this.parent_session_id.is_none() {
                    this.cancel_generation(cx);
                }
            }))
            .on_action(cx.listener(
                |this, _: &super::thread_search_bar::DismissThreadSearch, window, cx| {
                    this.close_thread_search(window, cx);
                },
            ))
            // Esc can arrive as `editor::Cancel` from the query editor.
            .on_action(
                cx.listener(|this, _: &editor::actions::Cancel, window, cx| {
                    if !this.close_thread_search(window, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(
                |this, action: &super::thread_search_bar::SelectNextThreadMatch, window, cx| {
                    if !this.thread_search_visible {
                        cx.propagate();
                        return;
                    }
                    if let Some(bar) = this.thread_search_bar.clone() {
                        bar.update(cx, |bar, cx| bar.select_next_match(action, window, cx));
                    }
                },
            ))
            .on_action(cx.listener(
                |this, action: &super::thread_search_bar::SelectPreviousThreadMatch, window, cx| {
                    if !this.thread_search_visible {
                        cx.propagate();
                        return;
                    }
                    if let Some(bar) = this.thread_search_bar.clone() {
                        bar.update(cx, |bar, cx| bar.select_prev_match(action, window, cx));
                    }
                },
            ))
            .on_action(
                cx.listener(|this, _: &search::ToggleCaseSensitive, window, cx| {
                    if !this.thread_search_visible {
                        cx.propagate();
                        return;
                    }
                    if let Some(bar) = this.thread_search_bar.clone() {
                        bar.update(cx, |bar, cx| {
                            bar.toggle_case_sensitive(&search::ToggleCaseSensitive, window, cx)
                        });
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &search::ToggleWholeWord, window, cx| {
                    if !this.thread_search_visible {
                        cx.propagate();
                        return;
                    }
                    if let Some(bar) = this.thread_search_bar.clone() {
                        bar.update(cx, |bar, cx| {
                            bar.toggle_whole_word(&search::ToggleWholeWord, window, cx)
                        });
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &search::ToggleRegex, window, cx| {
                if !this.thread_search_visible {
                    cx.propagate();
                    return;
                }
                if let Some(bar) = this.thread_search_bar.clone() {
                    bar.update(cx, |bar, cx| {
                        bar.toggle_regex(&search::ToggleRegex, window, cx)
                    });
                }
            }))
            .on_action(
                cx.listener(|this, action: &search::FocusSearch, window, cx| {
                    if !this.thread_search_visible {
                        cx.propagate();
                        return;
                    }
                    if let Some(bar) = this.thread_search_bar.clone() {
                        bar.update(cx, |bar, cx| bar.focus_search(action, window, cx));
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &workspace::GoBack, window, cx| {
                if let Some(parent_session_id) = this.thread.read(cx).parent_session_id().cloned() {
                    this.server_view
                        .update(cx, |view, cx| {
                            view.navigate_to_thread(parent_session_id, window, cx);
                        })
                        .ok();
                }
            }))
            .on_action(cx.listener(Self::keep_all))
            .on_action(cx.listener(Self::reject_all))
            .on_action(cx.listener(Self::undo_last_reject))
            .on_action(cx.listener(Self::allow_always))
            .on_action(cx.listener(Self::allow_once))
            .on_action(cx.listener(Self::reject_once))
            .on_action(cx.listener(Self::handle_authorize_tool_call))
            .on_action(cx.listener(Self::handle_select_permission_granularity))
            .on_action(cx.listener(Self::handle_toggle_command_pattern))
            .on_action(cx.listener(Self::open_permission_dropdown))
            .on_action(cx.listener(Self::open_add_context_menu))
            .on_action(cx.listener(Self::scroll_output_page_up))
            .on_action(cx.listener(Self::scroll_output_page_down))
            .on_action(cx.listener(Self::scroll_output_line_up))
            .on_action(cx.listener(Self::scroll_output_line_down))
            .on_action(cx.listener(Self::scroll_output_to_top))
            .on_action(cx.listener(Self::scroll_output_to_bottom))
            .on_action(cx.listener(Self::scroll_output_to_previous_message))
            .on_action(cx.listener(Self::scroll_output_to_next_message))
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(|this, _: &ToggleFastMode, window, cx| {
                this.toggle_fast_mode(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleThinkingMode, _window, cx| {
                if this.thread.read(cx).status() != ThreadStatus::Idle {
                    return;
                }
                if let Some(thread) = this.as_native_thread(cx) {
                    thread.update(cx, |thread, cx| {
                        let model_allows_disabling = thread
                            .model()
                            .is_none_or(|model| model.supports_disabling_thinking());
                        if model_allows_disabling {
                            thread.set_thinking_enabled(!thread.thinking_enabled(), cx);
                        }
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &CycleThinkingEffort, _window, cx| {
                if this.thread.read(cx).status() != ThreadStatus::Idle {
                    return;
                }
                if let Some(config_options_view) = this.config_options_view.clone() {
                    let handled = config_options_view.update(cx, |view, cx| {
                        view.cycle_category_option(
                            acp::SessionConfigOptionCategory::ThoughtLevel,
                            false,
                            cx,
                        )
                    });
                    if handled {
                        return;
                    }
                }
                this.cycle_native_agent_thinking_effort(cx);
            }))
            .on_action(
                cx.listener(|this, _: &ToggleThinkingEffortMenu, window, cx| {
                    if this.thread.read(cx).status() != ThreadStatus::Idle {
                        return;
                    }
                    if let Some(config_options_view) = this.config_options_view.clone() {
                        let handled = config_options_view.update(cx, |view, cx| {
                            view.toggle_category_picker(
                                acp::SessionConfigOptionCategory::ThoughtLevel,
                                window,
                                cx,
                            )
                        });
                        if handled {
                            return;
                        }
                    }
                    let menu_handle = this.thinking_effort_menu_handle.clone();
                    window.defer(cx, move |window, cx| {
                        menu_handle.toggle(window, cx);
                    });
                }),
            )
            .on_action(cx.listener(|this, _: &SendNextQueuedMessage, window, cx| {
                if let Some(id) = this.message_queue.first_id() {
                    this.send_queued_message_now(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RemoveFirstQueuedMessage, _, cx| {
                if let Some(id) = this.message_queue.first_id() {
                    this.remove_from_queue(id, cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &EditFirstQueuedMessage, window, cx| {
                if let Some(id) = this.message_queue.first_id() {
                    this.move_queued_message_to_main_editor(id, None, None, window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSteerFirstQueuedMessage, _, cx| {
                    if this.as_native_thread(cx).is_none() {
                        return;
                    }
                    if let Some(id) = this.message_queue.first_id() {
                        this.toggle_queue_entry_steer(id, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &ClearMessageQueue, _, cx| {
                this.clear_queue(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleProfileSelector, window, cx| {
                if let Some(config_options_view) = this.config_options_view.clone() {
                    let handled = config_options_view.update(cx, |view, cx| {
                        view.toggle_category_picker(
                            acp::SessionConfigOptionCategory::Mode,
                            window,
                            cx,
                        )
                    });
                    if handled {
                        return;
                    }
                }

                if let Some(profile_selector) = this.profile_selector.clone() {
                    profile_selector.read(cx).menu_handle().toggle(window, cx);
                } else if let Some(mode_selector) = this.mode_selector.clone() {
                    mode_selector.read(cx).menu_handle().toggle(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CycleModeSelector, window, cx| {
                if this.thread.read(cx).status() != ThreadStatus::Idle {
                    return;
                }
                if let Some(config_options_view) = this.config_options_view.clone() {
                    let handled = config_options_view.update(cx, |view, cx| {
                        view.cycle_category_option(
                            acp::SessionConfigOptionCategory::Mode,
                            false,
                            cx,
                        )
                    });
                    if handled {
                        return;
                    }
                }

                if let Some(profile_selector) = this.profile_selector.clone() {
                    profile_selector.update(cx, |profile_selector, cx| {
                        profile_selector.cycle_profile(cx);
                    });
                } else if let Some(mode_selector) = this.mode_selector.clone() {
                    mode_selector.update(cx, |mode_selector, cx| {
                        mode_selector.cycle_mode(window, cx);
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleModelSelector, window, cx| {
                if this.thread.read(cx).status() != ThreadStatus::Idle {
                    return;
                }
                if let Some(config_options_view) = this.config_options_view.clone() {
                    let handled = config_options_view.update(cx, |view, cx| {
                        view.toggle_category_picker(
                            acp::SessionConfigOptionCategory::Model,
                            window,
                            cx,
                        )
                    });
                    if handled {
                        return;
                    }
                }

                if let Some(model_selector) = this.model_selector.clone() {
                    model_selector
                        .update(cx, |model_selector, cx| model_selector.toggle(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CycleFavoriteModels, window, cx| {
                if this.thread.read(cx).status() != ThreadStatus::Idle {
                    return;
                }
                if let Some(config_options_view) = this.config_options_view.clone() {
                    let handled = config_options_view.update(cx, |view, cx| {
                        view.cycle_category_option(
                            acp::SessionConfigOptionCategory::Model,
                            true,
                            cx,
                        )
                    });
                    if handled {
                        return;
                    }
                }

                if let Some(model_selector) = this.model_selector.clone() {
                    model_selector.update(cx, |model_selector, cx| {
                        model_selector.cycle_favorite_models(window, cx);
                    });
                }
            }))
            .size_full()
            .children(self.render_subagent_titlebar(cx))
            .when_some(
                self.thread_search_visible
                    .then(|| self.thread_search_bar.clone())
                    .flatten(),
                |this, bar| this.child(bar),
            )
            .child(conversation)
            .children(self.render_multi_root_callout(cx))
            .children(self.render_activity_bar(window, cx))
            .when(self.show_external_source_prompt_warning, |this| {
                this.child(self.render_external_source_prompt_warning(cx))
            })
            .when(self.show_codex_windows_warning, |this| {
                this.child(self.render_codex_windows_warning(cx))
            })
            .children(self.render_skill_loading_issues(cx))
            .children(self.render_thread_retry_status_callout(cx))
            .children(self.render_thread_error(window, cx))
            .when_some(
                match has_messages {
                    true => None,
                    false => self.new_server_version_available.clone(),
                },
                |this, version| this.child(self.render_new_version_callout(&version, cx)),
            )
            .children(self.render_token_limit_callout(cx))
            .child(self.render_message_editor(window, cx))
    }
}
