use std::path::{Component, Path};
use std::sync::{Arc, LazyLock};

use settings::ToolPermissionMode;

#[derive(Clone, Debug, Default)]
pub struct ToolPermissions {
    /// Global default permission when no tool-specific rules or patterns match.
    pub default: ToolPermissionMode,
    pub tools: collections::HashMap<Arc<str>, ToolRules>,
}

impl ToolPermissions {
    /// Returns all invalid regex patterns across all tools.
    pub fn invalid_patterns(&self) -> Vec<&InvalidRegexPattern> {
        self.tools
            .values()
            .flat_map(|rules| rules.invalid_patterns.iter())
            .collect()
    }

    /// Returns true if any tool has invalid regex patterns.
    pub fn has_invalid_patterns(&self) -> bool {
        self.tools
            .values()
            .any(|rules| !rules.invalid_patterns.is_empty())
    }
}

/// Represents a regex pattern that failed to compile.
#[derive(Clone, Debug)]
pub struct InvalidRegexPattern {
    /// The pattern string that failed to compile.
    pub pattern: String,
    /// Which rule list this pattern was in (e.g., "always_deny", "always_allow", "always_confirm").
    pub rule_type: String,
    /// The error message from the regex compiler.
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct ToolRules {
    pub default: Option<ToolPermissionMode>,
    pub always_allow: Vec<CompiledRegex>,
    pub always_deny: Vec<CompiledRegex>,
    pub always_confirm: Vec<CompiledRegex>,
    /// Patterns that failed to compile. If non-empty, tool calls should be blocked.
    pub invalid_patterns: Vec<InvalidRegexPattern>,
}

#[derive(Clone)]
pub struct CompiledRegex {
    pub pattern: String,
    pub case_sensitive: bool,
    pub regex: regex::Regex,
}

impl std::fmt::Debug for CompiledRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRegex")
            .field("pattern", &self.pattern)
            .field("case_sensitive", &self.case_sensitive)
            .finish()
    }
}

impl CompiledRegex {
    pub fn new(pattern: &str, case_sensitive: bool) -> Option<Self> {
        Self::try_new(pattern, case_sensitive).ok()
    }

    pub fn try_new(pattern: &str, case_sensitive: bool) -> Result<Self, regex::Error> {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()?;
        Ok(Self {
            pattern: pattern.to_string(),
            case_sensitive,
            regex,
        })
    }

    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }
}

pub const HARDCODED_SECURITY_DENIAL_MESSAGE: &str = "Blocked by built-in security rule. This operation is considered too \
     harmful to be allowed, and cannot be overridden by settings.";

/// Security rules that are always enforced and cannot be overridden by any setting.
/// These protect against catastrophic operations like wiping filesystems.
pub struct HardcodedSecurityRules {
    pub terminal_deny: Vec<CompiledRegex>,
}

pub static HARDCODED_SECURITY_RULES: LazyLock<HardcodedSecurityRules> = LazyLock::new(|| {
    const FLAGS: &str = r"(--[a-zA-Z0-9][-a-zA-Z0-9_]*(=[^\s]*)?\s+|-[a-zA-Z]+\s+)*";
    const TRAILING_FLAGS: &str = r"(\s+--[a-zA-Z0-9][-a-zA-Z0-9_]*(=[^\s]*)?|\s+-[a-zA-Z]+)*\s*";

    HardcodedSecurityRules {
        terminal_deny: vec![
            // Recursive deletion of root - "rm -rf /", "rm -rf /*"
            CompiledRegex::new(
                &format!(r"\brm\s+{FLAGS}(--\s+)?/\*?{TRAILING_FLAGS}$"),
                false,
            )
            .expect("hardcoded regex should compile"),
            // Recursive deletion of home via tilde - "rm -rf ~", "rm -rf ~/"
            CompiledRegex::new(
                &format!(r"\brm\s+{FLAGS}(--\s+)?~/?\*?{TRAILING_FLAGS}$"),
                false,
            )
            .expect("hardcoded regex should compile"),
            // Recursive deletion of home via env var - "rm -rf $HOME", "rm -rf ${HOME}"
            CompiledRegex::new(
                &format!(r"\brm\s+{FLAGS}(--\s+)?(\$HOME|\$\{{HOME\}})/?(\*)?{TRAILING_FLAGS}$"),
                false,
            )
            .expect("hardcoded regex should compile"),
            // Recursive deletion of current directory - "rm -rf .", "rm -rf ./"
            CompiledRegex::new(
                &format!(r"\brm\s+{FLAGS}(--\s+)?\./?\*?{TRAILING_FLAGS}$"),
                false,
            )
            .expect("hardcoded regex should compile"),
            // Recursive deletion of parent directory - "rm -rf ..", "rm -rf ../"
            CompiledRegex::new(
                &format!(r"\brm\s+{FLAGS}(--\s+)?\.\./?\*?{TRAILING_FLAGS}$"),
                false,
            )
            .expect("hardcoded regex should compile"),
        ],
    }
});

/// Checks if input matches any hardcoded security rules that cannot be bypassed.
/// Returns the denial reason string if blocked, None otherwise.
///
/// `terminal_tool_name` should be the tool name used for the terminal tool
/// (e.g. `"terminal"`). `extracted_commands` can optionally provide parsed
/// sub-commands for chained command checking; callers with access to a shell
/// parser should extract sub-commands and pass them here.
pub fn check_hardcoded_security_rules(
    tool_name: &str,
    terminal_tool_name: &str,
    input: &str,
    extracted_commands: Option<&[String]>,
) -> Option<String> {
    if tool_name != terminal_tool_name {
        return None;
    }

    let rules = &*HARDCODED_SECURITY_RULES;
    let terminal_patterns = &rules.terminal_deny;

    if matches_hardcoded_patterns(input, terminal_patterns) {
        return Some(HARDCODED_SECURITY_DENIAL_MESSAGE.into());
    }

    if let Some(commands) = extracted_commands {
        for command in commands {
            if matches_hardcoded_patterns(command, terminal_patterns) {
                return Some(HARDCODED_SECURITY_DENIAL_MESSAGE.into());
            }
        }
    }

    None
}

fn matches_hardcoded_patterns(command: &str, patterns: &[CompiledRegex]) -> bool {
    for pattern in patterns {
        if pattern.is_match(command) {
            return true;
        }
    }

    for expanded in expand_rm_to_single_path_commands(command) {
        for pattern in patterns {
            if pattern.is_match(&expanded) {
                return true;
            }
        }
    }

    false
}

fn expand_rm_to_single_path_commands(command: &str) -> Vec<String> {
    let trimmed = command.trim();

    let first_token = trimmed.split_whitespace().next();
    if !first_token.is_some_and(|t| t.eq_ignore_ascii_case("rm")) {
        return vec![];
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let mut flags = Vec::new();
    let mut paths = Vec::new();
    let mut past_double_dash = false;

    for part in parts.iter().skip(1) {
        if !past_double_dash && *part == "--" {
            past_double_dash = true;
            flags.push(*part);
            continue;
        }
        if !past_double_dash && part.starts_with('-') {
            flags.push(*part);
        } else {
            paths.push(*part);
        }
    }

    let flags_str = if flags.is_empty() {
        String::new()
    } else {
        format!("{} ", flags.join(" "))
    };

    let mut results = Vec::new();
    for path in &paths {
        if path.starts_with('$') {
            let home_prefix = if path.starts_with("${HOME}") {
                Some("${HOME}")
            } else if path.starts_with("$HOME") {
                Some("$HOME")
            } else {
                None
            };

            if let Some(prefix) = home_prefix {
                let suffix = &path[prefix.len()..];
                if suffix.is_empty() {
                    results.push(format!("rm {flags_str}{path}"));
                } else if suffix.starts_with('/') {
                    let normalized_suffix = normalize_path(suffix);
                    let reconstructed = if normalized_suffix == "/" {
                        prefix.to_string()
                    } else {
                        format!("{prefix}{normalized_suffix}")
                    };
                    results.push(format!("rm {flags_str}{reconstructed}"));
                } else {
                    results.push(format!("rm {flags_str}{path}"));
                }
            } else {
                results.push(format!("rm {flags_str}{path}"));
            }
            continue;
        }

        let mut normalized = normalize_path(path);
        if normalized.is_empty() && !Path::new(path).has_root() {
            normalized = ".".to_string();
        }

        results.push(format!("rm {flags_str}{normalized}"));
    }

    results
}

pub fn normalize_path(raw: &str) -> String {
    let is_absolute = Path::new(raw).has_root();
    let mut components: Vec<&str> = Vec::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if components.last() == Some(&"..") {
                    components.push("..");
                } else if !components.is_empty() {
                    components.pop();
                } else if !is_absolute {
                    components.push("..");
                }
            }
            Component::Normal(segment) => {
                if let Some(s) = segment.to_str() {
                    components.push(s);
                }
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    let joined = components.join("/");
    if is_absolute {
        format!("/{joined}")
    } else {
        joined
    }
}
