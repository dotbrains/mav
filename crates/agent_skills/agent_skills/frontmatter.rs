use super::*;

pub fn parse_skill_frontmatter(
    skill_file_path: &Path,
    content: &str,
    source: SkillSource,
) -> Result<Skill> {
    let (metadata, _body, load_warnings) = parse_skill_file_content_for_loading(content)?;

    let directory_path = skill_file_path
        .parent()
        .context("SKILL.md file has no parent directory")?
        .to_path_buf();

    Ok(Skill {
        name: metadata.name,
        description: metadata.description,
        source,
        directory_path,
        skill_file_path: skill_file_path.to_path_buf(),
        load_warnings,
        disable_model_invocation: metadata.disable_model_invocation,
        embedded_body: None,
    })
}

/// Extract the YAML frontmatter and body from a SKILL.md file without
/// validating the metadata fields.
pub fn extract_skill_frontmatter(content: &str) -> Result<(SkillMetadata, &str)> {
    if content.len() > MAX_SKILL_FILE_SIZE {
        anyhow::bail!(
            "SKILL.md file exceeds maximum size of {}KB",
            MAX_SKILL_FILE_SIZE / 1024
        );
    }

    extract_frontmatter(content)
}

/// Parse and validate the YAML frontmatter and body from a SKILL.md file.
pub fn parse_skill_file_content(content: &str) -> Result<(SkillMetadata, &str)> {
    let (metadata, body) = extract_skill_frontmatter(content)?;

    validate_name(&metadata.name).map_err(anyhow::Error::msg)?;
    validate_description(&metadata.description).map_err(anyhow::Error::msg)?;

    Ok((metadata, body))
}

pub(super) fn parse_skill_file_content_for_loading(
    content: &str,
) -> Result<(SkillMetadata, &str, Vec<SkillLoadWarning>)> {
    let (metadata, body) = extract_skill_frontmatter(content)?;

    validate_name(&metadata.name).map_err(anyhow::Error::msg)?;
    let load_warnings =
        validate_description_for_loading(&metadata.description).map_err(anyhow::Error::msg)?;

    Ok((metadata, body, load_warnings))
}

fn validate_description_for_loading(
    description: &str,
) -> Result<Vec<SkillLoadWarning>, &'static str> {
    if description.trim().is_empty() {
        return Err("Skill description cannot be empty");
    }

    let mut warnings = Vec::new();
    if description.len() > MAX_SKILL_DESCRIPTION_LEN {
        warnings.push(SkillLoadWarning::DescriptionTooLong {
            actual_len: description.len(),
            max_len: MAX_SKILL_DESCRIPTION_LEN,
        });
    }

    Ok(warnings)
}

pub(super) fn extract_frontmatter(content: &str) -> Result<(SkillMetadata, &str)> {
    let content = content.trim_start();

    if !content.starts_with("---") {
        anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
    }

    // Find every candidate closing `---` line: a line consisting EXACTLY of
    // `---` (followed by `\n`, `\r\n`, or EOF) at column 0, excluding the
    // opening line itself. The opener occupies bytes 0..(first line ending),
    // and our scan starts after each `\n`, so the opener is naturally skipped.
    //
    // For each candidate we record the byte position right after its line
    // ending; that's both where the YAML stream slice ends and where the body
    // begins.
    let bytes = content.as_bytes();
    let mut candidates: Vec<usize> = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'\n' {
            continue;
        }
        let line_start = i + 1;
        if line_start + 3 > bytes.len() {
            continue;
        }
        if &bytes[line_start..line_start + 3] != b"---" {
            continue;
        }
        let after_dashes = line_start + 3;
        let end = if after_dashes == bytes.len() {
            after_dashes
        } else if bytes[after_dashes] == b'\n' {
            after_dashes + 1
        } else if after_dashes + 1 < bytes.len()
            && bytes[after_dashes] == b'\r'
            && bytes[after_dashes + 1] == b'\n'
        {
            after_dashes + 2
        } else {
            // Line is something like `---trailing` or `----`; not a candidate.
            continue;
        };
        candidates.push(end);
    }

    if candidates.is_empty() {
        anyhow::bail!("SKILL.md missing closing frontmatter delimiter (---)");
    }

    // Try each candidate in order: slice content up through the candidate's
    // terminator and ask `serde_yaml_ng` to parse it as a YAML stream. If the
    // first document deserializes into `SkillMetadata`, that candidate is the
    // real closer. Otherwise an earlier candidate may have cut the YAML in the
    // middle of a scalar / quoted string; try the next one.
    let mut last_error: Option<anyhow::Error> = None;
    for end in candidates {
        let prefix = &content[..end];
        let mut docs = serde_yaml_ng::Deserializer::from_str(prefix);
        let Some(first_doc) = docs.next() else {
            continue;
        };
        match SkillMetadata::deserialize(first_doc) {
            Ok(metadata) => return Ok((metadata, &content[end..])),
            Err(e) => last_error = Some(anyhow::Error::new(e)),
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("could not parse YAML frontmatter"))
        .context("Invalid YAML frontmatter"))
}
