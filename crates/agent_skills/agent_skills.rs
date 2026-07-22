#[path = "agent_skills/builtin.rs"]
mod builtin;
#[path = "agent_skills/frontmatter.rs"]
mod frontmatter;
#[path = "agent_skills/loading.rs"]
mod loading;
#[path = "agent_skills/paths.rs"]
mod paths;
#[path = "agent_skills/share_link.rs"]
mod share_link;
#[path = "agent_skills/validation.rs"]
mod validation;

#[cfg(test)]
#[path = "agent_skills/tests.rs"]
mod tests;

use anyhow::{Context as _, Result};
use const_format::{concatcp, formatcp};
use fs::Fs;
use futures::StreamExt;
use gpui::{App, Global, SharedString};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use url::Url;
use util::paths::component_matches_ignore_ascii_case;

/// First segment of the skills directory path: `.agents`.
pub const AGENTS_DIR_NAME: &str = ".agents";

/// Second segment of the skills directory path: `skills`.
pub const SKILLS_DIR_NAME: &str = "skills";

/// User-facing display form of the global skills directory path — i.e.
/// what a human should see in messages and prompts, with the platform's
/// native path separator and home-directory shorthand.
///
/// Windows doesn't recognize `~` as the home directory, so the env-var
/// form is used there instead.
#[cfg(target_os = "windows")]
pub const GLOBAL_SKILLS_DIR_DISPLAY: &str =
    concatcp!("%USERPROFILE%\\", AGENTS_DIR_NAME, "\\", SKILLS_DIR_NAME);
#[cfg(not(target_os = "windows"))]
pub const GLOBAL_SKILLS_DIR_DISPLAY: &str = concatcp!("~/", AGENTS_DIR_NAME, "/", SKILLS_DIR_NAME);

/// Opaque identifier for the project scope a skill was loaded from.
///
/// `agent_skills` is a leaf crate and intentionally does not depend on
/// `worktree`. Callers (e.g. the `agent` crate) construct these from
/// `worktree::WorktreeId::to_usize()` and recover the original ID via
/// `worktree::WorktreeId::from_usize()` when needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkillScopeId(pub usize);

/// Cap on concurrent filesystem operations during skill discovery and loading.
/// Without this bound, a `.agents/skills` directory containing thousands of
/// entries would fan out an equally large number of concurrent OS-level I/O
/// operations, potentially exhausting file descriptors or stalling the app.
const SKILL_IO_CONCURRENCY: usize = 16;

/// Maximum size for a single SKILL.md file (100KB)
pub const MAX_SKILL_FILE_SIZE: usize = 100 * 1024;

/// Maximum total size for skill descriptions in system prompt (50KB)
pub const MAX_SKILL_DESCRIPTIONS_SIZE: usize = 50 * 1024;

/// The name of the skill definition file
pub const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SkillLoadWarning {
    DescriptionTooLong { actual_len: usize, max_len: usize },
}

impl SkillLoadWarning {
    pub fn message(&self) -> String {
        match self {
            Self::DescriptionTooLong {
                actual_len,
                max_len,
            } => format!(
                "Skill description is {actual_len} bytes, exceeding the {max_len}-byte limit. The skill was loaded, but long descriptions may consume more model-context tokens."
            ),
        }
    }
}

/// Represents a loaded skill with all its metadata and content.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    /// Absolute path to the skill directory
    pub directory_path: PathBuf,
    /// Absolute path to the SKILL.md file
    pub skill_file_path: PathBuf,
    /// Non-fatal issues found while loading this skill.
    pub load_warnings: Vec<SkillLoadWarning>,
    /// When `true`, this skill is hidden from the model's catalog and the
    /// `skill` tool refuses to load it. The user can still invoke it as a
    /// slash command.
    pub disable_model_invocation: bool,
    /// For built-in skills whose content is compiled into the binary,
    /// this holds the full SKILL.md body so the skill tool can serve it
    /// without a filesystem read.
    pub embedded_body: Option<&'static str>,
}

/// Indicates where a skill was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// Compiled into the Mav binary. These are always available and have
    /// the lowest override priority (global and project-local skills can
    /// shadow them).
    BuiltIn,
    /// From ~/.agents/skills/
    Global,
    /// From {project}/.agents/skills/
    ProjectLocal {
        worktree_id: SkillScopeId,
        worktree_root_name: Arc<str>,
    },
}

impl SkillSource {
    /// Precedence for resolving same-named skills. Higher values shadow
    /// lower ones: `ProjectLocal` > `Global` > `BuiltIn`. Two sources
    /// returning equal precedence (e.g. two project-local skills from
    /// different worktrees) leave the winner up to the caller, which by
    /// convention keeps the first one in iteration order.
    ///
    /// Adding a new `SkillSource` variant should be a one-line change
    /// here — every consumer routes through this method so the hierarchy
    /// stays in sync.
    pub fn precedence(&self) -> u8 {
        match self {
            Self::BuiltIn => 0,
            Self::Global => 1,
            Self::ProjectLocal { .. } => 2,
        }
    }

    /// Scope prefix used in the `/<prefix>:<name>` slash-command
    /// syntax that the autocomplete popup inserts. Global skills use
    /// an empty prefix (so the inserted text is `/:<name>`), and
    /// project-local skills use their worktree root name (so the
    /// inserted text is `/<worktree>:<name>`).
    ///
    /// Using an empty prefix for globals rather than a literal
    /// `global` means a worktree literally named `global` is no
    /// longer ambiguous with the global source: the global skill is
    /// invoked as `/:<name>`, and the worktree's skill is invoked as
    /// `/global:<name>`. The two grammars never collide on the
    /// inserted text.
    /// Human-readable label for this source, used in the UI to
    /// distinguish skills from different origins.
    pub fn display_label(&self) -> &str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Global => "global",
            Self::ProjectLocal {
                worktree_root_name, ..
            } => worktree_root_name.as_ref(),
        }
    }

    pub fn scope_prefix(&self) -> &str {
        match self {
            Self::BuiltIn | Self::Global => "",
            Self::ProjectLocal {
                worktree_root_name, ..
            } => worktree_root_name.as_ref(),
        }
    }

    /// Whether this source matches the given scope qualifier from a
    /// `/<scope>:<name>` slash command. The empty scope is reserved
    /// for global skills; non-empty scopes match a project-local
    /// skill whose worktree root name equals the scope.
    ///
    /// Hand-typed `/global:<name>` is NOT treated as an alias for
    /// `/:<name>`. It looks for a project-local skill from a worktree
    /// named `global` and fails if none exists. The popup always
    /// inserts the unambiguous form (`/:<name>` for globals), so this
    /// strictness only affects users typing by memory.
    pub fn matches_scope(&self, scope: &str) -> bool {
        match self {
            Self::BuiltIn | Self::Global => scope.is_empty(),
            Self::ProjectLocal {
                worktree_root_name, ..
            } => !scope.is_empty() && worktree_root_name.as_ref() == scope,
        }
    }
}

/// App-wide index of loaded skills, published by NativeAgent and read
/// by any UI that needs to display the skill list (e.g. Settings UI).
#[derive(Default)]
pub struct SkillIndex {
    pub global_skills: Vec<Skill>,
    pub project_skills: Vec<ProjectSkillGroup>,
}

#[derive(Clone)]
pub struct ProjectSkillGroup {
    pub worktree_id: SkillScopeId,
    pub worktree_root_name: SharedString,
    pub skills: Vec<Skill>,
}

impl Global for SkillIndex {}

/// Rescan skill agent skill directories when skills are created or modified via UI
pub struct SkillsUpdatedHook(pub Rc<dyn Fn(&mut App)>);

impl Global for SkillsUpdatedHook {}

/// Just the frontmatter, used for parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default, rename = "disable-model-invocation")]
    pub disable_model_invocation: bool,
}

/// Minimal skill info for system prompt.
///
/// `Serialize` is required for handlebars rendering of the system prompt
/// template (see `ProjectContext` in `prompt_store`). `PartialEq, Eq` lets
/// the agent compare freshly-built `ProjectContext`s and skip pushing an
/// unchanged value through the project_context entity (which would
/// otherwise look like a system-prompt change to the model and invalidate
/// the API's prompt cache).
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    /// Absolute path to the SKILL.md file, so the model can resolve
    /// references relative to the skill's directory when reading bundled
    /// resources.
    pub location: String,
}

impl From<&Skill> for SkillSummary {
    fn from(skill: &Skill) -> Self {
        Self {
            name: skill.name.clone(),
            description: skill.description.clone(),
            location: skill.skill_file_path.to_string_lossy().into_owned(),
        }
    }
}

/// Error that occurred while loading a skill
#[derive(Debug, Clone)]
pub struct SkillLoadError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for SkillLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for SkillLoadError {}

/// Parse the frontmatter of a SKILL.md file into a `Skill` struct.
///
/// The file must have YAML frontmatter between `---` delimiters containing
/// `name` and `description` fields. The body (everything after the closing
/// `---`) is intentionally NOT returned — it's read on demand via
/// `read_skill_body` when the skill is actually being materialized for the
/// model, so we don't pay N × body-size in memory for N skills.
///
/// `content` only needs to contain bytes up through the closing `---`; any
pub use builtin::{builtin_skill_content, builtin_skills};
pub use frontmatter::{
    extract_skill_frontmatter, parse_skill_file_content, parse_skill_frontmatter,
};
pub use loading::{
    load_skill_frontmatter, load_skills_from_directory, read_skill_body,
    read_skill_body_from_content,
};
pub use paths::{global_skills_dir, is_agents_skills_path, project_skills_relative_path};
pub use share_link::{SKILL_SHARE_LINK_PREFIX, decode_skill_share_link, encode_skill_share_link};
pub use validation::{
    MAX_SKILL_DESCRIPTION_LEN, MAX_SKILL_NAME_LEN, slugify_skill_name, validate_description,
    validate_name,
};
