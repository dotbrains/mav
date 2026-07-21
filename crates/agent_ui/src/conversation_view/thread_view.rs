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
#[path = "thread_view/context_compaction.rs"]
mod context_compaction;
#[path = "thread_view/data_retention_error.rs"]
mod data_retention_error;
#[path = "thread_view/draft_agent_selector.rs"]
mod draft_agent_selector;
#[path = "thread_view/edited_files.rs"]
mod edited_files;
#[path = "thread_view/edits_summary.rs"]
mod edits_summary;
#[path = "thread_view/fast_mode_controls.rs"]
mod fast_mode_controls;
#[path = "thread_view/fast_mode_warning.rs"]
mod fast_mode_warning;
#[path = "thread_view/feedback_editor.rs"]
mod feedback_editor;
#[path = "thread_view/feedback_state.rs"]
mod feedback_state;
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
#[path = "thread_view/native_command.rs"]
mod native_command;
#[path = "thread_view/navigation.rs"]
mod navigation;
#[path = "thread_view/numbered_code_block.rs"]
mod numbered_code_block;
#[path = "thread_view/permission_buttons.rs"]
mod permission_buttons;
#[path = "thread_view/permission_dropdown.rs"]
mod permission_dropdown;
#[path = "thread_view/plan_rendering.rs"]
mod plan_rendering;
#[path = "thread_view/queue_rendering.rs"]
mod queue_rendering;
#[path = "thread_view/sandbox_authorization_details.rs"]
mod sandbox_authorization_details;
#[path = "thread_view/sandbox_policy.rs"]
mod sandbox_policy;
#[path = "thread_view/sandbox_status.rs"]
mod sandbox_status;
#[path = "thread_view/sandbox_warning.rs"]
mod sandbox_warning;
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
#[path = "thread_view/tool_call_content.rs"]
mod tool_call_content;
#[path = "thread_view/tool_call_dispatch.rs"]
mod tool_call_dispatch;
#[path = "thread_view/tool_call_label.rs"]
mod tool_call_label;
#[path = "thread_view/tool_call_layout.rs"]
mod tool_call_layout;
#[path = "thread_view/tool_call_output.rs"]
mod tool_call_output;
#[path = "thread_view/tool_call_support.rs"]
mod tool_call_support;
#[path = "thread_view/ui_state.rs"]
mod ui_state;
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

    // sending

    fn clear_external_source_prompt_warning(&mut self, cx: &mut Context<Self>) {
        if self.show_external_source_prompt_warning {
            self.show_external_source_prompt_warning = false;
            cx.notify();
        }
    }

    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thread = &self.thread;

        if self.is_loading_contents {
            return;
        }

        let message_editor = self.message_editor.clone();

        let is_editor_empty = message_editor.read(cx).is_empty(cx);
        let is_generating = thread.read(cx).status() != ThreadStatus::Idle;

        if is_editor_empty {
            if let Some(entry) = self.message_queue.try_fast_track(is_generating) {
                self.dispatch_queued_entry(entry, window, cx);
            }
            return;
        }

        if is_generating {
            cx.emit(AcpThreadViewEvent::Interacted);
            self.queue_message(message_editor, window, cx);
            return;
        }

        let text = message_editor.read(cx).text(cx);
        let text = text.trim();
        if text == "/login" || text == "/logout" {
            let connection = thread.read(cx).connection().clone();
            let can_login = !connection.auth_methods().is_empty();
            // Does the agent have a specific logout command? Prefer that in case they need to reset internal state.
            let logout_supported = text == "/logout"
                && self
                    .session_capabilities
                    .read()
                    .available_commands()
                    .iter()
                    .any(|available_command| available_command.name == "logout");
            if can_login && !logout_supported {
                message_editor.update(cx, |editor, cx| editor.clear(window, cx));
                self.clear_external_source_prompt_warning(cx);

                let connection = self.thread.read(cx).connection().clone();
                window.defer(cx, {
                    let agent_id = self.agent_id.clone();
                    let server_view = self.server_view.clone();
                    move |window, cx| {
                        ConversationView::handle_auth_required(
                            server_view.clone(),
                            AuthRequired::new(),
                            agent_id,
                            connection,
                            window,
                            cx,
                        );
                    }
                });
                cx.notify();
                return;
            }
        }

        // A built-in command (e.g. `/compact`): run the bare command without
        // echoing it as a user message, and queue any trailing text the user
        // typed so it isn't silently dropped.
        let native_command =
            leading_native_command(text, self.session_capabilities.read().available_commands());
        if let Some(command_name) = native_command {
            cx.emit(AcpThreadViewEvent::Interacted);
            self.send_command_queueing_remainder(message_editor, command_name, window, cx);
            return;
        }

        cx.emit(AcpThreadViewEvent::Interacted);
        self.send_impl(message_editor, window, cx)
    }

    /// Sends a bare `/command` turn and queues everything the user typed after
    /// it as a follow-up message. The queued remainder auto-processes when the
    /// command turn stops, so e.g. `/compact do X` compacts and then runs `do X`
    /// rather than discarding it.
    fn send_command_queueing_remainder(
        &mut self,
        message_editor: Entity<MessageEditor>,
        command_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Resolve the editor contents before clearing it: the resolve task
        // reads the editor lazily, so clearing first would wipe the contents.
        let contents = self.resolve_message_contents(&message_editor, cx);
        self.thread_error.take();
        self.thread_feedback.clear();
        self.editing_message.take();

        cx.spawn_in(window, async move |this, cx| {
            let (mut content, tracked_buffers) = contents.await?;

            cx.update(|window, cx| {
                message_editor.update(cx, |message_editor, cx| {
                    message_editor.clear(window, cx);
                });
            })?;

            // Strip the leading `/command` from the first text block; whatever
            // remains (including any later mention blocks) becomes the queued
            // follow-up message.
            if let Some(acp::ContentBlock::Text(text_content)) = content.first_mut() {
                text_content.text = strip_leading_command(&text_content.text, &command_name);
            }
            if matches!(
                content.first(),
                Some(acp::ContentBlock::Text(text)) if text.text.trim().is_empty()
            ) {
                content.remove(0);
            }

            let command_block =
                acp::ContentBlock::Text(acp::TextContent::new(format!("/{command_name}")));

            this.update_in(cx, |this, window, cx| {
                // Queue the remainder first, then start the command turn; the
                // queue auto-processes when the command turn stops.
                if !content.is_empty() {
                    this.add_to_queue(content, tracked_buffers, window, cx);
                }
                this.send_content(
                    Task::ready(Ok(Some((vec![command_block], Vec::new())))),
                    true,
                    window,
                    cx,
                );
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn send_impl(
        &mut self,
        message_editor: Entity<MessageEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let contents = self.resolve_message_contents(&message_editor, cx);

        self.thread_error.take();
        self.thread_feedback.clear();
        self.editing_message.take();
        // Sending a message is active engagement: un-freeze the queue if it
        // was paused by a manual stop.
        self.message_queue.resume();

        if self.should_be_following {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.follow(CollaboratorId::Agent, window, cx);
                })
                .ok();
        }

        let contents_task = cx.spawn_in(window, async move |_this, cx| {
            let (contents, tracked_buffers) = contents.await?;

            if contents.is_empty() {
                return Ok(None);
            }

            let _ = cx.update(|window, cx| {
                message_editor.update(cx, |message_editor, cx| {
                    message_editor.clear(window, cx);
                });
            });

            Ok(Some((contents, tracked_buffers)))
        });

        self.send_content(contents_task, false, window, cx);
    }

    pub fn send_content(
        &mut self,
        contents_task: Task<anyhow::Result<Option<(Vec<acp::ContentBlock>, Vec<Entity<Buffer>>)>>>,
        is_native_command: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_id = self.thread.read(cx).session_id().clone();
        let parent_session_id = self.thread.read(cx).parent_session_id().cloned();
        let agent_telemetry_id = self.thread.read(cx).connection().telemetry_id();
        let is_first_message = self.thread.read(cx).entries().is_empty();
        let thread = self.thread.downgrade();

        self.is_loading_contents = true;

        let model_id = self.current_model_id(cx);
        let mode_id = self.current_mode_id(cx);
        let guard = cx.new(|_| ());
        cx.observe_release(&guard, |this, _guard, cx| {
            this.is_loading_contents = false;
            cx.notify();
        })
        .detach();

        let side = crate::sidebar_side(cx);

        let task = cx.spawn_in(window, async move |this, cx| {
            let Some((contents, tracked_buffers)) = contents_task.await? else {
                return Ok(());
            };

            let generation = this.update(cx, |this, cx| {
                this.clear_external_source_prompt_warning(cx);
                let generation = this.start_turn(cx);
                this.in_flight_prompt = Some(contents.clone());
                generation
            })?;

            this.update_in(cx, |this, _window, cx| {
                this.set_editor_is_expanded(false, cx);
            })?;

            let _ = this.update(cx, |this, cx| {
                this.list_state.scroll_to_end();
                cx.notify();
            });

            let _stop_turn = defer({
                let this = this.clone();
                let mut cx = cx.clone();
                move || {
                    this.update(&mut cx, |this, cx| {
                        this.stop_turn(generation, cx);
                        cx.notify();
                    })
                    .ok();
                }
            });
            if is_first_message && thread.read_with(cx, |thread, _cx| thread.title().is_none())? {
                let text: String = contents
                    .iter()
                    .filter_map(|block| match block {
                        acp::ContentBlock::Text(text_content) => Some(text_content.text.clone()),
                        acp::ContentBlock::ResourceLink(resource_link) => {
                            Some(format!("@{}", resource_link.name))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let text = text.lines().next().unwrap_or("").trim();
                if !text.is_empty() {
                    let title: SharedString = util::truncate_and_trailoff(text, 200).into();
                    thread.update(cx, |thread, cx| {
                        thread.set_provisional_title(title, cx);
                    })?;
                }
            }

            let turn_start_time = Instant::now();
            let send = thread.update(cx, |thread, cx| {
                thread.action_log().update(cx, |action_log, cx| {
                    for buffer in tracked_buffers {
                        action_log.buffer_read(buffer, cx)
                    }
                });
                drop(guard);

                telemetry::event!(
                    "Agent Message Sent",
                    agent = agent_telemetry_id,
                    session = session_id,
                    parent_session_id = parent_session_id.as_ref().map(|id| id.to_string()),
                    model = model_id,
                    mode = mode_id,
                    side = side
                );

                if is_native_command {
                    thread.send_command(contents, cx)
                } else {
                    thread.send(contents, cx)
                }
            })?;

            let _ = this.update(cx, |this, cx| {
                this.sync_generating_indicator(cx);
                cx.notify();
            });

            let res = send.await;
            let turn_time_ms = turn_start_time.elapsed().as_millis();
            drop(_stop_turn);
            let status = if res.is_ok() {
                let _ = this.update(cx, |this, _| this.in_flight_prompt.take());
                "success"
            } else {
                "failure"
            };
            telemetry::event!(
                "Agent Turn Completed",
                agent = agent_telemetry_id,
                session = session_id,
                parent_session_id = parent_session_id.as_ref().map(|id| id.to_string()),
                model = model_id,
                mode = mode_id,
                status,
                turn_time_ms,
                side = side
            );
            res.map(|_| ())
        });

        cx.spawn(async move |this, cx| {
            if let Err(err) = task.await {
                this.update(cx, |this, cx| {
                    this.handle_thread_error(err, cx);
                })
                .ok();
            } else {
                this.update(cx, |this, cx| {
                    let should_be_following = this
                        .workspace
                        .update(cx, |workspace, _| {
                            workspace.is_being_followed(CollaboratorId::Agent)
                        })
                        .unwrap_or_default();
                    this.should_be_following = should_be_following;
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn interrupt_and_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thread = &self.thread;

        if self.is_loading_contents {
            return;
        }

        cx.emit(AcpThreadViewEvent::Interacted);

        let message_editor = self.message_editor.clone();
        if thread.read(cx).status() == ThreadStatus::Idle {
            self.send_impl(message_editor, window, cx);
            return;
        }

        self.stop_current_and_send_new_message(message_editor, window, cx);
    }

    fn stop_current_and_send_new_message(
        &mut self,
        message_editor: Entity<MessageEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = self.thread.clone();
        self.message_queue.pause();

        let cancelled = thread.update(cx, |thread, cx| thread.cancel(cx));

        cx.spawn_in(window, async move |this, cx| {
            cancelled.await;

            this.update_in(cx, |this, window, cx| {
                this.send_impl(message_editor, window, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn handle_thread_error(
        &mut self,
        error: impl Into<ThreadError>,
        cx: &mut Context<Self>,
    ) {
        let error = error.into();
        self.emit_thread_error_telemetry(&error, cx);
        self.thread_error = Some(error);
        cx.notify();
    }

    fn emit_thread_error_telemetry(&self, error: &ThreadError, cx: &mut Context<Self>) {
        let (error_kind, acp_error_code, message): (&str, Option<SharedString>, SharedString) =
            match error {
                ThreadError::PaymentRequired => (
                    "payment_required",
                    None,
                    "You reached your free usage limit. Upgrade to Mav Pro for more prompts."
                        .into(),
                ),
                ThreadError::Refusal => {
                    let model_or_agent_name = self.current_model_name(cx);
                    let message = format!(
                        "{} refused to respond to this prompt. This can happen when a model believes the prompt violates its content policy or safety guidelines, so rephrasing it can sometimes address the issue.",
                        model_or_agent_name
                    );
                    ("refusal", None, message.into())
                }
                ThreadError::DataRetentionConsentRequired => {
                    let message = format!(
                        "{} is not available with Zero Data Retention.",
                        self.current_model_name(cx)
                    );
                    ("data_retention_consent_required", None, message.into())
                }
                ThreadError::AuthenticationRequired(message) => {
                    ("authentication_required", None, message.clone())
                }
                ThreadError::RateLimitExceeded { provider } => (
                    "rate_limit_exceeded",
                    None,
                    format!("{provider}'s rate limit was reached.").into(),
                ),
                ThreadError::ServerOverloaded { provider } => (
                    "server_overloaded",
                    None,
                    format!("{provider}'s servers are temporarily unavailable.").into(),
                ),
                ThreadError::PromptTooLarge => (
                    "prompt_too_large",
                    None,
                    "Context too large for the model's context window.".into(),
                ),
                ThreadError::NoCredentials { provider } => (
                    "no_api_key",
                    None,
                    format!("No credentials configured for {provider}.").into(),
                ),
                ThreadError::StreamError { provider } => (
                    "stream_error",
                    None,
                    format!("Connection to {provider}'s API was interrupted.").into(),
                ),
                ThreadError::AuthenticationFailed { provider } => (
                    "invalid_api_key",
                    None,
                    format!("Authentication with {provider} failed.").into(),
                ),
                ThreadError::PermissionDenied { provider, message } => (
                    "permission_denied",
                    None,
                    message.clone().unwrap_or_else(|| {
                        format!(
                            "{provider}'s API rejected the request due to insufficient permissions."
                        )
                        .into()
                    }),
                ),
                ThreadError::RequestFailed => (
                    "request_failed",
                    None,
                    "Request could not be completed after multiple attempts.".into(),
                ),
                ThreadError::MaxOutputTokens => (
                    "max_output_tokens",
                    None,
                    "Model reached its maximum output length.".into(),
                ),
                ThreadError::NoModelSelected => {
                    ("no_model_selected", None, "No model selected.".into())
                }
                ThreadError::ApiError { provider } => (
                    "api_error",
                    None,
                    format!("{provider}'s API returned an unexpected error.").into(),
                ),
                ThreadError::Other {
                    acp_error_code,
                    message,
                } => ("other", acp_error_code.clone(), message.clone()),
            };

        let agent_telemetry_id = self.thread.read(cx).connection().telemetry_id();
        let session_id = self.thread.read(cx).session_id().clone();
        let parent_session_id = self
            .thread
            .read(cx)
            .parent_session_id()
            .map(|id| id.to_string());

        telemetry::event!(
            "Agent Panel Error Shown",
            agent = agent_telemetry_id,
            session_id = session_id,
            parent_session_id = parent_session_id,
            kind = error_kind,
            acp_error_code = acp_error_code,
            message = message,
        );
    }

    pub fn cancel_generation(&mut self, cx: &mut Context<Self>) {
        self.thread_retry_status.take();
        self.thread_error.take();
        self.message_queue.pause();
        self._cancel_task = Some(self.thread.update(cx, |thread, cx| thread.cancel(cx)));
        self.sync_generating_indicator(cx);
        cx.notify();
    }

    pub fn retry_generation(&mut self, cx: &mut Context<Self>) {
        self.thread_error.take();

        let thread = &self.thread;
        if !thread.read(cx).can_retry(cx) {
            return;
        }

        let task = thread.update(cx, |thread, cx| thread.retry(cx));
        cx.emit(AcpThreadViewEvent::Interacted);
        self.sync_generating_indicator(cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.handle_thread_error(err, cx);
                }
            })
        })
        .detach();
    }

    pub fn regenerate(
        &mut self,
        entry_ix: usize,
        message_editor: Entity<MessageEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_loading_contents {
            return;
        }
        let thread = self.thread.clone();

        let Some(client_id) = thread.update(cx, |thread, _| {
            thread
                .entries()
                .get(entry_ix)?
                .user_message()?
                .client_id
                .clone()
        }) else {
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            // Check if there are any edits from prompts before the one being regenerated.
            //
            // If there are, we keep/accept them since we're not regenerating the prompt that created them.
            //
            // If editing the prompt that generated the edits, they are auto-rejected
            // through the `rewind` function in the `acp_thread`.
            //
            // Subagent edits never show up as diffs in the parent thread's entries (they
            // are only forwarded to the parent's action log), so treat any earlier
            // subagent tool call as potentially having edits. Keeping all edits is a
            // no-op when the subagent didn't make any.
            let has_earlier_edits = thread.read_with(cx, |thread, _| {
                thread.entries().iter().take(entry_ix).any(|entry| {
                    entry.diffs().next().is_some()
                        || matches!(
                            entry,
                            AgentThreadEntry::ToolCall(tool_call) if tool_call.is_subagent()
                        )
                })
            });

            if has_earlier_edits {
                thread.update(cx, |thread, cx| {
                    thread.action_log().update(cx, |action_log, cx| {
                        action_log.keep_all_edits(None, cx);
                    });
                });
            }

            thread
                .update(cx, |thread, cx| thread.rewind(client_id, cx))
                .await?;
            this.update_in(cx, |thread, window, cx| {
                cx.emit(AcpThreadViewEvent::Interacted);
                thread.send_impl(message_editor, window, cx);
                thread.focus_handle(cx).focus(window, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    // message queueing

    fn queue_message(
        &mut self,
        message_editor: Entity<MessageEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_idle = self.thread.read(cx).status() == acp_thread::ThreadStatus::Idle;

        if is_idle {
            self.send_impl(message_editor, window, cx);
            return;
        }

        let contents = self.resolve_message_contents(&message_editor, cx);

        cx.spawn_in(window, async move |this, cx| {
            let (content, tracked_buffers) = contents.await?;

            if content.is_empty() {
                return Ok::<(), anyhow::Error>(());
            }

            this.update_in(cx, |this, window, cx| {
                this.add_to_queue(content, tracked_buffers, window, cx);
                message_editor.update(cx, |message_editor, cx| {
                    message_editor.clear(window, cx);
                });
                cx.notify();
            })?;
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn add_to_queue(
        &mut self,
        content: Vec<acp::ContentBlock>,
        tracked_buffers: Vec<Entity<Buffer>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The ID must be allocated up front so the editor event subscription
        // can capture it before the entry (which owns the subscription) exists.
        let id = self.message_queue.next_id();

        let editor = cx.new(|cx| {
            let mut editor = MessageEditor::new(
                self.workspace.clone(),
                self.project.clone(),
                None,
                self.session_capabilities.clone(),
                self.agent_id.clone(),
                "",
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: Some(10),
                },
                window,
                cx,
            );
            editor.set_read_only(true, cx);
            editor.set_message(content.clone(), window, cx);
            editor
        });

        let subscription =
            cx.subscribe_in(&editor, window, move |this, _editor, event, window, cx| {
                this.handle_queue_editor_event(id, event, window, cx);
            });

        self.message_queue.enqueue(QueueEntry {
            id,
            content,
            tracked_buffers,
            steer: false,
            editor,
            _subscription: subscription,
        });
        self.sync_queue_flag_to_native_thread(cx);
        cx.notify();
    }

    fn handle_queue_editor_event(
        &mut self,
        id: QueueEntryId,
        event: &MessageEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            MessageEditorEvent::InputAttempted {
                attempt,
                cursor_offset,
            } => {
                self.move_queued_message_to_main_editor(
                    id,
                    Some(attempt.clone()),
                    Some(*cursor_offset),
                    window,
                    cx,
                );
            }
            MessageEditorEvent::LostFocus => {
                self.save_queued_message(id, cx);
            }
            MessageEditorEvent::Cancel | MessageEditorEvent::Send => {
                window.focus(&self.message_editor.focus_handle(cx), cx);
            }
            MessageEditorEvent::SendImmediately => {
                self.send_queued_message_now(id, window, cx);
            }
            _ => {}
        }
    }

    fn save_queued_message(&mut self, id: QueueEntryId, cx: &mut Context<Self>) {
        let Some(entry) = self.message_queue.entry_by_id(id) else {
            return;
        };
        let contents_task = entry
            .editor
            .update(cx, |editor, cx| editor.contents(false, cx));

        cx.spawn(async move |this, cx| {
            let (content, tracked_buffers) = contents_task.await?;

            this.update(cx, |this, cx| {
                if let Some(entry) = this.message_queue.entry_by_id_mut(id) {
                    entry.content = content;
                    entry.tracked_buffers = tracked_buffers;
                }
                cx.notify();
            })?;

            Ok::<(), anyhow::Error>(())
        })
        .detach_and_log_err(cx);
    }

    pub fn remove_from_queue(
        &mut self,
        id: QueueEntryId,
        cx: &mut Context<Self>,
    ) -> Option<QueueEntry> {
        let removed = self.message_queue.remove(id);
        if removed.is_some() {
            self.sync_queue_flag_to_native_thread(cx);
        }
        removed
    }

    fn toggle_queue_entry_steer(&mut self, id: QueueEntryId, cx: &mut Context<Self>) {
        self.message_queue.toggle_steer(id);
        self.sync_queue_flag_to_native_thread(cx);
        cx.notify();
    }

    pub fn sync_queue_flag_to_native_thread(&self, cx: &mut Context<Self>) {
        if let Some(native_thread) = self.as_native_thread(cx) {
            // By default queued messages wait for the turn to fully complete.
            // Only a "steering" front message ends the turn at the next boundary.
            let end_at_boundary = self.message_queue.front_wants_steer();
            native_thread.update(cx, |thread, _| {
                thread.set_end_turn_at_next_boundary(end_at_boundary);
            });
        }
    }

    pub fn send_queued_message_now(
        &mut self,
        id: QueueEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_generating = self.thread.read(cx).status() == acp_thread::ThreadStatus::Generating;
        if let Some(entry) = self.message_queue.send_now(id, is_generating) {
            self.dispatch_queued_entry(entry, window, cx);
        }
    }

    /// The shared "actually send this entry" path, used by fast-track,
    /// auto-processing on Stopped, and "Send Now". The entry must already have
    /// been removed from the queue.
    pub fn dispatch_queued_entry(
        &mut self,
        entry: QueueEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_queue_flag_to_native_thread(cx);

        cx.emit(AcpThreadViewEvent::Interacted);

        self.message_editor.focus_handle(cx).focus(window, cx);

        let content = entry.content;
        let tracked_buffers = entry.tracked_buffers;

        // A queued message can itself be a built-in command (e.g. the user typed
        // `/compact` while a turn was generating). Detect that so we run it as a
        // command turn without echoing it as a user message, matching the
        // non-queued path.
        let is_native_command = content
            .first()
            .and_then(|block| match block {
                acp::ContentBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .and_then(|text| {
                leading_native_command(text, self.session_capabilities.read().available_commands())
            })
            .is_some();

        let cancelled = self.thread.update(cx, |thread, cx| thread.cancel(cx));

        let workspace = self.workspace.clone();

        let should_be_following = self.should_be_following;
        let contents_task = cx.spawn_in(window, async move |_this, cx| {
            cancelled.await;
            if should_be_following {
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.follow(CollaboratorId::Agent, window, cx);
                    })
                    .ok();
            }

            Ok(Some((content, tracked_buffers)))
        });

        self.send_content(contents_task, is_native_command, window, cx);
    }

    pub fn move_queued_message_to_main_editor(
        &mut self,
        id: QueueEntryId,
        attempt: Option<InputAttempt>,
        cursor_offset: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(queued_message) = self.remove_from_queue(id, cx) else {
            return false;
        };
        let queued_content = queued_message.content;
        let message_editor = self.message_editor.clone();

        window.focus(&message_editor.focus_handle(cx), cx);

        let adjusted_cursor_offset = if message_editor.read(cx).is_empty(cx) {
            message_editor.update(cx, |editor, cx| {
                editor.set_message(queued_content, window, cx);
            });
            cursor_offset
        } else {
            let existing_len = message_editor.read(cx).text(cx).len();
            let separator = "\n\n";
            message_editor.update(cx, |editor, cx| {
                editor.append_message(queued_content, Some(separator), window, cx);
            });
            cursor_offset.map(|offset| existing_len + separator.len() + offset)
        };

        message_editor.update(cx, |editor, cx| {
            if let Some(offset) = adjusted_cursor_offset {
                editor.set_cursor_offset(offset, window, cx);
            }
            match attempt {
                Some(InputAttempt::Text(text)) => {
                    editor.insert_text(&text, window, cx);
                }
                Some(InputAttempt::Paste(clipboard)) => {
                    editor.paste_item(&clipboard, window, cx);
                }
                None => {}
            }
        });

        cx.notify();
        true
    }

    fn handle_message_editor_move_up(
        &mut self,
        _: &mav_actions::editor::MoveUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.message_editor.read(cx).is_empty(cx) {
            cx.propagate();
            return;
        }
        let Some(last_id) = self.message_queue.last_id() else {
            cx.propagate();
            return;
        };
        self.move_queued_message_to_main_editor(last_id, None, None, window, cx);
    }

    // editor methods

    pub fn expand_message_editor(
        &mut self,
        _: &ExpandMessageEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.list_state.item_count() == 0 {
            return;
        }
        self.set_editor_is_expanded(!self.editor_expanded, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub fn set_editor_is_expanded(&mut self, is_expanded: bool, cx: &mut Context<Self>) {
        self.editor_expanded = is_expanded;
        self.message_editor.update(cx, |editor, cx| {
            if is_expanded {
                editor.set_mode(
                    EditorMode::Full {
                        scale_ui_elements_with_buffer_font_size: false,
                        show_active_line_background: false,
                        sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
                    },
                    cx,
                )
            } else {
                let agent_settings = AgentSettings::get_global(cx);
                editor.set_mode(
                    EditorMode::AutoHeight {
                        min_lines: agent_settings.message_editor_min_lines,
                        max_lines: Some(agent_settings.set_message_editor_max_lines()),
                    },
                    cx,
                )
            }
        });
        cx.notify();
    }

    pub fn handle_title_editor_event(
        &mut self,
        title_editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::BufferEdited => {
                // We only want to set the title if the user has actively edited
                // it. If the title editor is not focused, we programmatically
                // changed the text, so we don't want to set the title again.
                if !title_editor.read(cx).is_focused(window) {
                    return;
                }

                let new_title = title_editor.read(cx).text(cx);
                if new_title.is_empty() {
                    return;
                }
                self.apply_renamed_title(SharedString::from(new_title), cx);
            }
            EditorEvent::Blurred => {
                if title_editor.read(cx).text(cx).is_empty() {
                    title_editor.update(cx, |editor, cx| {
                        editor.set_text(DEFAULT_THREAD_TITLE, window, cx);
                    });
                }
            }
            _ => {}
        }
    }

    /// Renames the thread, mirroring the editor text and persisting the new
    /// title. Used by callers outside of the title editor (e.g. the sidebar's
    /// inline rename) so that they go through the same persistence path as
    /// the in-thread title editor.
    pub fn rename(&mut self, title: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        if self.title_editor.read(cx).text(cx) != title.as_ref() {
            self.title_editor.update(cx, |editor, cx| {
                editor.set_text(title.clone(), window, cx);
            });
        }
        self.apply_renamed_title(title, cx);
    }

    fn apply_renamed_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        if let Some(store) = ThreadMetadataStore::try_global(cx)
            && !self.is_subagent()
        {
            let thread_id = self.root_thread_id;
            store.update(cx, |store, cx| {
                store.set_title_override(thread_id, title.clone(), cx);
            });
        }
        self.thread.update(cx, |thread, cx| {
            if thread.can_set_title(cx) {
                thread.set_title(title, cx).detach_and_log_err(cx);
            }
        });
    }

    pub fn cancel_editing(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_message.take()
            && let Some(editor) = &self
                .entry_view_state
                .read(cx)
                .entry(index)
                .and_then(|e| e.message_editor())
                .cloned()
        {
            editor.update(cx, |editor, cx| {
                if let Some(user_message) = self
                    .thread
                    .read(cx)
                    .entries()
                    .get(index)
                    .and_then(|e| e.user_message())
                {
                    editor.set_message(user_message.chunks.clone(), window, cx);
                }
            })
        };
        self.message_editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub fn authorize_tool_call(
        &mut self,
        session_id: acp::SessionId,
        tool_call_id: acp::ToolCallId,
        outcome: SelectedPermissionOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.conversation.update(cx, |conversation, cx| {
            conversation.authorize_tool_call(session_id, tool_call_id, outcome, cx);
        });
        if self.should_be_following {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.follow(CollaboratorId::Agent, window, cx);
                })
                .ok();
        }
        cx.notify();
    }

    pub fn allow_always(&mut self, _: &AllowAlways, window: &mut Window, cx: &mut Context<Self>) {
        self.authorize_pending_tool_call(acp::PermissionOptionKind::AllowAlways, window, cx);
    }

    pub fn allow_once(&mut self, _: &AllowOnce, window: &mut Window, cx: &mut Context<Self>) {
        self.authorize_pending_with_granularity(true, window, cx);
    }

    pub fn reject_once(&mut self, _: &RejectOnce, window: &mut Window, cx: &mut Context<Self>) {
        self.authorize_pending_with_granularity(false, window, cx);
    }

    pub fn authorize_pending_tool_call(
        &mut self,
        kind: acp::PermissionOptionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let session_id = self.thread.read(cx).session_id().clone();
        self.conversation.update(cx, |conversation, cx| {
            conversation.authorize_pending_tool_call(&session_id, kind, cx)
        })?;
        if self.should_be_following {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.follow(CollaboratorId::Agent, window, cx);
                })
                .ok();
        }
        cx.notify();
        Some(())
    }

    fn is_waiting_for_confirmation(entry: &AgentThreadEntry) -> bool {
        if let AgentThreadEntry::ToolCall(tool_call) = entry {
            matches!(
                tool_call.status,
                ToolCallStatus::WaitingForConfirmation { .. }
            )
        } else {
            false
        }
    }

    fn handle_authorize_tool_call(
        &mut self,
        action: &AuthorizeToolCall,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tool_call_id = acp::ToolCallId::new(action.tool_call_id.clone());
        let option_id = acp::PermissionOptionId::new(action.option_id.clone());
        let option_kind = match action.option_kind.as_str() {
            "AllowOnce" => acp::PermissionOptionKind::AllowOnce,
            "AllowAlways" => acp::PermissionOptionKind::AllowAlways,
            "RejectOnce" => acp::PermissionOptionKind::RejectOnce,
            "RejectAlways" => acp::PermissionOptionKind::RejectAlways,
            _ => acp::PermissionOptionKind::AllowOnce,
        };

        let session_id = self.thread.read(cx).session_id().clone();
        self.authorize_tool_call(
            session_id,
            tool_call_id,
            SelectedPermissionOutcome::new(option_id, option_kind),
            window,
            cx,
        );
    }

    pub fn handle_select_permission_granularity(
        &mut self,
        action: &SelectPermissionGranularity,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tool_call_id = acp::ToolCallId::new(action.tool_call_id.clone());
        self.permission_selections
            .insert(tool_call_id, PermissionSelection::Choice(action.index));

        cx.notify();
    }

    pub fn handle_toggle_command_pattern(
        &mut self,
        action: &crate::ToggleCommandPattern,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tool_call_id = acp::ToolCallId::new(action.tool_call_id.clone());

        match self.permission_selections.get_mut(&tool_call_id) {
            Some(PermissionSelection::SelectedPatterns(checked)) => {
                // Already in pattern mode — toggle the individual pattern.
                if let Some(pos) = checked.iter().position(|&i| i == action.pattern_index) {
                    checked.swap_remove(pos);
                } else {
                    checked.push(action.pattern_index);
                }
            }
            _ => {
                // First click: activate "Select options" with all patterns checked.
                let thread = self.thread.read(cx);
                let pattern_count = thread
                    .entries()
                    .iter()
                    .find_map(|entry| {
                        if let AgentThreadEntry::ToolCall(call) = entry {
                            if call.id == tool_call_id {
                                if let ToolCallStatus::WaitingForConfirmation { options, .. } =
                                    &call.status
                                {
                                    if let PermissionOptions::DropdownWithPatterns {
                                        patterns,
                                        ..
                                    } = options
                                    {
                                        return Some(patterns.len());
                                    }
                                }
                            }
                        }
                        None
                    })
                    .unwrap_or(0);
                self.permission_selections.insert(
                    tool_call_id,
                    PermissionSelection::SelectedPatterns((0..pattern_count).collect()),
                );
            }
        }
        cx.notify();
    }

    fn authorize_pending_with_granularity(
        &mut self,
        is_allow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let session_id = self.thread.read(cx).session_id().clone();
        let (returned_session_id, tool_call_id, _) = self
            .conversation
            .read(cx)
            .pending_tool_call(&session_id, cx)?;
        self.authorize_with_granularity(returned_session_id, tool_call_id, is_allow, window, cx)
    }

    fn authorize_with_granularity(
        &mut self,
        session_id: acp::SessionId,
        tool_call_id: acp::ToolCallId,
        is_allow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let selection = self.permission_selections.get(&tool_call_id).cloned();
        let result = self.conversation.update(cx, |conversation, cx| {
            conversation.authorize_with_granularity(
                session_id,
                tool_call_id,
                selection.as_ref(),
                is_allow,
                cx,
            )
        });
        if self.should_be_following {
            self.workspace
                .update(cx, |workspace, cx| {
                    workspace.follow(CollaboratorId::Agent, window, cx);
                })
                .ok();
        }
        cx.notify();
        result
    }

    // edits

    pub fn keep_all(&mut self, _: &KeepAll, _window: &mut Window, cx: &mut Context<Self>) {
        let thread = &self.thread;
        let telemetry = ActionLogTelemetry::from(thread.read(cx));
        let action_log = thread.read(cx).action_log().clone();
        action_log.update(cx, |action_log, cx| {
            action_log.keep_all_edits(Some(telemetry), cx)
        });
    }

    pub fn reject_all(&mut self, _: &RejectAll, _window: &mut Window, cx: &mut Context<Self>) {
        let thread = &self.thread;
        let telemetry = ActionLogTelemetry::from(thread.read(cx));
        let action_log = thread.read(cx).action_log().clone();
        let has_changes = action_log.read(cx).changed_buffers(cx).next().is_some();

        action_log
            .update(cx, |action_log, cx| {
                action_log.reject_all_edits(Some(telemetry), cx)
            })
            .detach();

        if has_changes {
            if let Some(workspace) = self.workspace.upgrade() {
                workspace.update(cx, |workspace, cx| {
                    crate::ui::show_undo_reject_toast(workspace, action_log, cx);
                });
            }
        }
    }

    pub fn undo_last_reject(
        &mut self,
        _: &UndoLastReject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = &self.thread;
        let action_log = thread.read(cx).action_log().clone();
        action_log
            .update(cx, |action_log, cx| action_log.undo_last_reject(cx))
            .detach()
    }

    pub fn open_edited_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = &self.thread;

        let Some(diff) =
            AgentDiffPane::deploy(thread.clone(), self.workspace.clone(), window, cx).log_err()
        else {
            return;
        };

        diff.update(cx, |diff, cx| {
            diff.move_to_path(PathKey::for_buffer(buffer, cx), window, cx)
        })
    }

    // thread stuff

    pub fn restore_checkpoint(&mut self, client_id: &ClientUserMessageId, cx: &mut Context<Self>) {
        self.thread
            .update(cx, |thread, cx| {
                thread.restore_checkpoint(client_id.clone(), cx)
            })
            .detach_and_log_err(cx);
    }

    pub fn clear_thread_error(&mut self, cx: &mut Context<Self>) {
        self.thread_error = None;
        self.thread_error_markdown = None;
        self.token_limit_callout_dismissed = true;
        cx.notify();
    }

    fn is_following(&self, cx: &App) -> bool {
        match self.thread.read(cx).status() {
            ThreadStatus::Generating => self
                .workspace
                .read_with(cx, |workspace, _| {
                    workspace.is_being_followed(CollaboratorId::Agent)
                })
                .unwrap_or(false),
            _ => self.should_be_following,
        }
    }

    fn toggle_following(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let following = self.is_following(cx);

        self.should_be_following = !following;
        if self.thread.read(cx).status() == ThreadStatus::Generating {
            self.workspace
                .update(cx, |workspace, cx| {
                    if following {
                        workspace.unfollow(CollaboratorId::Agent, window, cx);
                    } else {
                        workspace.follow(CollaboratorId::Agent, window, cx);
                    }
                })
                .ok();
        }

        telemetry::event!("Follow Agent Selected", following = !following);
    }

    fn callout_border_position(&self) -> CalloutBorderPosition {
        if self.list_state.item_count() > 0 {
            CalloutBorderPosition::Top
        } else {
            CalloutBorderPosition::Bottom
        }
    }

    pub fn render_thread_retry_status_callout(&self, cx: &mut Context<Self>) -> Option<Callout> {
        let state = self.thread_retry_status.as_ref()?;

        if let Some(fallback_model) = acp_thread::refusal_fallback_model_from_meta(&state.meta) {
            return Some(
                Callout::new()
                    .icon(IconName::Warning)
                    .severity(Severity::Warning)
                    .title(state.last_error.clone())
                    .description(format!("Retrying with {fallback_model}"))
                    .dismiss_action(
                        IconButton::new("dismiss-refusal-fallback", IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Dismiss"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.thread_retry_status = None;
                                cx.notify();
                            })),
                    ),
            );
        }

        let next_attempt_in = state
            .duration
            .saturating_sub(Instant::now().saturating_duration_since(state.started_at));
        if next_attempt_in.is_zero() {
            return None;
        }

        let next_attempt_in_secs = next_attempt_in.as_secs() + 1;

        let retry_message = if state.max_attempts == 1 {
            if next_attempt_in_secs == 1 {
                "Retrying. Next attempt in 1 second.".to_string()
            } else {
                format!("Retrying. Next attempt in {next_attempt_in_secs} seconds.")
            }
        } else if next_attempt_in_secs == 1 {
            format!(
                "Retrying. Next attempt in 1 second (Attempt {} of {}).",
                state.attempt, state.max_attempts,
            )
        } else {
            format!(
                "Retrying. Next attempt in {next_attempt_in_secs} seconds (Attempt {} of {}).",
                state.attempt, state.max_attempts,
            )
        };

        Some(
            Callout::new()
                .border_position(self.callout_border_position())
                .icon(IconName::Warning)
                .severity(Severity::Warning)
                .title(state.last_error.clone())
                .description(retry_message),
        )
    }

    fn collect_subagent_items_for_sessions(
        entries: &[AgentThreadEntry],
        awaiting_session_ids: &[acp::SessionId],
        cx: &App,
    ) -> Vec<(SharedString, usize)> {
        let tool_calls_by_session: HashMap<_, _> = entries
            .iter()
            .enumerate()
            .filter_map(|(entry_ix, entry)| {
                let AgentThreadEntry::ToolCall(tool_call) = entry else {
                    return None;
                };
                let info = tool_call.subagent_session_info.as_ref()?;
                let summary_text = tool_call.label.read(cx).source().to_string();
                let subagent_summary = if summary_text.is_empty() {
                    SharedString::from("Subagent")
                } else {
                    SharedString::from(summary_text)
                };
                Some((info.session_id.clone(), (subagent_summary, entry_ix)))
            })
            .collect();

        awaiting_session_ids
            .iter()
            .filter_map(|session_id| tool_calls_by_session.get(session_id).cloned())
            .collect()
    }

    fn render_subagents_awaiting_permission(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let awaiting = self.conversation.read(cx).subagents_awaiting_permission(cx);

        if awaiting.is_empty() {
            return None;
        }

        let awaiting_session_ids: Vec<_> = awaiting
            .iter()
            .map(|(session_id, _)| session_id.clone())
            .collect();

        let thread = self.thread.read(cx);
        let entries = thread.entries();
        let subagent_items =
            Self::collect_subagent_items_for_sessions(entries, &awaiting_session_ids, cx);

        if subagent_items.is_empty() {
            return None;
        }

        let item_count = subagent_items.len();

        Some(
            v_flex()
                .child(
                    h_flex()
                        .py_1()
                        .px_2()
                        .w_full()
                        .gap_1()
                        .border_b_1()
                        .border_color(cx.theme().colors().border)
                        .child(
                            Label::new("Subagents Awaiting Permission:")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(Label::new(item_count.to_string()).size(LabelSize::Small)),
                )
                .child(
                    v_flex().children(subagent_items.into_iter().enumerate().map(
                        |(ix, (label, entry_ix))| {
                            let is_last = ix == item_count - 1;
                            let group = format!("group-{}", entry_ix);

                            h_flex()
                                .cursor_pointer()
                                .id(format!("subagent-permission-{}", entry_ix))
                                .group(&group)
                                .p_1()
                                .pl_2()
                                .min_w_0()
                                .w_full()
                                .gap_1()
                                .justify_between()
                                .bg(cx.theme().colors().editor_background)
                                .hover(|s| s.bg(cx.theme().colors().element_hover))
                                .when(!is_last, |this| {
                                    this.border_b_1().border_color(cx.theme().colors().border)
                                })
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .child(
                                            Icon::new(IconName::Circle)
                                                .size(IconSize::XSmall)
                                                .color(Color::Warning),
                                        )
                                        .child(
                                            Label::new(label)
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .truncate(),
                                        ),
                                )
                                .child(
                                    div().visible_on_hover(&group).child(
                                        Label::new("Scroll to Subagent")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .truncate(),
                                    ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.list_state.scroll_to(ListOffset {
                                        item_ix: entry_ix,
                                        offset_in_item: px(0.0),
                                    });
                                    cx.notify();
                                }))
                        },
                    )),
                )
                .into_any(),
        )
    }

    pub(crate) fn render_main_agent_awaiting_permission(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if self.is_subagent() {
            return None;
        }

        let active_session_id = self.thread.read(cx).session_id().clone();
        let conversation = self.conversation.read(cx);
        let tool_call_id = conversation.pending_tool_call_for_session(&active_session_id, cx)?;
        let pending_count = conversation.pending_tool_call_count_for_session(&active_session_id);

        let thread = self.thread.read(cx);
        let (entry_ix, tool_call) = thread.tool_call(&tool_call_id)?;

        let scroll_icon = if self.list_state.item_is_above_viewport(entry_ix)? {
            IconName::ArrowUp
        } else if self.list_state.item_is_below_viewport(entry_ix)? {
            IconName::ArrowDown
        } else {
            return None;
        };

        let focus_handle = self.focus_handle(cx);

        let card = self.render_any_tool_call(
            &active_session_id,
            entry_ix,
            tool_call,
            &focus_handle,
            ToolCallLayout::Floating,
            window,
            cx,
        );

        let label: SharedString = if pending_count > 1 {
            format!("Awaiting Confirmation ({pending_count})").into()
        } else {
            "Awaiting Confirmation".into()
        };

        let header = h_flex()
            .p_1p5()
            .pl_2()
            .w_full()
            .gap_1p5()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .w_2()
                            .justify_center()
                            .child(GeneratingSpinnerElement::new(SpinnerVariant::Sand)),
                    )
                    .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
            )
            .child(
                Button::new("main-agent-permission-scroll-to", "Scroll")
                    .label_size(LabelSize::Small)
                    .end_icon(
                        Icon::new(scroll_icon)
                            .size(IconSize::XSmall)
                            .color(Color::Default),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.list_state.scroll_to(ListOffset {
                            item_ix: entry_ix,
                            offset_in_item: px(0.0),
                        });
                        cx.notify();
                    })),
            );

        Some(v_flex().child(header).child(card).into_any())
    }

    pub(crate) fn render_message_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.is_subagent() {
            return div().into_any_element();
        }

        let focus_handle = self.message_editor.focus_handle(cx);
        let editor_bg_color = cx.theme().colors().editor_background;

        let editor_expanded = self.editor_expanded;
        let (expand_icon, expand_tooltip) = if editor_expanded {
            (IconName::Minimize, "Minimize Message Editor")
        } else {
            (IconName::Maximize, "Expand Message Editor")
        };

        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        let has_messages = self.list_state.item_count() > 0;
        let compact_editor = has_messages || self.is_draft(cx);
        let fills_container = !compact_editor || editor_expanded;
        let draft_agent_selector = self
            .is_draft(cx)
            .then(|| self.render_draft_agent_selector(cx));

        h_flex()
            .py_2()
            .bg(editor_bg_color)
            .justify_center()
            .on_action(cx.listener(Self::handle_message_editor_move_up))
            .map(|this| {
                if compact_editor {
                    this.on_action(cx.listener(Self::expand_message_editor))
                        .flex_none()
                        .border_t_1()
                        .border_color(cx.theme().colors().border)
                        .when(editor_expanded, |this| this.h(vh(0.8, window)))
                } else {
                    this.flex_1().size_full()
                }
            })
            .child(
                v_flex()
                    .when_some(max_content_width, |this, max_w| this.flex_basis(max_w))
                    .when(max_content_width.is_none(), |this| this.w_full())
                    .when(fills_container, |this| this.h_full())
                    .px_2()
                    .flex_shrink_1()
                    .flex_grow_0()
                    .justify_between()
                    .gap_2()
                    .child(
                        v_flex()
                            .relative()
                            .w_full()
                            .min_h_0()
                            .when(fills_container, |this| this.flex_1())
                            .pt_1()
                            .pr_2p5()
                            .child(self.message_editor.clone())
                            .when(has_messages, |this| {
                                this.child(
                                    h_flex()
                                        .absolute()
                                        .top_0()
                                        .right_0()
                                        .opacity(0.5)
                                        .hover(|s| s.opacity(1.0))
                                        .child(
                                            IconButton::new("toggle-height", expand_icon)
                                                .icon_size(IconSize::Small)
                                                .icon_color(Color::Muted)
                                                .tooltip({
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            expand_tooltip,
                                                            &ExpandMessageEditor,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.expand_message_editor(
                                                        &ExpandMessageEditor,
                                                        window,
                                                        cx,
                                                    );
                                                })),
                                        ),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_none()
                            .flex_wrap()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_0p5()
                                    .child(self.render_add_context_button(cx))
                                    .child(self.render_follow_toggle(cx))
                                    .children(self.render_fast_mode_control(cx))
                                    .children(self.render_thinking_control(cx)),
                            )
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .children(self.render_token_usage(cx))
                                    .children(self.profile_selector.clone())
                                    .map(|this| match self.config_options_view.clone() {
                                        Some(config_view) => this.child(config_view),
                                        None => this
                                            .children(self.mode_selector.clone())
                                            .children(self.model_selector.clone()),
                                    })
                                    .children(draft_agent_selector)
                                    .child(self.render_send_button(cx)),
                            ),
                    ),
            )
            .into_any()
    }
}

impl ThreadView {
    fn render_entries(&mut self, cx: &mut Context<Self>) -> List {
        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        let centered_container = move |content: AnyElement| {
            h_flex().w_full().justify_center().child(
                div()
                    .when_some(max_content_width, |this, max_w| this.max_w(max_w))
                    .w_full()
                    .child(content),
            )
        };

        list(
            self.list_state.clone(),
            cx.processor(move |this, index: usize, window, cx| {
                let entries = this.thread.read(cx).entries();
                if let Some(entry) = entries.get(index) {
                    let rendered = this.render_entry(index, entries.len(), entry, window, cx);
                    centered_container(rendered.into_any_element()).into_any_element()
                } else if this.generating_indicator_in_list {
                    let confirmation = entries
                        .last()
                        .is_some_and(|entry| Self::is_waiting_for_confirmation(entry));
                    let rendered = this.render_generating(confirmation, cx);
                    centered_container(rendered.into_any_element()).into_any_element()
                } else {
                    Empty.into_any()
                }
            }),
        )
        .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
        .flex_grow_1()
    }

    fn render_entry(
        &self,
        entry_ix: usize,
        total_entries: usize,
        entry: &AgentThreadEntry,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_indented = entry.is_indented();
        let is_first_indented = is_indented
            && self
                .thread
                .read(cx)
                .entries()
                .get(entry_ix.saturating_sub(1))
                .is_none_or(|entry| !entry.is_indented());

        let primary = match &entry {
            AgentThreadEntry::UserMessage(message) => {
                let Some(editor) = self
                    .entry_view_state
                    .read(cx)
                    .entry(entry_ix)
                    .and_then(|entry| entry.message_editor())
                    .cloned()
                else {
                    return Empty.into_any_element();
                };

                let editing = self.editing_message == Some(entry_ix);
                let editor_focus = editor.focus_handle(cx).is_focused(window);
                let focus_border = cx.theme().colors().border_focused;
                // Drop shadows render as a dark halo on transparent windows.
                let opaque_window = cx.theme().window_background_appearance()
                    == gpui::WindowBackgroundAppearance::Opaque;

                let has_checkpoint_button = message
                    .checkpoint
                    .as_ref()
                    .is_some_and(|checkpoint| checkpoint.show);

                let is_subagent = self.is_subagent();
                let can_rewind = self.thread.read(cx).supports_truncate(cx);
                let is_editable = can_rewind && message.client_id.is_some() && !is_subagent;
                let agent_name = if is_subagent {
                    "subagents".into()
                } else {
                    self.agent_id.clone()
                };

                v_flex()
                    .id(("user_message", entry_ix))
                    .map(|this| {
                        if is_first_indented {
                            this.pt_0p5()
                        } else {
                            this.pt_2()
                        }
                    })
                    .pb_3()
                    .px_2()
                    .gap_1p5()
                    .w_full()
                    .when(is_editable && has_checkpoint_button, |this| {
                        this.children(message.client_id.clone().map(|client_id| {
                            h_flex()
                                .px_3()
                                .gap_2()
                                .child(Divider::horizontal())
                                .child(
                                    Button::new("restore-checkpoint", "Restore Checkpoint")
                                        .start_icon(Icon::new(IconName::Undo).size(IconSize::XSmall).color(Color::Muted))
                                        .label_size(LabelSize::XSmall)
                                        .color(Color::Muted)
                                        .tooltip(Tooltip::text("Restores all files in the project to the content they had at this point in the conversation."))
                                        .on_click(cx.listener(move |this, _, _window, cx| {
                                            this.restore_checkpoint(&client_id, cx);
                                        }))
                                )
                                .child(Divider::horizontal())
                        }))
                    })
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .py_3()
                                    .px_2()
                                    .rounded_md()
                                    .bg(cx.theme().colors().editor_background)
                                    .border_1()
                                    .when(is_indented, |this| {
                                        this.py_2().px_2().when(opaque_window, |this| {
                                            this.shadow_sm()
                                        })
                                    })
                                    .border_color(cx.theme().colors().border)
                                    .map(|this| {
                                        if !is_editable {
                                            if is_subagent {
                                                return this.border_dashed();
                                            }
                                            return this;
                                        }
                                        if editing && editor_focus {
                                            return this.border_color(focus_border);
                                        }
                                        if editing && !editor_focus {
                                            return this.border_dashed()
                                        }
                                        this.when(opaque_window, |this| this.shadow_md())
                                            .hover(|s| {
                                                s.border_color(focus_border.opacity(0.8))
                                            })
                                    })
                                    .text_xs()
                                    .child(editor.clone().into_any_element())
                            )
                            .when(editor_focus, |this| {
                                let base_container = h_flex()
                                    .absolute()
                                    .top_neg_3p5()
                                    .right_3()
                                    .gap_1()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(cx.theme().colors().border)
                                    .bg(cx.theme().colors().editor_background)
                                    .overflow_hidden();

                                let is_loading_contents = self.is_loading_contents;
                                if is_editable {
                                    this.child(
                                        base_container
                                            .child(
                                                IconButton::new("cancel", IconName::Close)
                                                    .disabled(is_loading_contents)
                                                    .icon_color(Color::Error)
                                                    .icon_size(IconSize::XSmall)
                                                    .on_click(cx.listener(Self::cancel_editing))
                                            )
                                            .child(
                                                if is_loading_contents {
                                                    div()
                                                        .id("loading-edited-message-content")
                                                        .tooltip(Tooltip::text("Loading Added Context…"))
                                                        .child(loading_contents_spinner(IconSize::XSmall))
                                                        .into_any_element()
                                                } else {
                                                    IconButton::new("regenerate", IconName::Return)
                                                        .icon_color(Color::Muted)
                                                        .icon_size(IconSize::XSmall)
                                                        .tooltip(Tooltip::text(
                                                            "Editing will restart the thread from this point."
                                                        ))
                                                        .on_click(cx.listener({
                                                            let editor = editor.clone();
                                                            move |this, _, window, cx| {
                                                                this.regenerate(
                                                                    entry_ix, editor.clone(), window, cx,
                                                                );
                                                            }
                                                        })).into_any_element()
                                                }
                                            )
                                    )
                                } else {
                                    this.child(
                                        base_container
                                            .border_dashed()
                                            .child(IconButton::new("non_editable", IconName::PencilUnavailable)
                                                .icon_size(IconSize::Small)
                                                .icon_color(Color::Muted)
                                                .style(ButtonStyle::Transparent)
                                                .tooltip(Tooltip::element({
                                                    let agent_name = agent_name.clone();
                                                    move |_, _| {
                                                        v_flex()
                                                            .gap_1()
                                                            .child(Label::new("Unavailable Editing"))
                                                            .child(
                                                                div().max_w_64().child(
                                                                    Label::new(format!(
                                                                        "Editing previous messages is not available for {} yet.",
                                                                        agent_name
                                                                    ))
                                                                    .size(LabelSize::Small)
                                                                    .color(Color::Muted),
                                                                ),
                                                            )
                                                            .into_any_element()
                                                    }
                                                }))),
                                    )
                                }
                            }),
                    )
                    .into_any()
            }
            AgentThreadEntry::AssistantMessage(AssistantMessage {
                chunks,
                indented: _,
                is_subagent_output: _,
            }) => {
                let mut is_blank = true;
                let is_last = entry_ix + 1 == total_entries;

                let style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx);
                let message_body = v_flex()
                    .w_full()
                    .gap_3()
                    .children(chunks.iter().enumerate().filter_map(
                        |(chunk_ix, chunk)| match chunk {
                            AssistantMessageChunk::Message { block, .. } => {
                                block.markdown().and_then(|md| {
                                    let this_is_blank = md.read(cx).source().trim().is_empty();
                                    is_blank = is_blank && this_is_blank;
                                    if this_is_blank {
                                        return None;
                                    }

                                    Some(
                                        self.render_markdown(md.clone(), style.clone(), cx)
                                            .into_any_element(),
                                    )
                                })
                            }
                            AssistantMessageChunk::Thought { block, .. } => {
                                block.markdown().and_then(|md| {
                                    let this_is_blank = md.read(cx).source().trim().is_empty();
                                    is_blank = is_blank && this_is_blank;
                                    if this_is_blank {
                                        return None;
                                    }
                                    Some(
                                        self.render_thinking_block(
                                            entry_ix,
                                            chunk_ix,
                                            md.clone(),
                                            window,
                                            cx,
                                        )
                                        .into_any_element(),
                                    )
                                })
                            }
                        },
                    ))
                    .into_any();

                if is_blank {
                    Empty.into_any()
                } else {
                    v_flex()
                        .px_5()
                        .py_1p5()
                        .when(is_last, |this| this.pb_4())
                        .w_full()
                        .text_ui(cx)
                        .child(self.render_message_context_menu(entry_ix, message_body, cx))
                        .when_some(
                            self.entry_view_state
                                .read(cx)
                                .entry(entry_ix)
                                .and_then(|entry| entry.focus_handle(cx)),
                            |this, handle| this.track_focus(&handle),
                        )
                        .into_any()
                }
            }
            AgentThreadEntry::ToolCall(tool_call) => {
                // A canceled tool call that produced visible output is still worth
                // showing, but one that was canceled before producing anything just
                // renders as a useless "Canceled" card — hide those entirely.
                if matches!(tool_call.status, ToolCallStatus::Canceled) {
                    let has_visible_content =
                        tool_call.content.iter().any(|content| match content {
                            ToolCallContent::ContentBlock(block) => block.visible_content(cx),
                            ToolCallContent::Diff(_) | ToolCallContent::Terminal(_) => true,
                        });
                    if !has_visible_content {
                        return Empty.into_any();
                    }
                }

                let tool_call = self.render_any_tool_call(
                    self.thread.read(cx).session_id(),
                    entry_ix,
                    tool_call,
                    &self.focus_handle(cx),
                    ToolCallLayout::Standalone,
                    window,
                    cx,
                );

                if let Some(handle) = self
                    .entry_view_state
                    .read(cx)
                    .entry(entry_ix)
                    .and_then(|entry| entry.focus_handle(cx))
                {
                    tool_call.track_focus(&handle).into_any()
                } else {
                    tool_call.into_any()
                }
            }
            AgentThreadEntry::CompletedPlan(entries) => {
                self.render_completed_plan(entries, window, cx)
            }
            AgentThreadEntry::ContextCompaction(compaction) => {
                self.render_context_compaction(entry_ix, compaction, window, cx)
            }
        };

        let is_subagent_output = self.is_subagent()
            && matches!(entry, AgentThreadEntry::AssistantMessage(msg) if msg.is_subagent_output);

        let primary = if is_subagent_output {
            v_flex()
                .w_full()
                .child(
                    h_flex()
                        .id("subagent_output")
                        .px_5()
                        .py_1()
                        .gap_2()
                        .child(Divider::horizontal())
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Icon::new(IconName::ForwardArrowUp)
                                        .color(Color::Muted)
                                        .size(IconSize::Small),
                                )
                                .child(
                                    Label::new("Subagent Output")
                                        .size(LabelSize::Custom(self.tool_name_font_size()))
                                        .color(Color::Muted),
                                ),
                        )
                        .child(Divider::horizontal())
                        .tooltip(Tooltip::text("Everything below this line was sent as output from this subagent to the main agent.")),
                )
                .child(primary)
                .into_any_element()
        } else {
            primary
        };

        let thread = self.thread.clone();

        let primary = if is_indented {
            let line_top = if is_first_indented {
                rems_from_px(-12.0)
            } else {
                rems_from_px(0.0)
            };

            div()
                .relative()
                .w_full()
                .pl_5()
                .bg(cx.theme().colors().panel_background.opacity(0.2))
                .child(
                    div()
                        .absolute()
                        .left(rems_from_px(18.0))
                        .top(line_top)
                        .bottom_0()
                        .w_px()
                        .bg(cx.theme().colors().border.opacity(0.6)),
                )
                .child(primary)
                .into_any_element()
        } else {
            primary
        };

        let needs_confirmation = Self::is_waiting_for_confirmation(entry);

        let comments_editor = self.thread_feedback.comments_editor.clone();

        let primary = if entry_ix + 1 == total_entries {
            v_flex()
                .w_full()
                .child(primary)
                .when(!needs_confirmation, |this| {
                    this.child(self.render_thread_controls(&thread, cx))
                })
                .when_some(comments_editor, |this, editor| {
                    this.child(Self::render_feedback_feedback_editor(editor, cx))
                })
                .into_any_element()
        } else {
            primary
        };

        if let Some(editing_index) = self.editing_message
            && editing_index < entry_ix
        {
            let is_subagent = self.is_subagent();

            let backdrop = div()
                .id(("backdrop", entry_ix))
                .size_full()
                .absolute()
                .inset_0()
                .bg(cx.theme().colors().panel_background)
                .opacity(0.8)
                .block_mouse_except_scroll()
                .on_click(cx.listener(Self::cancel_editing));

            div()
                .relative()
                .child(primary)
                .when(!is_subagent, |this| this.child(backdrop))
                .into_any_element()
        } else {
            primary
        }
    }

    fn render_tool_call(
        &self,
        active_session_id: &acp::SessionId,
        entry_ix: usize,
        tool_call: &ToolCall,
        focus_handle: &FocusHandle,
        layout: ToolCallLayout,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let has_location = tool_call.locations.len() == 1;
        let card_header_id = SharedString::from(format!("inner-tool-call-header-{entry_ix}"));

        let failed_or_canceled = match &tool_call.status {
            ToolCallStatus::Rejected | ToolCallStatus::Canceled | ToolCallStatus::Failed => true,
            _ => false,
        };

        let needs_confirmation = matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation { .. }
        );
        let is_terminal_tool = matches!(tool_call.kind, acp::ToolKind::Execute);

        let is_edit =
            matches!(tool_call.kind, acp::ToolKind::Edit) || tool_call.diffs().next().is_some();

        let is_cancelled_edit = is_edit && matches!(tool_call.status, ToolCallStatus::Canceled);
        let (has_revealed_diff, tool_call_output_focus, tool_call_output_focus_handle) = tool_call
            .diffs()
            .next()
            .and_then(|diff| {
                let editor = self
                    .entry_view_state
                    .read(cx)
                    .entry(entry_ix)
                    .and_then(|entry| entry.editor_for_diff(diff))?;
                let has_revealed_diff = diff.read(cx).has_revealed_range(cx);
                let has_focus = editor.read(cx).is_focused(window);
                let focus_handle = editor.focus_handle(cx);
                Some((has_revealed_diff, has_focus, focus_handle))
            })
            .unwrap_or_else(|| (false, false, focus_handle.clone()));

        let use_card_layout = needs_confirmation || is_edit || is_terminal_tool;

        let has_image_content = tool_call.content.iter().any(|c| c.image().is_some());
        let is_collapsible = !tool_call.content.is_empty() && !needs_confirmation;
        let mut is_open = self
            .entry_view_state
            .read(cx)
            .is_tool_call_expanded(&tool_call.id);

        is_open |= needs_confirmation;

        let should_show_raw_input = !is_terminal_tool && !is_edit && !has_image_content;

        let input_output_header = |label: SharedString| {
            Label::new(label)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .buffer_font(cx)
        };

        let tool_output_display = if is_open {
            match &tool_call.status {
                ToolCallStatus::WaitingForConfirmation { options, .. } => {
                    let confirmation_content = v_flex()
                        .w_full()
                        .children(tool_call.content.iter().enumerate().map(
                            |(content_ix, content)| {
                                div()
                                    .child(self.render_tool_call_content(
                                        active_session_id,
                                        entry_ix,
                                        content,
                                        content_ix,
                                        tool_call,
                                        use_card_layout,
                                        failed_or_canceled,
                                        focus_handle,
                                        window,
                                        cx,
                                    ))
                                    .into_any_element()
                            },
                        ))
                        .when_some(
                            tool_call.sandbox_authorization_details.as_ref(),
                            |this, details| {
                                this.child(self.render_sandbox_authorization_details(
                                    entry_ix,
                                    &tool_call.id,
                                    details,
                                    cx,
                                ))
                            },
                        )
                        .when_some(
                            tool_call.sandbox_fallback_authorization_details.as_ref(),
                            |this, details| {
                                this.child(
                                    self.render_sandbox_fallback_authorization_details(details, cx),
                                )
                            },
                        )
                        .when(should_show_raw_input, |this| {
                            let is_raw_input_expanded =
                                self.expanded_tool_call_raw_inputs.contains(&tool_call.id);

                            let input_header = if is_raw_input_expanded {
                                "Raw Input:"
                            } else {
                                "View Raw Input"
                            };

                            this.child(
                                v_flex()
                                    .p_2()
                                    .gap_1()
                                    .border_t_1()
                                    .border_color(self.tool_card_border_color(cx))
                                    .child(
                                        h_flex()
                                            .id("disclosure_container")
                                            .pl_0p5()
                                            .gap_1()
                                            .justify_between()
                                            .rounded_xs()
                                            .hover(|s| s.bg(cx.theme().colors().element_hover))
                                            .child(input_output_header(input_header.into()))
                                            .child(
                                                Disclosure::new(
                                                    ("raw-input-disclosure", entry_ix),
                                                    is_raw_input_expanded,
                                                )
                                                .opened_icon(IconName::ChevronUp)
                                                .closed_icon(IconName::ChevronDown),
                                            )
                                            .on_click(cx.listener({
                                                let id = tool_call.id.clone();

                                                move |this: &mut Self, _, _, cx| {
                                                    if this
                                                        .expanded_tool_call_raw_inputs
                                                        .contains(&id)
                                                    {
                                                        this.expanded_tool_call_raw_inputs
                                                            .remove(&id);
                                                    } else {
                                                        this.expanded_tool_call_raw_inputs
                                                            .insert(id.clone());
                                                    }
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                    .when(is_raw_input_expanded, |this| {
                                        this.children(tool_call.raw_input_markdown.clone().map(
                                            |input| {
                                                self.render_markdown(
                                                    input,
                                                    MarkdownStyle::themed(
                                                        MarkdownFont::Agent,
                                                        window,
                                                        cx,
                                                    ),
                                                    cx,
                                                )
                                            },
                                        ))
                                    }),
                            )
                        });

                    v_flex()
                        .w_full()
                        .map(|this| {
                            if layout == ToolCallLayout::Floating {
                                // Cap the content (e.g. a full plan awaiting
                                // approval) so the floating row can never
                                // consume the entire panel and squeeze the
                                // conversation list to zero height, while the
                                // permission buttons below stay visible.
                                this.child(
                                    div()
                                        .id(("floating-confirmation-content", entry_ix))
                                        .max_h_40()
                                        .overflow_y_scroll()
                                        .child(confirmation_content),
                                )
                            } else {
                                this.child(confirmation_content)
                            }
                        })
                        .child(self.render_permission_buttons(
                            self.thread.read(cx).session_id().clone(),
                            self.is_first_tool_call(active_session_id, &tool_call.id, cx),
                            options,
                            entry_ix,
                            tool_call.id.clone(),
                            focus_handle,
                            cx,
                        ))
                        .into_any()
                }
                ToolCallStatus::Pending | ToolCallStatus::InProgress
                    if is_edit
                        && tool_call.content.is_empty()
                        && self.as_native_connection(cx).is_some() =>
                {
                    self.render_diff_loading(cx)
                }
                ToolCallStatus::Pending
                | ToolCallStatus::InProgress
                | ToolCallStatus::Completed
                | ToolCallStatus::Failed
                | ToolCallStatus::Canceled => v_flex()
                    .when(should_show_raw_input, |this| {
                        this.mt_1p5().w_full().child(
                            v_flex()
                                .ml(rems(0.4))
                                .px_3p5()
                                .pb_1()
                                .gap_1()
                                .border_l_1()
                                .border_color(self.tool_card_border_color(cx))
                                .child(input_output_header("Raw Input:".into()))
                                .children(tool_call.raw_input_markdown.clone().map(|input| {
                                    div().id(("tool-call-raw-input-markdown", entry_ix)).child(
                                        self.render_markdown(
                                            input,
                                            MarkdownStyle::themed(MarkdownFont::Agent, window, cx),
                                            cx,
                                        ),
                                    )
                                }))
                                .child(input_output_header("Output:".into())),
                        )
                    })
                    .children(
                        tool_call
                            .content
                            .iter()
                            .enumerate()
                            .map(|(content_ix, content)| {
                                div().id(("tool-call-output", entry_ix)).child(
                                    self.render_tool_call_content(
                                        active_session_id,
                                        entry_ix,
                                        content,
                                        content_ix,
                                        tool_call,
                                        use_card_layout,
                                        failed_or_canceled,
                                        focus_handle,
                                        window,
                                        cx,
                                    ),
                                )
                            }),
                    )
                    .when(!use_card_layout, |this| {
                        let button_id =
                            SharedString::from(format!("tool_output-collapse-{:?}", tool_call.id));
                        let tool_call_id = tool_call.id.clone();

                        this.child(
                            div()
                                .ml(rems(0.4))
                                .px_3p5()
                                .pt_2()
                                .border_l_1()
                                .border_color(self.tool_card_border_color(cx))
                                .child(
                                    IconButton::new(button_id, IconName::ChevronUp)
                                        .full_width()
                                        .style(ButtonStyle::Outlined)
                                        .icon_color(Color::Muted)
                                        .on_click(cx.listener({
                                            move |this: &mut Self,
                                                  _,
                                                  window,
                                                  cx: &mut Context<Self>| {
                                                this.entry_view_state.update(cx, |state, _cx| {
                                                    state.collapse_tool_call(&tool_call_id);
                                                });
                                                this.refresh_thread_search(window, cx);
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    })
                    .into_any(),
                ToolCallStatus::Rejected => Empty.into_any(),
            }
            .into()
        } else {
            None
        };

        v_flex()
            .map(|this| {
                if matches!(
                    layout,
                    ToolCallLayout::Embedded | ToolCallLayout::Floating
                ) {
                    this
                } else if use_card_layout {
                    this.my_1p5()
                        .rounded_md()
                        .border_1()
                        .when(failed_or_canceled, |this| this.border_dashed())
                        .border_color(self.tool_card_border_color(cx))
                        .bg(cx.theme().colors().editor_background)
                        .overflow_hidden()
                } else {
                    this.my_1()
                }
            })
            .when(layout == ToolCallLayout::Standalone, |this| {
                this.map(|this| {
                    if has_location && !use_card_layout {
                        this.ml_4()
                    } else {
                        this.ml_5()
                    }
                })
                .mr_5()
            })
            .map(|this| {
                if is_terminal_tool {
                    this.child(self.render_collapsible_command(
                        card_header_id.clone(),
                        true,
                        tool_call.label.clone(),
                        window,
                        cx,
                    ))
                } else {
                    this.child(
                        h_flex()
                            .group(&card_header_id)
                            .relative()
                            .w_full()
                            .justify_between()
                            .when(use_card_layout, |this| {
                                this.p_0p5()
                                    .rounded_t(rems_from_px(5.))
                                    .bg(self.tool_card_header_bg(cx))
                            })
                            .child(self.render_tool_call_label(
                                entry_ix,
                                tool_call,
                                is_edit,
                                is_cancelled_edit,
                                has_revealed_diff,
                                use_card_layout,
                                window,
                                cx,
                            ))
                            .child(
                                h_flex()
                                    .when(is_collapsible || failed_or_canceled, |this| {
                                        let diff_for_discard = if has_revealed_diff
                                            && is_cancelled_edit
                                        {
                                            tool_call.diffs().next().cloned()
                                        } else {
                                            None
                                        };

                                        this.child(
                                            h_flex()
                                                .pr_0p5()
                                                .gap_1()
                                                .when(is_collapsible, |this| {
                                                    this.child(
                                                        Disclosure::new(
                                                            ("expand-output", entry_ix),
                                                            is_open,
                                                        )
                                                        .opened_icon(IconName::ChevronUp)
                                                        .closed_icon(IconName::ChevronDown)
                                                        .visible_on_hover(&card_header_id)
                                                        .on_click(cx.listener({
                                                            let id = tool_call.id.clone();
                                                            move |this: &mut Self,
                                                                  _,
                                                                  window,
                                                                  cx: &mut Context<Self>| {
                                                                this.entry_view_state.update(
                                                                    cx,
                                                                    |state, _cx| {
                                                                        state
                                                                            .toggle_tool_call_expansion(
                                                                                &id,
                                                                            );
                                                                    },
                                                                );
                                                                this.refresh_thread_search(window, cx);
                                                                cx.notify();
                                                            }
                                                        })),
                                                    )
                                                })
                                                .when(failed_or_canceled, |this| {
                                                    if is_cancelled_edit && !has_revealed_diff {
                                                        this.child(
                                                            div()
                                                                .id(entry_ix)
                                                                .tooltip(Tooltip::text(
                                                                    "Interrupted Edit",
                                                                ))
                                                                .child(
                                                                    Icon::new(IconName::XCircle)
                                                                        .color(Color::Muted)
                                                                        .size(IconSize::Small),
                                                                ),
                                                        )
                                                    } else if is_cancelled_edit {
                                                        this
                                                    } else {
                                                        this.child(
                                                            Icon::new(IconName::Close)
                                                                .color(Color::Error)
                                                                .size(IconSize::Small),
                                                        )
                                                    }
                                                })
                                                .when_some(diff_for_discard, |this, diff| {
                                                    let tool_call_id = tool_call.id.clone();
                                                    let is_discarded = self
                                                        .discarded_partial_edits
                                                        .contains(&tool_call_id);

                                                    this.when(!is_discarded, |this| {
                                                        this.child(
                                                            IconButton::new(
                                                                ("discard-partial-edit", entry_ix),
                                                                IconName::Undo,
                                                            )
                                                            .icon_size(IconSize::Small)
                                                            .tooltip(move |_, cx| {
                                                                Tooltip::with_meta(
                                                                    "Discard Interrupted Edit",
                                                                    None,
                                                                    "You can discard this interrupted partial edit and restore the original file content.",
                                                                    cx,
                                                                )
                                                            })
                                                            .on_click(cx.listener({
                                                                let tool_call_id =
                                                                    tool_call_id.clone();
                                                                move |this, _, _window, cx| {
                                                                    let diff_data = diff.read(cx);
                                                                    let base_text = diff_data
                                                                        .base_text()
                                                                        .clone();
                                                                    let buffer =
                                                                        diff_data.buffer().clone();
                                                                    buffer.update(
                                                                        cx,
                                                                        |buffer, cx| {
                                                                            buffer.set_text(
                                                                                base_text.as_ref(),
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    this.discarded_partial_edits
                                                                        .insert(
                                                                            tool_call_id.clone(),
                                                                        );
                                                                    cx.notify();
                                                                }
                                                            })),
                                                        )
                                                    })
                                                }),
                                        )
                                    })
                                    .when(tool_call_output_focus, |this| {
                                        this.child(
                                            Button::new("open-file-button", "Open File")
                                                .style(ButtonStyle::Outlined)
                                                .label_size(LabelSize::Small)
                                                .key_binding(
                                                    KeyBinding::for_action_in(&OpenExcerpts, &tool_call_output_focus_handle, cx)
                                                        .map(|s| s.size(rems_from_px(12.))),
                                                )
                                                .on_click(|_, window, cx| {
                                                    window.dispatch_action(
                                                        Box::new(OpenExcerpts),
                                                        cx,
                                                    )
                                                }),
                                        )
                                    }),
                            )

                    )
                }
            })
            .children(tool_output_display)
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
