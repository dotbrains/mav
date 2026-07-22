use super::*;

pub fn global_skills_dir() -> PathBuf {
    ::paths::home_dir()
        .join(AGENTS_DIR_NAME)
        .join(SKILLS_DIR_NAME)
}

/// Project-local skills live at this path relative to a worktree root,
/// e.g. `<worktree>/.agents/skills/<skill>/SKILL.md`.
pub fn project_skills_relative_path() -> &'static str {
    ".agents/skills"
}

/// Returns `true` if `path` looks like it points into an agent skills
/// directory — i.e. it contains `AGENTS_DIR_NAME` immediately followed by
/// `SKILLS_DIR_NAME` as two consecutive path components, anywhere in the
/// path. Comparison is case-insensitive so it agrees with classifiers
/// that canonicalize against `~/.agents/skills` on case-insensitive
/// filesystems (macOS/Windows by default).
///
/// The path arriving here can be any of:
///
///   1. Bare relative-to-worktree-root: `.agents/skills/...`
///   2. Worktree-name prefixed:         `<worktree>/.agents/skills/...`
///   3. Absolute:                       `/path/to/worktree/.agents/skills/...`
///
/// Any-depth matching has a known cost: a `.agents/skills` directory
/// nested inside vendored sources (e.g. `vendor/x/.agents/skills/...`)
/// would also be flagged. We accept that as the safer-failing direction —
/// an extra confirmation prompt for a vendored file is annoying, while
/// silently letting the agent overwrite a `.agents/skills` tree the user
/// didn't expect to be touched is unsafe.
pub fn is_agents_skills_path(path: &Path) -> bool {
    let mut components = path.components().map(|c| c.as_os_str());
    let Some(mut prev) = components.next() else {
        return false;
    };
    for curr in components {
        if component_matches_ignore_ascii_case(prev, AGENTS_DIR_NAME)
            && component_matches_ignore_ascii_case(curr, SKILLS_DIR_NAME)
        {
            return true;
        }
        prev = curr;
    }
    false
}
