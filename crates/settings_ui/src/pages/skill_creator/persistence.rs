use super::*;

/// Serialize the SKILL.md file to disk at `<skills_dir>/<name>/SKILL.md`.
///
/// Refuses to overwrite an existing directory at `<skills_dir>/<name>`. The
/// caller surfaces the resulting error to the user, who picks a different
/// name.
async fn write_skill_to_disk(
    fs: &dyn Fs,
    skills_dir: &std::path::Path,
    name: &str,
    description: &str,
    body: &str,
    disable_model_invocation: bool,
) -> Result<PathBuf> {
    let skill_dir = skills_dir.join(name);
    match fs.metadata(&skill_dir).await {
        Ok(Some(metadata)) if metadata.is_dir => {
            anyhow::bail!(
                "A skill named \"{name}\" already exists at {}. Pick a different name.",
                skill_dir.display()
            );
        }
        Ok(Some(_)) => {
            // Something exists at this path, but it isn't a directory — e.g.
            // a stray file the user (or another tool) left there. Without
            // this branch we'd fall through to `create_dir`, which on the
            // real fs returns a generic "File exists" IO error that gives
            // the user no idea what's wrong or how to recover.
            anyhow::bail!(
                "A file (not a skill directory) already exists at {}. \
                 Delete it or pick a different skill name.",
                skill_dir.display()
            );
        }
        Ok(None) => {}
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to check whether {} already exists",
                    skill_dir.display()
                )
            });
        }
    }

    let content = format_skill_file(name, description, body, disable_model_invocation)?;

    fs.create_dir(&skill_dir)
        .await
        .with_context(|| format!("failed to create skill directory {}", skill_dir.display()))?;
    let skill_file_path = skill_dir.join(SKILL_FILE_NAME);
    fs.write(&skill_file_path, content.as_bytes())
        .await
        .with_context(|| format!("failed to write {}", skill_file_path.display()))?;

    Ok(skill_file_path)
}

fn format_skill_file(
    name: &str,
    description: &str,
    body: &str,
    disable_model_invocation: bool,
) -> Result<String> {
    let metadata = SkillMetadata {
        name: name.to_string(),
        description: description.to_string(),
        disable_model_invocation,
    };
    let frontmatter = serde_yaml_ng::to_string(&metadata)
        .context("failed to serialize skill frontmatter as YAML")?;

    let mut content = String::with_capacity(frontmatter.len() + body.len() + 16);
    content.push_str("---\n");
    content.push_str(&frontmatter);
    content.push_str("---\n");
    let trimmed_body = body.trim();
    if !trimmed_body.is_empty() {
        content.push('\n');
        content.push_str(trimmed_body);
        content.push('\n');
    }
    Ok(content)
}
