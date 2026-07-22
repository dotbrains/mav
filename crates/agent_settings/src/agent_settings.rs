mod agent_profile;
mod layout;
mod settings_impl;
mod settings_types;
mod tool_permissions;
mod user_agents_md;

#[cfg(test)]
mod tests;

pub use crate::agent_profile::*;
pub use crate::layout::{AutoCompactSettings, AutoCompactThreshold, PanelLayout, WindowLayout};
pub use crate::settings_types::{
    AgentProfileId, AgentSettings, SandboxPermissions, language_model_to_selection,
};
pub use crate::tool_permissions::{
    CompiledRegex, HARDCODED_SECURITY_DENIAL_MESSAGE, HARDCODED_SECURITY_RULES,
    HardcodedSecurityRules, InvalidRegexPattern, ToolPermissions, ToolRules,
    check_hardcoded_security_rules, normalize_path,
};
pub use crate::user_agents_md::{UserAgentsMd, UserAgentsMdState, init as init_user_agents_md};

pub const SUMMARIZE_THREAD_PROMPT: &str = include_str!("prompts/summarize_thread_prompt.txt");
pub const SUMMARIZE_THREAD_DETAILED_PROMPT: &str =
    include_str!("prompts/summarize_thread_detailed_prompt.txt");
pub const COMPACTION_PROMPT: &str = include_str!("prompts/compaction_prompt.txt");
