use super::frontmatter::extract_frontmatter;
use super::*;

/// Content of the built-in `create-skill` SKILL.md, embedded at compile time.
const CREATE_SKILL_CONTENT: &str = include_str!("../builtin/create-skill/SKILL.md");

/// Returns the set of skills that are compiled into the Mav binary.
pub fn builtin_skills() -> Vec<Skill> {
    let mut skills = Vec::new();
    if let Ok(skill) = parse_builtin_skill("create-skill", CREATE_SKILL_CONTENT) {
        skills.push(skill);
    }
    skills
}

/// Parse a built-in skill from its embedded SKILL.md content. The skill
/// gets a synthetic `<built-in>` path since it doesn't live on disk.
fn parse_builtin_skill(name: &str, content: &'static str) -> Result<Skill> {
    let (metadata, body) = extract_frontmatter(content)?;
    validate_name(&metadata.name).map_err(anyhow::Error::msg)?;
    validate_description(&metadata.description).map_err(anyhow::Error::msg)?;

    let synthetic_dir = PathBuf::from(format!("<built-in>/{}", name));
    let synthetic_path = synthetic_dir.join(SKILL_FILE_NAME);

    Ok(Skill {
        name: metadata.name,
        description: metadata.description,
        source: SkillSource::BuiltIn,
        directory_path: synthetic_dir,
        skill_file_path: synthetic_path,
        load_warnings: Vec::new(),
        disable_model_invocation: metadata.disable_model_invocation,
        embedded_body: Some(body.trim()),
    })
}

/// All built-in skills as `(name, raw_content)` pairs. Used by
/// `builtin_skill_content` to serve the full SKILL.md without disk I/O.
const BUILTIN_SKILL_ENTRIES: &[(&str, &str)] = &[("create-skill", CREATE_SKILL_CONTENT)];

/// Look up the full embedded content of a built-in skill by its
/// synthetic file path. Returns `None` if the path doesn't match any
/// built-in skill.
pub fn builtin_skill_content(skill_file_path: &Path) -> Option<&'static str> {
    BUILTIN_SKILL_ENTRIES.iter().find_map(|(name, content)| {
        let expected = PathBuf::from(format!("<built-in>/{}", name)).join(SKILL_FILE_NAME);
        (expected == skill_file_path).then_some(*content)
    })
}
