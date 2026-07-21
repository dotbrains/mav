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
#[path = "thread_view/event_handlers.rs"]
mod event_handlers;
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
#[path = "thread_view/rendering.rs"]
mod rendering;
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
#[path = "thread_view/setup.rs"]
mod setup;
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
#[path = "thread_view/turn_lifecycle.rs"]
mod turn_lifecycle;
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
