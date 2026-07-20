use std::{
    cell::Cell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use acp_thread::{AcpThread, AcpThreadEvent, MentionUri, ThreadStatus};
use agent::{ContextServerRegistry, SharedThread, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
use agent_servers::AgentServer;
use agent_settings::UserAgentsMd;
use db::kvp::{Dismissable, KeyValueStore};
use itertools::Itertools;
use project::AgentId;
use settings::{LanguageModelProviderSetting, LanguageModelSelection};

use mav_actions::{
    DecreaseBufferFontSize, IncreaseBufferFontSize, ResetBufferFontSize,
    agent::{
        AddSelectionToThread, LogoutAgent, OpenSettings, ReauthenticateAgent, ResetAgentZoom,
        ResetOnboarding, ResolveConflictedFilesWithAgent, ResolveConflictsWithAgent,
        ReviewBranchDiff,
    },
    assistant::{
        FocusAgent, ManageSkills, OpenGlobalAgentsMdRules, OpenProjectAgentsMdRules, Toggle,
        ToggleFocus,
    },
};

use crate::ExpandMessageEditor;
use crate::ManageProfiles;
use crate::agent_connection_store::AgentConnectionStore;
use crate::completion_provider::AgentContextSource;
use crate::terminal_thread_metadata_store::{
    TerminalThreadMetadata, TerminalThreadMetadataStore, terminal_title_without_prefix,
};
use crate::thread_metadata_store::{ThreadId, ThreadMetadataStore, ThreadMetadataStoreEvent};
use crate::{
    AddContextServer, AgentDiffPane, ConversationView, CopyThreadToClipboard, Follow,
    LoadThreadFromClipboard, NewTerminalThread, NewThread, OpenActiveThreadAsMarkdown,
    OpenAgentDiff, ResetFastModeWarnings, ResetTrialEndUpsell, ResetTrialUpsell,
    ShowAllSidebarThreadMetadata, ShowThreadMetadata, ToggleNewThreadMenu, ToggleOptionsMenu,
    agent_configuration::{AgentConfiguration, AssistantConfigurationEvent},
    conversation_view::{
        AcpThreadViewEvent, RootThreadUpdated, ThreadView, reset_fast_mode_warnings,
    },
    ui::{AgentNotification, AgentNotificationEvent, EndTrialUpsell},
};
use crate::{
    Agent, AgentInitialContent, AgentThreadSource, ExternalSourcePrompt, NewExternalAgentThread,
    NewNativeAgentThreadFromSummary,
};
#[path = "agent_panel_actions.rs"]
mod agent_panel_actions;
#[path = "agent_panel_commands.rs"]
mod agent_panel_commands;
#[path = "agent_panel_configuration.rs"]
mod agent_panel_configuration;
#[path = "agent_panel_constructor.rs"]
mod agent_panel_constructor;
#[path = "agent_panel_diagnostics.rs"]
mod agent_panel_diagnostics;
#[path = "agent_panel_draft.rs"]
mod agent_panel_draft;
#[path = "agent_panel_environment.rs"]
mod agent_panel_environment;
#[path = "agent_panel_header.rs"]
mod agent_panel_header;
#[path = "agent_panel_init.rs"]
mod agent_panel_init;
#[path = "agent_panel_load.rs"]
mod agent_panel_load;
#[path = "agent_panel_model_override.rs"]
mod agent_panel_model_override;
#[path = "agent_panel_navigation.rs"]
mod agent_panel_navigation;
#[path = "agent_panel_panel.rs"]
mod agent_panel_panel;
#[path = "agent_panel_persistence.rs"]
mod agent_panel_persistence;
#[path = "agent_panel_prompts.rs"]
mod agent_panel_prompts;
#[path = "agent_panel_render.rs"]
mod agent_panel_render;
#[path = "agent_panel_rules.rs"]
mod agent_panel_rules;
#[path = "agent_panel_sibling_host.rs"]
mod agent_panel_sibling_host;
#[path = "agent_panel_source_init.rs"]
mod agent_panel_source_init;
#[path = "agent_panel_surface.rs"]
mod agent_panel_surface;
#[path = "agent_panel_terminal.rs"]
mod agent_panel_terminal;
#[path = "agent_panel_terminal_lifecycle.rs"]
mod agent_panel_terminal_lifecycle;
#[path = "agent_panel_terminal_metadata.rs"]
mod agent_panel_terminal_metadata;
#[path = "agent_panel_terminal_notifications.rs"]
mod agent_panel_terminal_notifications;
#[path = "agent_panel_terminal_title.rs"]
mod agent_panel_terminal_title;
#[cfg(any(test, feature = "test-support"))]
#[path = "agent_panel_test_support.rs"]
mod agent_panel_test_support;
#[path = "agent_panel_thread_access.rs"]
mod agent_panel_thread_access;
#[path = "agent_panel_thread_creation.rs"]
mod agent_panel_thread_creation;
#[path = "agent_panel_thread_entries.rs"]
mod agent_panel_thread_entries;
#[path = "agent_panel_thread_entry.rs"]
mod agent_panel_thread_entry;
#[path = "agent_panel_thread_io.rs"]
mod agent_panel_thread_io;
#[path = "agent_panel_thread_launch.rs"]
mod agent_panel_thread_launch;
#[path = "agent_panel_thread_status.rs"]
mod agent_panel_thread_status;
#[path = "agent_panel_thread_types.rs"]
mod agent_panel_thread_types;
#[path = "agent_panel_toolbar.rs"]
mod agent_panel_toolbar;
#[path = "agent_panel_view_lifecycle.rs"]
mod agent_panel_view_lifecycle;
#[path = "agent_panel_view_state.rs"]
mod agent_panel_view_state;
use agent_panel_diagnostics::thread_metadata_to_debug_json;
pub use agent_panel_init::init;
pub(crate) use agent_panel_model_override::apply_native_model_override;
pub use agent_panel_panel::AgentPanelEvent;
use agent_panel_persistence::{
    AgentPanelEntryKind, SerializedActiveThread, SerializedAgentPanel,
    read_global_last_created_entry_kind, read_global_last_used_agent, read_legacy_serialized_panel,
    read_serialized_panel, save_serialized_panel, write_global_last_created_entry_kind,
    write_global_last_used_agent,
};
#[cfg(test)]
use agent_panel_prompts::conflict_resource_block;
use agent_panel_prompts::{
    build_conflict_resolution_prompt, build_conflicted_files_resolution_prompt,
    format_selection_for_terminal,
};
use agent_panel_render::{OnboardingUpsell, TrialEndUpsell};
use agent_panel_rules::{open_global_rules, open_project_rules, project_agents_md_path};
use agent_panel_sibling_host::AgentPanelSiblingHost;
pub use agent_panel_terminal::{AgentPanelTerminalInfo, MaxIdleRetainedThreads, TerminalId};
use agent_panel_terminal::{AgentTerminal, TERMINAL_AGENT_TELEMETRY_ID};
pub use agent_panel_thread_types::{CreateThreadOptions, ThreadTitleRegenerationResult};
use agent_panel_view_state::{AgentThread, BaseView, OverlayView, VisibleSurface, WhichFontSize};
use agent_settings::AgentSettings;
use ai_onboarding::AgentPanelOnboarding;
use anyhow::Result;
#[cfg(feature = "audio")]
use audio::{Audio, Sound};
use chrono::{DateTime, Utc};
use client::UserStore;
use cloud_api_types::Plan;
use collections::HashMap;
use editor::{Editor, MultiBuffer};
use extension_host::ExtensionStore;
use feature_flags::{
    AgentSettingsUiFeatureFlag, CreateThreadToolFeatureFlag, FeatureFlagAppExt as _,
};

use fs::Fs;
use futures::FutureExt as _;
use gpui::{
    Action, Anchor, Animation, AnimationExt, AnyElement, App, AsyncWindowContext, ClipboardItem,
    Entity, ExternalPaths, FocusHandle, Focusable, KeyContext, Pixels, PlatformDisplay,
    Subscription, Task, TaskExt, WeakEntity, prelude::*, pulsating_between,
};
use language::LanguageRegistry;
use language_model::LanguageModelRegistry;
use notifications::status_toast::StatusToast;
use project::{Project, ProjectPath, Worktree};
use settings::{NotifyWhenAgentWaiting, Settings, update_settings_file};

use terminal::Event as TerminalEvent;
use terminal_view::TerminalView;
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, ContextMenuEntry, GradientFade, IconButton, KeyBinding, PopoverMenu,
    PopoverMenuHandle, ProjectEmptyState, Tab, Tooltip, prelude::*, utils::WithRemSize,
};
use util::ResultExt as _;
use workspace::{
    CollaboratorId, DraggedSelection, DraggedTab, MultiWorkspace, PaneKind, PathList,
    ToggleSidebar, ToggleZoom, Workspace, WorkspaceId,
    dock::{Panel, PanelEvent},
    item::ItemEvent,
};

#[cfg(test)]
use collections::HashSet;
#[cfg(test)]
use mav_actions::agent::ConflictContent;

const MIN_PANEL_WIDTH: Pixels = px(300.);
const TERMINAL_INIT_COMMAND_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AgentPanel {
    workspace: WeakEntity<Workspace>,
    /// Workspace id is used as a database key
    workspace_id: Option<WorkspaceId>,
    user_store: Entity<UserStore>,
    project: Entity<Project>,
    fs: Arc<dyn Fs>,
    language_registry: Arc<LanguageRegistry>,
    thread_store: Entity<ThreadStore>,
    connection_store: Entity<AgentConnectionStore>,
    context_server_registry: Entity<ContextServerRegistry>,
    configuration: Option<Entity<AgentConfiguration>>,
    configuration_subscription: Option<Subscription>,
    focus_handle: FocusHandle,
    base_view: BaseView,
    last_created_entry_kind: AgentPanelEntryKind,
    overlay_view: Option<OverlayView>,
    draft_thread: Option<Entity<ConversationView>>,
    retained_threads: HashMap<ThreadId, Entity<ConversationView>>,
    terminals: HashMap<TerminalId, AgentTerminal>,
    pending_terminal_spawn: Option<TerminalId>,
    new_thread_menu_handle: PopoverMenuHandle<ContextMenu>,
    agent_panel_menu_handle: PopoverMenuHandle<ContextMenu>,
    _extension_subscription: Option<Subscription>,
    _project_subscription: Subscription,
    zoomed: bool,
    pending_serialization: Option<Task<Result<()>>>,
    new_user_onboarding: Entity<AgentPanelOnboarding>,
    new_user_onboarding_upsell_dismissed: AtomicBool,
    selected_agent: Agent,
    _thread_view_subscription: Option<Subscription>,
    _active_thread_focus_subscription: Option<Subscription>,
    _base_view_observation: Option<Subscription>,
    _draft_editor_observation: Option<Subscription>,
    _active_draft_reclaim_observation: Option<Subscription>,
    _thread_metadata_store_subscription: Subscription,
    last_context_source: Option<AgentContextSource>,

    is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewWorktreeBranchTarget;
    use crate::conversation_view::tests::{StubAgentServer, init_test};
    use crate::test_support::{
        active_session_id, active_thread_id, open_thread_with_connection,
        open_thread_with_custom_connection, register_test_sidebar, send_message,
    };
    use acp_thread::{AgentConnection, StubAgentConnection, ThreadStatus};
    use action_log::ActionLog;
    use anyhow::{Result, anyhow};
    use feature_flags::FeatureFlagAppExt;
    use fs::FakeFs;
    use gpui::{App, TestAppContext, UpdateGlobal, VisualTestContext};
    use parking_lot::Mutex;
    use project::{Project, WorktreePaths};
    use settings::{SettingsStore, WorkingDirectory};
    use std::any::Any;

    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Instant;

    #[path = "agent_panel_conflict_prompt_tests.rs"]
    mod conflict_prompt_tests;
    #[path = "agent_panel_serialization_tests.rs"]
    mod serialization_tests;
    #[path = "agent_panel_session_tracking_connection.rs"]
    mod session_tracking_connection;
    use session_tracking_connection::SessionTrackingConnection;
    #[path = "agent_panel_agent_selection_tests.rs"]
    mod agent_selection_tests;
    #[path = "agent_panel_disassociation_tests.rs"]
    mod disassociation_tests;
    #[path = "agent_panel_draft_lifecycle_tests.rs"]
    mod draft_lifecycle_tests;
    #[path = "agent_panel_draft_promotion_tests.rs"]
    mod draft_promotion_tests;
    #[path = "agent_panel_draft_prompt_tests.rs"]
    mod draft_prompt_tests;
    #[path = "agent_panel_draft_reload_tests.rs"]
    mod draft_reload_tests;
    #[path = "agent_panel_draft_switching_tests.rs"]
    mod draft_switching_tests;
    #[path = "agent_panel_entry_action_tests.rs"]
    mod entry_action_tests;
    #[path = "agent_panel_entry_command_tests.rs"]
    mod entry_command_tests;
    #[path = "agent_panel_external_drop_tests.rs"]
    mod external_drop_tests;
    #[path = "agent_panel_misc_regression_tests.rs"]
    mod misc_regression_tests;
    #[path = "agent_panel_source_initialization_tests.rs"]
    mod source_initialization_tests;
    #[path = "agent_panel_source_overwrite_tests.rs"]
    mod source_overwrite_tests;
    #[path = "agent_panel_terminal_init_tests.rs"]
    mod terminal_init_tests;
    #[path = "agent_panel_terminal_notification_lifecycle_tests.rs"]
    mod terminal_notification_lifecycle_tests;
    #[path = "agent_panel_terminal_notification_overlay_tests.rs"]
    mod terminal_notification_overlay_tests;
    #[path = "agent_panel_terminal_notification_sidebar_tests.rs"]
    mod terminal_notification_sidebar_tests;
    #[path = "agent_panel_terminal_restore_tests.rs"]
    mod terminal_restore_tests;
    #[path = "agent_panel_terminal_title_display_tests.rs"]
    mod terminal_title_display_tests;
    #[path = "agent_panel_terminal_title_editor_tests.rs"]
    mod terminal_title_editor_tests;
    #[path = "agent_panel_test_helpers.rs"]
    mod test_helpers;
    #[path = "agent_panel_thread_cleanup_tests.rs"]
    mod thread_cleanup_tests;
    #[path = "agent_panel_thread_navigation_tests.rs"]
    mod thread_navigation_tests;
    #[path = "agent_panel_thread_options_tests.rs"]
    mod thread_options_tests;
    #[path = "agent_panel_thread_restore_tests.rs"]
    mod thread_restore_tests;
    #[path = "agent_panel_thread_workdir_tests.rs"]
    mod thread_workdir_tests;
    #[path = "agent_panel_worktree_rollback_tests.rs"]
    mod worktree_rollback_tests;
    use test_helpers::*;
}
