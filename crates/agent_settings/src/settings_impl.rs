use gpui::px;
use settings::Settings;
use util::ResultExt as _;

use crate::layout::parse_auto_compact_threshold;
use crate::{
    AgentProfileId, AgentSettings, AutoCompactSettings, AutoCompactThreshold, CompiledRegex,
    InvalidRegexPattern, SandboxPermissions, ToolPermissions, ToolRules,
};

impl Settings for AgentSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let agent = content.agent.clone().unwrap();
        Self {
            enabled: agent.enabled.unwrap(),
            button: agent.button.unwrap(),
            dock: agent.dock.unwrap(),
            default_width: px(agent.default_width.unwrap()),
            default_height: px(agent.default_height.unwrap()),
            max_content_width: if agent.limit_content_width.unwrap() {
                Some(px(agent.max_content_width.unwrap()))
            } else {
                None
            },
            flexible: agent.flexible.unwrap(),
            default_model: Some(agent.default_model.unwrap()),
            subagent_model: agent.subagent_model,
            inline_assistant_model: agent.inline_assistant_model,
            inline_assistant_use_streaming_tools: agent
                .inline_assistant_use_streaming_tools
                .unwrap_or(true),
            commit_message_include_project_rules: agent
                .commit_message_include_project_rules
                .unwrap(),
            commit_message_model: agent.commit_message_model,
            commit_message_instructions: agent.commit_message_instructions,
            thread_summary_model: agent.thread_summary_model,
            inline_alternatives: agent.inline_alternatives.unwrap_or_default(),
            favorite_models: agent.favorite_models,
            default_profile: AgentProfileId(agent.default_profile.unwrap()),
            profiles: agent
                .profiles
                .unwrap()
                .into_iter()
                .map(|(key, val)| (AgentProfileId(key), val.into()))
                .collect(),

            notify_when_agent_waiting: agent.notify_when_agent_waiting.unwrap(),
            play_sound_when_agent_done: agent.play_sound_when_agent_done.unwrap_or_default(),
            single_file_review: agent.single_file_review.unwrap(),
            model_parameters: agent.model_parameters,
            auto_compact: {
                let auto_compact = agent.auto_compact.unwrap();
                let threshold = parse_auto_compact_threshold(&auto_compact.threshold.unwrap().0)
                    .log_err()
                    .unwrap_or(AutoCompactThreshold::DEFAULT);
                AutoCompactSettings {
                    enabled: auto_compact.enabled.unwrap(),
                    threshold,
                }
            },
            enable_feedback: agent.enable_feedback.unwrap(),
            expand_edit_card: agent.expand_edit_card.unwrap(),
            expand_terminal_card: agent.expand_terminal_card.unwrap(),
            terminal_init_command: agent
                .terminal_init_command
                .filter(|command| !command.trim().is_empty()),
            thinking_display: agent.thinking_display.unwrap(),
            cancel_generation_on_terminal_stop: agent.cancel_generation_on_terminal_stop.unwrap(),
            use_modifier_to_send: agent.use_modifier_to_send.unwrap(),
            message_editor_min_lines: agent.message_editor_min_lines.unwrap(),
            show_turn_stats: agent.show_turn_stats.unwrap(),
            show_merge_conflict_indicator: agent.show_merge_conflict_indicator.unwrap(),
            tool_permissions: compile_tool_permissions(agent.tool_permissions),
            sandbox_permissions: compile_sandbox_permissions(agent.sandbox_permissions),
        }
    }
}

fn compile_sandbox_permissions(
    content: Option<settings::SandboxPermissionsContent>,
) -> SandboxPermissions {
    let Some(content) = content else {
        return SandboxPermissions::default();
    };

    let mut write_paths = Vec::new();
    for path in content.write_paths.map(|paths| paths.0).unwrap_or_default() {
        // Normalize away `..`/`.` before storing, since coverage checks are
        // purely lexical; drop paths that escape the filesystem root.
        if let Ok(normalized) = util::paths::normalize_lexically(&path) {
            util::paths::insert_subtree(&mut write_paths, normalized);
        }
    }

    let network_hosts = content
        .network_hosts
        .map(|hosts| hosts.0)
        .unwrap_or_default();

    SandboxPermissions {
        allow_all_hosts: content.allow_all_hosts.unwrap_or(false),
        network_hosts,
        allow_git_access: content.allow_git_access.unwrap_or(false),
        allow_fs_write_all: content.allow_fs_write_all.unwrap_or(false),
        allow_unsandboxed: content.allow_unsandboxed.unwrap_or(false),
        write_paths,
    }
}

fn compile_tool_permissions(content: Option<settings::ToolPermissionsContent>) -> ToolPermissions {
    let Some(content) = content else {
        return ToolPermissions::default();
    };

    let tools = content
        .tools
        .into_iter()
        .map(|(tool_name, rules_content)| {
            let mut invalid_patterns = Vec::new();

            let (always_allow, allow_errors) = compile_regex_rules(
                rules_content.always_allow.map(|v| v.0).unwrap_or_default(),
                "always_allow",
            );
            invalid_patterns.extend(allow_errors);

            let (always_deny, deny_errors) = compile_regex_rules(
                rules_content.always_deny.map(|v| v.0).unwrap_or_default(),
                "always_deny",
            );
            invalid_patterns.extend(deny_errors);

            let (always_confirm, confirm_errors) = compile_regex_rules(
                rules_content
                    .always_confirm
                    .map(|v| v.0)
                    .unwrap_or_default(),
                "always_confirm",
            );
            invalid_patterns.extend(confirm_errors);

            // Log invalid patterns for debugging. Users will see an error when they
            // attempt to use a tool with invalid patterns in their settings.
            for invalid in &invalid_patterns {
                log::error!(
                    "Invalid regex pattern in tool_permissions for '{}' tool ({}): '{}' - {}",
                    tool_name,
                    invalid.rule_type,
                    invalid.pattern,
                    invalid.error,
                );
            }

            let rules = ToolRules {
                // Preserve tool-specific default; None means fall back to global default at decision time
                default: rules_content.default,
                always_allow,
                always_deny,
                always_confirm,
                invalid_patterns,
            };
            (tool_name, rules)
        })
        .collect();

    ToolPermissions {
        default: content.default.unwrap_or_default(),
        tools,
    }
}

fn compile_regex_rules(
    rules: Vec<settings::ToolRegexRule>,
    rule_type: &str,
) -> (Vec<CompiledRegex>, Vec<InvalidRegexPattern>) {
    let mut compiled = Vec::new();
    let mut errors = Vec::new();

    for rule in rules {
        if rule.pattern.is_empty() {
            errors.push(InvalidRegexPattern {
                pattern: rule.pattern,
                rule_type: rule_type.to_string(),
                error: "empty regex patterns are not allowed".to_string(),
            });
            continue;
        }
        let case_sensitive = rule.case_sensitive.unwrap_or(false);
        match CompiledRegex::try_new(&rule.pattern, case_sensitive) {
            Ok(regex) => compiled.push(regex),
            Err(error) => {
                errors.push(InvalidRegexPattern {
                    pattern: rule.pattern,
                    rule_type: rule_type.to_string(),
                    error: error.to_string(),
                });
            }
        }
    }

    (compiled, errors)
}
