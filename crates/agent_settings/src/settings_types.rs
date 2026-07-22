use std::path::PathBuf;
use std::sync::Arc;

use collections::{HashSet, IndexMap};
use gpui::{App, Pixels, SharedString};
use language_model::LanguageModel;
use project::DisableAiSettings;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::{
    DockPosition, LanguageModelParameters, LanguageModelSelection, NotifyWhenAgentWaiting,
    PlaySoundWhenAgentDone, RegisterSetting, Settings, ThinkingBlockDisplay,
};

use crate::{AgentProfileSettings, AutoCompactSettings, ToolPermissions};

#[derive(Clone, Debug, RegisterSetting)]
pub struct AgentSettings {
    pub enabled: bool,
    pub button: bool,
    pub dock: DockPosition,
    pub flexible: bool,
    pub default_width: Pixels,
    pub default_height: Pixels,
    pub max_content_width: Option<Pixels>,
    pub default_model: Option<LanguageModelSelection>,
    pub subagent_model: Option<LanguageModelSelection>,
    pub inline_assistant_model: Option<LanguageModelSelection>,
    pub inline_assistant_use_streaming_tools: bool,
    pub commit_message_model: Option<LanguageModelSelection>,
    pub commit_message_include_project_rules: bool,
    pub commit_message_instructions: Option<String>,
    pub thread_summary_model: Option<LanguageModelSelection>,
    pub inline_alternatives: Vec<LanguageModelSelection>,
    pub favorite_models: Vec<LanguageModelSelection>,
    pub default_profile: AgentProfileId,
    pub profiles: IndexMap<AgentProfileId, AgentProfileSettings>,

    pub notify_when_agent_waiting: NotifyWhenAgentWaiting,
    pub play_sound_when_agent_done: PlaySoundWhenAgentDone,
    pub single_file_review: bool,
    pub model_parameters: Vec<LanguageModelParameters>,
    pub auto_compact: AutoCompactSettings,
    pub enable_feedback: bool,
    pub expand_edit_card: bool,
    pub expand_terminal_card: bool,
    pub terminal_init_command: Option<String>,
    pub thinking_display: ThinkingBlockDisplay,
    pub cancel_generation_on_terminal_stop: bool,
    pub use_modifier_to_send: bool,
    pub message_editor_min_lines: usize,
    pub show_turn_stats: bool,
    pub show_merge_conflict_indicator: bool,
    pub tool_permissions: ToolPermissions,
    pub sandbox_permissions: SandboxPermissions,
}

impl AgentSettings {
    pub fn enabled(&self, cx: &App) -> bool {
        self.enabled && !DisableAiSettings::get_global(cx).disable_ai
    }

    pub fn temperature_for_model(model: &Arc<dyn LanguageModel>, cx: &App) -> Option<f32> {
        let settings = Self::get_global(cx);
        for setting in settings.model_parameters.iter().rev() {
            if let Some(provider) = &setting.provider
                && provider.0 != model.provider_id().0
            {
                continue;
            }
            if let Some(setting_model) = &setting.model
                && *setting_model != model.id().0
            {
                continue;
            }
            return setting.temperature;
        }
        return None;
    }

    pub fn set_message_editor_max_lines(&self) -> usize {
        self.message_editor_min_lines * 2
    }

    pub fn favorite_model_ids(&self) -> HashSet<SharedString> {
        self.favorite_models
            .iter()
            .map(|sel| SharedString::from(format!("{}/{}", sel.provider.0, sel.model)))
            .collect()
    }
}

pub fn language_model_to_selection(
    model: &Arc<dyn LanguageModel>,
    override_selection: Option<&LanguageModelSelection>,
) -> LanguageModelSelection {
    let provider = model.provider_id().0.to_string().into();
    let model_name = model.id().0.to_string();
    match override_selection {
        Some(current) => LanguageModelSelection {
            provider,
            model: model_name,
            enable_thinking: current.enable_thinking && model.supports_thinking(),
            effort: current
                .effort
                .clone()
                .filter(|value| {
                    model
                        .supported_effort_levels()
                        .iter()
                        .any(|level| level.value.as_ref() == value.as_str())
                })
                .or_else(|| {
                    model
                        .default_effort_level()
                        .map(|effort| effort.value.to_string())
                }),
            speed: current.speed.filter(|_| model.supports_fast_mode()),
        },
        None => LanguageModelSelection {
            provider,
            model: model_name,
            enable_thinking: model.supports_thinking(),
            effort: model
                .default_effort_level()
                .map(|effort| effort.value.to_string()),
            speed: None,
        },
    }
}
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentProfileId(pub Arc<str>);

impl AgentProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for AgentProfileId {
    fn default() -> Self {
        Self("write".into())
    }
}

/// Persistent "allow always" sandbox grants for agent-run terminal commands.
///
/// Coverage decisions for these grants are made in
/// `agent::sandboxing::ThreadSandboxGrants::covers_with_persistent`, which
/// combines them with the in-memory per-thread grants. `write_paths` are
/// stored as minimal, lexically-normalized subtrees (see
/// [`compile_sandbox_permissions`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SandboxPermissions {
    /// Allow sandboxed commands to reach any host over the network.
    pub allow_all_hosts: bool,
    /// Hosts sandboxed commands may always reach, in canonical form (exact
    /// hostnames or leading-`*.` subdomain wildcards). Parsed/validated where
    /// consumed (`agent::sandboxing`).
    pub network_hosts: Vec<String>,
    /// Allow sandboxed commands to access protected Git metadata paths.
    pub allow_git_access: bool,
    pub allow_fs_write_all: bool,
    /// Persistently run agent terminal commands outside the OS sandbox. This is
    /// the model-facing "off switch": when set, the sandboxed terminal tool is
    /// not exposed and the system prompt omits the sandbox section, so the
    /// model uses the plain `terminal` tool (on Windows, WSL sandbox setup is
    /// skipped). Distinct from the model-requested `unsandboxed: true` escape
    /// approved "once" or "for this thread", which keeps the sandboxed
    /// tool/prompt in place — see `agent::sandboxing`.
    pub allow_unsandboxed: bool,
    pub write_paths: Vec<PathBuf>,
}
