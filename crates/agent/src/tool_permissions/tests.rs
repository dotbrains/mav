use super::*;
use crate::pattern_extraction::extract_terminal_pattern;
use crate::tools::{DeletePathTool, FetchTool, TerminalTool};
use crate::{AgentTool, EditFileTool};
use agent_settings::{AgentProfileId, CompiledRegex, InvalidRegexPattern, ToolRules};
use gpui::px;
use settings::{DockPosition, NotifyWhenAgentWaiting, PlaySoundWhenAgentDone};
use std::sync::Arc;

mod basic_rules;
mod file_tools;
mod hardcoded_rules;
mod paths;
mod shell_rules;

use super::*;
use crate::pattern_extraction::extract_terminal_pattern;
use crate::tools::{DeletePathTool, FetchTool, TerminalTool};
use crate::{AgentTool, EditFileTool};
use agent_settings::{AgentProfileId, CompiledRegex, InvalidRegexPattern, ToolRules};
use gpui::px;
use settings::{DockPosition, NotifyWhenAgentWaiting, PlaySoundWhenAgentDone};
use std::sync::Arc;

fn test_agent_settings(tool_permissions: ToolPermissions) -> AgentSettings {
    AgentSettings {
        enabled: true,
        button: true,
        dock: DockPosition::Right,
        flexible: true,
        default_width: px(300.),
        default_height: px(600.),
        max_content_width: Some(px(850.)),
        default_model: None,
        subagent_model: None,
        inline_assistant_model: None,
        inline_assistant_use_streaming_tools: false,
        commit_message_model: None,
        commit_message_include_project_rules: true,
        commit_message_instructions: None,
        thread_summary_model: None,
        inline_alternatives: vec![],
        favorite_models: vec![],
        default_profile: AgentProfileId::default(),
        profiles: Default::default(),
        notify_when_agent_waiting: NotifyWhenAgentWaiting::default(),
        play_sound_when_agent_done: PlaySoundWhenAgentDone::default(),
        single_file_review: false,
        model_parameters: vec![],
        auto_compact: agent_settings::AutoCompactSettings {
            enabled: false,
            threshold: agent_settings::AutoCompactThreshold::DEFAULT,
        },
        enable_feedback: false,
        expand_edit_card: true,
        expand_terminal_card: true,
        terminal_init_command: None,
        cancel_generation_on_terminal_stop: true,
        use_modifier_to_send: true,
        message_editor_min_lines: 1,
        tool_permissions,
        sandbox_permissions: Default::default(),
        show_turn_stats: false,
        show_merge_conflict_indicator: true,
        thinking_display: Default::default(),
    }
}

fn pattern(command: &str) -> &'static str {
    Box::leak(
        extract_terminal_pattern(command)
            .expect("failed to extract pattern")
            .into_boxed_str(),
    )
}

struct PermTest {
    tool: &'static str,
    input: &'static str,
    mode: Option<ToolPermissionMode>,
    allow: Vec<(&'static str, bool)>,
    deny: Vec<(&'static str, bool)>,
    confirm: Vec<(&'static str, bool)>,
    global_default: ToolPermissionMode,
    shell: ShellKind,
}

impl PermTest {
    fn new(input: &'static str) -> Self {
        Self {
            tool: TerminalTool::NAME,
            input,
            mode: None,
            allow: vec![],
            deny: vec![],
            confirm: vec![],
            global_default: ToolPermissionMode::Confirm,
            shell: ShellKind::Posix,
        }
    }

    fn tool(mut self, t: &'static str) -> Self {
        self.tool = t;
        self
    }
    fn mode(mut self, m: ToolPermissionMode) -> Self {
        self.mode = Some(m);
        self
    }
    fn allow(mut self, p: &[&'static str]) -> Self {
        self.allow = p.iter().map(|s| (*s, false)).collect();
        self
    }
    fn allow_case_sensitive(mut self, p: &[&'static str]) -> Self {
        self.allow = p.iter().map(|s| (*s, true)).collect();
        self
    }
    fn deny(mut self, p: &[&'static str]) -> Self {
        self.deny = p.iter().map(|s| (*s, false)).collect();
        self
    }
    fn deny_case_sensitive(mut self, p: &[&'static str]) -> Self {
        self.deny = p.iter().map(|s| (*s, true)).collect();
        self
    }
    fn confirm(mut self, p: &[&'static str]) -> Self {
        self.confirm = p.iter().map(|s| (*s, false)).collect();
        self
    }
    fn global_default(mut self, m: ToolPermissionMode) -> Self {
        self.global_default = m;
        self
    }
    fn shell(mut self, s: ShellKind) -> Self {
        self.shell = s;
        self
    }

    fn is_allow(self) {
        assert_eq!(
            self.run(),
            ToolPermissionDecision::Allow,
            "expected Allow for '{}'",
            self.input
        );
    }
    fn is_deny(self) {
        assert!(
            matches!(self.run(), ToolPermissionDecision::Deny(_)),
            "expected Deny for '{}'",
            self.input
        );
    }
    fn is_confirm(self) {
        assert_eq!(
            self.run(),
            ToolPermissionDecision::Confirm,
            "expected Confirm for '{}'",
            self.input
        );
    }

    fn run(&self) -> ToolPermissionDecision {
        let mut tools = collections::HashMap::default();
        tools.insert(
            Arc::from(self.tool),
            ToolRules {
                default: self.mode,
                always_allow: self
                    .allow
                    .iter()
                    .map(|(p, cs)| {
                        CompiledRegex::new(p, *cs)
                            .unwrap_or_else(|| panic!("invalid regex in test: {p:?}"))
                    })
                    .collect(),
                always_deny: self
                    .deny
                    .iter()
                    .map(|(p, cs)| {
                        CompiledRegex::new(p, *cs)
                            .unwrap_or_else(|| panic!("invalid regex in test: {p:?}"))
                    })
                    .collect(),
                always_confirm: self
                    .confirm
                    .iter()
                    .map(|(p, cs)| {
                        CompiledRegex::new(p, *cs)
                            .unwrap_or_else(|| panic!("invalid regex in test: {p:?}"))
                    })
                    .collect(),
                invalid_patterns: vec![],
            },
        );
        ToolPermissionDecision::from_input(
            self.tool,
            &[self.input.to_string()],
            &ToolPermissions {
                default: self.global_default,
                tools,
            },
            self.shell,
        )
    }
}

fn t(input: &'static str) -> PermTest {
    PermTest::new(input)
}

fn no_rules(input: &str, global_default: ToolPermissionMode) -> ToolPermissionDecision {
    ToolPermissionDecision::from_input(
        TerminalTool::NAME,
        &[input.to_string()],
        &ToolPermissions {
            default: global_default,
            tools: collections::HashMap::default(),
        },
        ShellKind::Posix,
    )
}
