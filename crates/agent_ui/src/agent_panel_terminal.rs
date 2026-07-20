use std::{fmt, path::PathBuf};

use chrono::{DateTime, Utc};
use editor::Editor;
use gpui::{App, Entity, SharedString, Subscription, WindowHandle};
use terminal_view::TerminalView;

use crate::{
    AgentThreadSource,
    terminal_thread_metadata_store::{
        compose_terminal_thread_title, terminal_title_without_prefix,
    },
    ui::AgentNotification,
};

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

pub(crate) struct AgentTerminal {
    pub(crate) view: Entity<TerminalView>,
    pub(crate) title_editor: Option<Entity<Editor>>,
    pub(crate) title_editor_initial_title: Option<String>,
    pub(crate) title_editor_subscription: Option<Subscription>,
    pub(crate) last_known_title: String,
    pub(crate) last_known_terminal_title: String,
    pub(crate) last_observed_program: Option<String>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) has_notification: bool,
    pub(crate) notification_windows: Vec<WindowHandle<AgentNotification>>,
    pub(crate) notification_subscriptions: Vec<Subscription>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

impl AgentTerminal {
    pub(crate) fn terminal_title_for_view(view: &TerminalView, cx: &App) -> SharedString {
        let terminal = view.terminal().read(cx);
        if terminal.breadcrumb_text.is_empty() {
            let title = terminal.title(true);
            if title == "Terminal" {
                SharedString::from("")
            } else {
                title.into()
            }
        } else {
            terminal.breadcrumb_text.clone().into()
        }
    }

    pub(crate) fn current_terminal_title(&self, cx: &App) -> SharedString {
        let view = self.view.read(cx);
        Self::terminal_title_for_view(view, cx)
    }

    pub(crate) fn terminal_title(&self, cx: &App) -> SharedString {
        let title = self.current_terminal_title(cx);
        if title.is_empty() && !self.last_known_terminal_title.is_empty() {
            SharedString::from(self.last_known_terminal_title.clone())
        } else {
            title
        }
    }

    pub(crate) fn title(&self, cx: &App) -> SharedString {
        let terminal_title = self.terminal_title(cx);
        let custom_title = self.custom_title(cx);
        compose_terminal_thread_title(
            terminal_title.as_ref(),
            custom_title.as_ref().map(|title| title.as_ref()),
        )
    }

    pub(crate) fn editable_title(&self, cx: &App) -> SharedString {
        if let Some(custom_title) = self.custom_title(cx) {
            custom_title
        } else {
            let terminal_title = self.terminal_title(cx);
            SharedString::from(terminal_title_without_prefix(terminal_title.as_ref()).to_string())
        }
    }

    pub(crate) fn refresh_title(&mut self, cx: &mut App) -> bool {
        let terminal_title = self.current_terminal_title(cx);
        if !terminal_title.is_empty() {
            self.last_known_terminal_title = terminal_title.to_string();
        }

        let title = self.title(cx);
        let changed = self.last_known_title != title.as_ref();
        if changed {
            self.last_known_title = title.to_string();
        }
        changed
    }

    pub(crate) fn refresh_metadata(&mut self, cx: &mut App) -> bool {
        let title_changed = self.refresh_title(cx);
        let current_working_directory = self.view.read(cx).terminal().read(cx).working_directory();
        let working_directory_changed = current_working_directory
            .as_ref()
            .is_some_and(|current| self.working_directory.as_ref() != Some(current));
        if working_directory_changed {
            self.working_directory = current_working_directory;
        }
        title_changed || working_directory_changed
    }

    pub(crate) fn custom_title(&self, cx: &App) -> Option<SharedString> {
        self.view.read(cx).custom_title().map(SharedString::from)
    }

    pub(crate) fn report_started_terminal_program(
        &mut self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        cx: &App,
    ) {
        let current_program = self
            .view
            .read(cx)
            .terminal()
            .read(cx)
            .foreground_process_command_name();

        if let Some(program) =
            terminal_program_to_report(&mut self.last_observed_program, current_program)
        {
            telemetry::event!(
                "Agent Terminal Program Started",
                agent = TERMINAL_AGENT_TELEMETRY_ID,
                terminal_id = terminal_id.to_key_string(),
                program = program,
                source = source.as_str(),
                side = crate::sidebar_side(cx),
                thread_location = "current_worktree",
            );
        }
    }
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
