use super::*;

pub enum ToolPermissionDecision {
    Allow,
    Deny(String),
    Confirm,
}

impl ToolPermissionDecision {
    /// Determines the permission decision for a tool invocation based on configured rules.
    ///
    /// # Precedence Order (highest to lowest)
    ///
    /// 1. **Hardcoded security rules** - Critical safety checks (e.g., blocking `rm -rf /`)
    ///    that cannot be bypassed by any user settings.
    /// 2. **`always_deny`** - If any deny pattern matches, the tool call is blocked immediately.
    ///    This takes precedence over `always_confirm` and `always_allow` patterns.
    /// 3. **`always_confirm`** - If any confirm pattern matches (and no deny matched),
    ///    the user is prompted for confirmation.
    /// 4. **`always_allow`** - If any allow pattern matches (and no deny/confirm matched),
    ///    the tool call proceeds without prompting.
    /// 5. **Tool-specific `default`** - If no patterns match and the tool has an explicit
    ///    `default` configured, that mode is used.
    /// 6. **Global `default`** - Falls back to `tool_permissions.default` when no
    ///    tool-specific default is set, or when the tool has no entry at all.
    ///
    /// # Shell Compatibility (Terminal Tool Only)
    ///
    /// For the terminal tool, commands are parsed to extract sub-commands for security.
    /// All currently supported `ShellKind` variants are treated as compatible because
    /// brush-parser can handle their command chaining syntax. If a new `ShellKind`
    /// variant is added that brush-parser cannot safely parse, it should be excluded
    /// from `ShellKind::supports_posix_chaining()`, which will cause `always_allow`
    /// patterns to be disabled for that shell.
    ///
    /// # Pattern Matching Tips
    ///
    /// Patterns are matched as regular expressions against the tool input (e.g., the command
    /// string for the terminal tool). Some tips for writing effective patterns:
    ///
    /// - Use word boundaries (`\b`) to avoid partial matches. For example, pattern `rm` will
    ///   match "storm" and "arms", but `\brm\b` will only match the standalone word "rm".
    ///   This is important for security rules where you want to block specific commands
    ///   without accidentally blocking unrelated commands that happen to contain the same
    ///   substring.
    /// - Patterns are case-insensitive by default. Set `case_sensitive: true` for exact matching.
    /// - Use `^` and `$` anchors to match the start/end of the input.
    pub fn from_input(
        tool_name: &str,
        inputs: &[String],
        permissions: &ToolPermissions,
        shell_kind: ShellKind,
    ) -> ToolPermissionDecision {
        // First, check hardcoded security rules, such as banning `rm -rf /` in terminal tool.
        // These cannot be bypassed by any user settings.
        if let Some(denial) = check_hardcoded_security_rules(tool_name, inputs, shell_kind) {
            return denial;
        }

        let rules = permissions.tools.get(tool_name);

        // Check for invalid regex patterns before evaluating rules.
        // If any patterns failed to compile, block the tool call entirely.
        if let Some(error) = rules.and_then(|rules| check_invalid_patterns(tool_name, rules)) {
            return ToolPermissionDecision::Deny(error);
        }

        if tool_name == TerminalTool::NAME
            && !rules.map_or(
                matches!(permissions.default, ToolPermissionMode::Allow),
                |rules| is_unconditional_allow_all(rules, permissions.default),
            )
            && inputs.iter().any(|input| {
                matches!(
                    validate_terminal_command(input),
                    TerminalCommandValidation::Unsafe | TerminalCommandValidation::Unsupported
                )
            })
        {
            return ToolPermissionDecision::Deny(INVALID_TERMINAL_COMMAND_MESSAGE.into());
        }

        let rules = match rules {
            Some(rules) => rules,
            None => {
                // No tool-specific rules, use the global default
                return match permissions.default {
                    ToolPermissionMode::Allow => ToolPermissionDecision::Allow,
                    ToolPermissionMode::Deny => {
                        ToolPermissionDecision::Deny("Blocked by global default: deny".into())
                    }
                    ToolPermissionMode::Confirm => ToolPermissionDecision::Confirm,
                };
            }
        };

        // For the terminal tool, parse each input command to extract all sub-commands.
        // This prevents shell injection attacks where a user configures an allow
        // pattern like "^ls" and an attacker crafts "ls && rm -rf /".
        //
        // If parsing fails or the shell syntax is unsupported, always_allow is
        // disabled for this command (we set allow_enabled to false to signal this).
        if tool_name == TerminalTool::NAME {
            // Our shell parser (brush-parser) only supports POSIX-like shell syntax.
            // See the doc comment above for the list of compatible/incompatible shells.
            if !shell_kind.supports_posix_chaining() {
                // For shells with incompatible syntax, we can't reliably parse
                // the command to extract sub-commands.
                if !rules.always_allow.is_empty() {
                    // If the user has configured always_allow patterns, we must deny
                    // because we can't safely verify the command doesn't contain
                    // hidden sub-commands that bypass the allow patterns.
                    return ToolPermissionDecision::Deny(format!(
                        "The {} shell does not support \"always allow\" patterns for the terminal \
                         tool because Mav cannot parse its command chaining syntax. Please remove \
                         the always_allow patterns from your tool_permissions settings, or switch \
                         to a POSIX-conforming shell.",
                        shell_kind
                    ));
                }
                // No always_allow rules, so we can still check deny/confirm patterns.
                return check_commands(
                    inputs.iter().map(|s| s.to_string()),
                    rules,
                    tool_name,
                    false,
                    permissions.default,
                );
            }

            // Expand each input into its sub-commands and check them all together.
            let mut all_commands = Vec::new();
            let mut any_parse_failed = false;
            for input in inputs {
                match extract_commands(input) {
                    Some(commands) => all_commands.extend(commands),
                    None => {
                        any_parse_failed = true;
                        all_commands.push(input.to_string());
                    }
                }
            }
            // If any command failed to parse, disable allow patterns for safety.
            check_commands(
                all_commands,
                rules,
                tool_name,
                !any_parse_failed,
                permissions.default,
            )
        } else {
            check_commands(
                inputs.iter().map(|s| s.to_string()),
                rules,
                tool_name,
                true,
                permissions.default,
            )
        }
    }
}

/// Evaluates permission rules against a set of commands.
///
/// This function performs a single pass through all commands with the following logic:
/// - **DENY**: If ANY command matches a deny pattern, deny immediately (short-circuit)
/// - **CONFIRM**: Track if ANY command matches a confirm pattern
/// - **ALLOW**: Track if ALL commands match at least one allow pattern
///
/// The `allow_enabled` flag controls whether allow patterns are checked. This is set
/// to `false` when we can't reliably parse shell commands (e.g., parse failures or
/// unsupported shell syntax), ensuring we don't auto-allow potentially dangerous commands.
pub(super) fn check_commands(
    commands: impl IntoIterator<Item = String>,
    rules: &ToolRules,
    tool_name: &str,
    allow_enabled: bool,
    global_default: ToolPermissionMode,
) -> ToolPermissionDecision {
    // Single pass through all commands:
    // - DENY: If ANY command matches a deny pattern, deny immediately (short-circuit)
    // - CONFIRM: Track if ANY command matches a confirm pattern
    // - ALLOW: Track if ALL commands match at least one allow pattern
    let mut any_matched_confirm = false;
    let mut all_matched_allow = true;
    let mut had_any_commands = false;

    for command in commands {
        had_any_commands = true;

        // DENY: immediate return if any command matches a deny pattern
        if rules.always_deny.iter().any(|r| r.is_match(&command)) {
            return ToolPermissionDecision::Deny(format!(
                "Command blocked by security rule for {} tool",
                tool_name
            ));
        }

        // CONFIRM: remember if any command matches a confirm pattern
        if rules.always_confirm.iter().any(|r| r.is_match(&command)) {
            any_matched_confirm = true;
        }

        // ALLOW: track if all commands match at least one allow pattern
        if !rules.always_allow.iter().any(|r| r.is_match(&command)) {
            all_matched_allow = false;
        }
    }

    // After processing all commands, check accumulated state
    if any_matched_confirm {
        return ToolPermissionDecision::Confirm;
    }

    if allow_enabled && all_matched_allow && had_any_commands {
        return ToolPermissionDecision::Allow;
    }

    match rules.default.unwrap_or(global_default) {
        ToolPermissionMode::Deny => {
            ToolPermissionDecision::Deny(format!("{} tool is disabled", tool_name))
        }
        ToolPermissionMode::Allow => ToolPermissionDecision::Allow,
        ToolPermissionMode::Confirm => ToolPermissionDecision::Confirm,
    }
}

pub(super) fn is_unconditional_allow_all(
    rules: &ToolRules,
    global_default: ToolPermissionMode,
) -> bool {
    // `always_allow` is intentionally not checked here: when the effective default
    // is already Allow and there are no deny/confirm restrictions, allow patterns
    // are redundant — the user has opted into allowing everything.
    rules.always_deny.is_empty()
        && rules.always_confirm.is_empty()
        && matches!(
            rules.default.unwrap_or(global_default),
            ToolPermissionMode::Allow
        )
}

/// Checks if the tool rules contain any invalid regex patterns.
/// Returns an error message if invalid patterns are found.
pub(super) fn check_invalid_patterns(tool_name: &str, rules: &ToolRules) -> Option<String> {
    if rules.invalid_patterns.is_empty() {
        return None;
    }

    let count = rules.invalid_patterns.len();
    let pattern_word = if count == 1 { "pattern" } else { "patterns" };

    Some(format!(
        "The {} tool cannot run because {} regex {} failed to compile. \
         Please fix the invalid patterns in your tool_permissions settings.",
        tool_name, count, pattern_word
    ))
}

/// Convenience wrapper that extracts permission settings from `AgentSettings`.
///
/// This is the primary entry point for tools to check permissions. It extracts
/// `tool_permissions` from the settings and
/// delegates to [`ToolPermissionDecision::from_input`], using the system shell.
pub fn decide_permission_from_settings(
    tool_name: &str,
    inputs: &[String],
    settings: &AgentSettings,
) -> ToolPermissionDecision {
    ToolPermissionDecision::from_input(
        tool_name,
        inputs,
        &settings.tool_permissions,
        ShellKind::system(),
    )
}
