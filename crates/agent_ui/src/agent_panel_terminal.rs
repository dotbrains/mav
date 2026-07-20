use std::{fmt, path::PathBuf};

use chrono::{DateTime, Utc};
use gpui::{App, SharedString};

pub(crate) const TERMINAL_AGENT_TELEMETRY_ID: &str = "terminal";

const KNOWN_TERMINAL_AGENT_COMMANDS: &[&str] = &[
    "agent", // Unfortunately, both Cursor cli + grok
    "agy",
    "aider",
    "amp",
    "claude",
    "codex",
    "copilot",
    "crush",
    "devin",
    "droid",
    "gemini",
    "goose",
    "grok",
    "openhands",
    "opencode",
    "pi",
    "qwen",
];

pub(crate) fn is_known_terminal_agent_command(command: &str) -> bool {
    KNOWN_TERMINAL_AGENT_COMMANDS.contains(&command)
}

pub(crate) fn terminal_program_to_report(
    last_observed_program: &mut Option<String>,
    current_program: Option<String>,
) -> Option<String> {
    let current_program =
        current_program.filter(|program| is_known_terminal_agent_command(program));
    let program_to_report =
        if current_program.is_some() && current_program != *last_observed_program {
            current_program.clone()
        } else {
            None
        };
    *last_observed_program = current_program;
    program_to_report
}

/// Maximum number of idle threads kept in the agent panel's retained list.
/// Set as a GPUI global to override; otherwise defaults to 5.
pub struct MaxIdleRetainedThreads(pub usize);
impl gpui::Global for MaxIdleRetainedThreads {}

impl MaxIdleRetainedThreads {
    pub fn global(cx: &App) -> usize {
        cx.try_global::<Self>().map_or(5, |g| g.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TerminalId(uuid::Uuid);

impl TerminalId {
    pub(crate) fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub(crate) fn to_key_string(self) -> String {
        self.0.hyphenated().to_string()
    }

    pub(crate) fn from_key_string(key: &str) -> anyhow::Result<Self> {
        Ok(Self(uuid::Uuid::parse_str(key)?))
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
pub struct AgentPanelTerminalInfo {
    pub id: TerminalId,
    pub title: SharedString,
    pub created_at: DateTime<Utc>,
    pub has_notification: bool,
    pub custom_title: Option<SharedString>,
    pub working_directory: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_known_terminal_agent_command() {
        assert!(is_known_terminal_agent_command("claude"));
        assert!(is_known_terminal_agent_command("codex"));
        assert!(!is_known_terminal_agent_command("cargo"));
        assert!(!is_known_terminal_agent_command("internal-agent"));
    }

    #[test]
    fn test_terminal_program_to_report() {
        let mut last_observed_program = None;
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("codex".to_string())),
            Some("codex".to_string())
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("codex".to_string())),
            None
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("zsh".to_string())),
            None
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("claude".to_string())),
            Some("claude".to_string())
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("codex".to_string())),
            Some("codex".to_string())
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, None),
            None
        );
        assert_eq!(
            terminal_program_to_report(&mut last_observed_program, Some("codex".to_string())),
            Some("codex".to_string())
        );
    }
}
