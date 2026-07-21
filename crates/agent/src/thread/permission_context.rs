use super::*;

#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub tool_name: String,
    pub input_values: Vec<String>,
    pub scope: ToolPermissionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionScope {
    ToolInput,
    SymlinkTarget,
    AgentSkills,
}

impl ToolPermissionContext {
    pub fn new(tool_name: impl Into<String>, input_values: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values,
            scope: ToolPermissionScope::ToolInput,
        }
    }

    pub fn symlink_target(tool_name: impl Into<String>, target_paths: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values: target_paths,
            scope: ToolPermissionScope::SymlinkTarget,
        }
    }

    pub fn for_agent_skills(mut self) -> Self {
        self.scope = ToolPermissionScope::AgentSkills;
        self
    }

    /// Builds the permission options for this tool context.
    ///
    /// This is the canonical source for permission option generation.
    /// Tests should use this function rather than manually constructing options.
    ///
    /// # Shell Compatibility for Terminal Tool
    ///
    /// For the terminal tool, "Always allow" options are only shown when the user's
    /// shell supports POSIX-like command chaining syntax (`&&`, `||`, `;`, `|`).
    ///
    /// **Why this matters:** When a user sets up an "always allow" pattern like `^cargo`,
    /// we need to parse the command to extract all sub-commands and verify that EVERY
    /// sub-command matches the pattern. Otherwise, an attacker could craft a command like
    /// `cargo build && rm -rf /` that would bypass the security check.
    ///
    /// **Supported shells:** Posix (sh, bash, dash, zsh), Fish 3.0+, PowerShell 7+/Pwsh,
    /// Cmd, Xonsh, Csh, Tcsh
    ///
    /// **Unsupported shells:** Nushell (uses `and`/`or` keywords), Elvish (uses `and`/`or`
    /// keywords), Rc (Plan 9 shell - no `&&`/`||` operators)
    ///
    /// For unsupported shells, we hide the "Always allow" UI options entirely, and if
    /// the user has `always_allow` rules configured in settings, `ToolPermissionDecision::from_input`
    /// will return a `Deny` with an explanatory error message.
    pub fn build_permission_options(&self) -> acp_thread::PermissionOptions {
        use crate::pattern_extraction::*;
        use util::shell::ShellKind;

        let tool_name = &self.tool_name;
        let input_values = &self.input_values;
        if self.scope == ToolPermissionScope::SymlinkTarget {
            return acp_thread::PermissionOptions::Flat(vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Yes",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "No",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ]);
        }

        // Skills always prompt, so offer only once-only allow/deny.
        if self.scope == ToolPermissionScope::AgentSkills {
            return acp_thread::PermissionOptions::Flat(vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "Deny",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ]);
        }

        // Check if the user's shell supports POSIX-like command chaining.
        // See the doc comment above for the full explanation of why this is needed.
        let shell_supports_always_allow = if tool_name == TerminalTool::NAME {
            ShellKind::system().supports_posix_chaining()
        } else {
            true
        };

        // For terminal commands with multiple pipeline commands, use DropdownWithPatterns
        // to let users individually select which command patterns to always allow.
        if tool_name == TerminalTool::NAME && shell_supports_always_allow {
            if let Some(input) = input_values.first() {
                let all_patterns = extract_all_terminal_patterns(input);
                if all_patterns.len() > 1 {
                    let mut choices = Vec::new();
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_allow:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::AllowAlways,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_deny:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::RejectAlways,
                        ),
                        sub_patterns: vec![],
                    });
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("allow"),
                            "Only this time",
                            acp::PermissionOptionKind::AllowOnce,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("deny"),
                            "Only this time",
                            acp::PermissionOptionKind::RejectOnce,
                        ),
                        sub_patterns: vec![],
                    });
                    return acp_thread::PermissionOptions::DropdownWithPatterns {
                        choices,
                        patterns: all_patterns,
                        tool_name: tool_name.clone(),
                    };
                }
            }
        }

        let extract_for_value = |value: &str| -> (Option<String>, Option<String>) {
            if tool_name == TerminalTool::NAME {
                (
                    extract_terminal_pattern(value),
                    extract_terminal_pattern_display(value),
                )
            } else if tool_name == CopyPathTool::NAME
                || tool_name == MovePathTool::NAME
                || tool_name == EditFileTool::NAME
                || tool_name == WriteFileTool::NAME
                || tool_name == DeletePathTool::NAME
                || tool_name == CreateDirectoryTool::NAME
            {
                (
                    extract_path_pattern(value),
                    extract_path_pattern_display(value),
                )
            } else if tool_name == FetchTool::NAME {
                (
                    extract_url_pattern(value),
                    extract_url_pattern_display(value),
                )
            } else {
                (None, None)
            }
        };

        // Extract patterns from all input values. Only offer a pattern-specific
        // "always allow/deny" button when every value produces the same pattern.
        let (pattern, pattern_display) = match input_values.as_slice() {
            [single] => extract_for_value(single),
            _ => {
                let mut iter = input_values.iter().map(|v| extract_for_value(v));
                match iter.next() {
                    Some(first) => {
                        if iter.all(|pair| pair.0 == first.0) {
                            first
                        } else {
                            (None, None)
                        }
                    }
                    None => (None, None),
                }
            }
        };

        let mut choices = Vec::new();

        let mut push_choice =
            |label: String, allow_id, deny_id, allow_kind, deny_kind, sub_patterns: Vec<String>| {
                choices.push(acp_thread::PermissionOptionChoice {
                    allow: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(allow_id),
                        label.clone(),
                        allow_kind,
                    ),
                    deny: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(deny_id),
                        label,
                        deny_kind,
                    ),
                    sub_patterns,
                });
            };

        if shell_supports_always_allow {
            push_choice(
                format!("Always for {}", tool_name.replace('_', " ")),
                format!("always_allow:{}", tool_name),
                format!("always_deny:{}", tool_name),
                acp::PermissionOptionKind::AllowAlways,
                acp::PermissionOptionKind::RejectAlways,
                vec![],
            );

            if let (Some(pattern), Some(display)) = (pattern, pattern_display) {
                let button_text = if tool_name == TerminalTool::NAME {
                    format!("Always for `{}` commands", display)
                } else {
                    format!("Always for `{}`", display)
                };
                push_choice(
                    button_text,
                    format!("always_allow:{}", tool_name),
                    format!("always_deny:{}", tool_name),
                    acp::PermissionOptionKind::AllowAlways,
                    acp::PermissionOptionKind::RejectAlways,
                    vec![pattern],
                );
            }
        }

        push_choice(
            "Only this time".to_string(),
            "allow".to_string(),
            "deny".to_string(),
            acp::PermissionOptionKind::AllowOnce,
            acp::PermissionOptionKind::RejectOnce,
            vec![],
        );

        acp_thread::PermissionOptions::Dropdown(choices)
    }
}

#[derive(Debug)]
pub struct ToolCallAuthorization {
    pub tool_call: acp::ToolCallUpdate,
    pub options: acp_thread::PermissionOptions,
    pub response: oneshot::Sender<acp_thread::SelectedPermissionOutcome>,
    pub context: Option<ToolPermissionContext>,
    pub kind: acp_thread::AuthorizationKind,
}

pub(crate) fn auto_resolve_permission_outcome(
    options: &acp_thread::PermissionOptions,
    is_allow: bool,
) -> Result<acp_thread::SelectedPermissionOutcome> {
    let kind = if is_allow {
        acp::PermissionOptionKind::AllowOnce
    } else {
        acp::PermissionOptionKind::RejectOnce
    };
    let option = options
        .first_option_of_kind(kind)
        .ok_or_else(|| anyhow!("permission prompt has no auto-resolution option"))?;

    Ok(acp_thread::SelectedPermissionOutcome::new(
        option.option_id.clone(),
        option.kind,
    ))
}
