use super::frontmatter::parse_skill_file_content_for_loading;
use super::*;

pub async fn load_skills_from_directory(
    fs: &Arc<dyn Fs>,
    directory: &Path,
    source: SkillSource,
) -> Vec<Result<Skill, SkillLoadError>> {
    if !fs.is_dir(directory).await {
        return Vec::new();
    }

    let skill_files = find_skill_files(fs, directory).await;

    let mut results: Vec<Result<Skill, SkillLoadError>> = futures::stream::iter(skill_files)
        .map(|path| {
            let fs = fs.clone();
            let source = source.clone();
            async move { load_skill_frontmatter(fs, path, source).await }
        })
        .buffer_unordered(SKILL_IO_CONCURRENCY)
        .collect()
        .await;

    // Sort by path so name-conflict resolution in `apply_skill_overrides`
    // is deterministic — `fs.read_dir` order is filesystem-dependent.
    results.sort_by(|a, b| {
        let path_a: &Path = match a {
            Ok(skill) => &skill.skill_file_path,
            Err(error) => &error.path,
        };
        let path_b: &Path = match b {
            Ok(skill) => &skill.skill_file_path,
            Err(error) => &error.path,
        };
        path_a.cmp(path_b)
    });

    results
}

/// Find every `<skills_root>/<name>/SKILL.md` directly under `directory`.
///
/// Discovery is intentionally one level deep: a skill is the immediate
/// child directory of the skills root, and `SKILL.md` is the file that
/// names it. See `crates/agent_skills/README.md` for why we don't recurse.
async fn find_skill_files(fs: &Arc<dyn Fs>, directory: &Path) -> Vec<PathBuf> {
    let Ok(mut entries) = fs.read_dir(directory).await else {
        return Vec::new();
    };

    let mut entry_paths = Vec::new();
    while let Some(entry) = entries.next().await {
        if let Ok(entry_path) = entry {
            entry_paths.push(entry_path);
        }
    }

    futures::stream::iter(entry_paths)
        .map(|entry_path| {
            let fs = fs.clone();
            async move {
                let Ok(Some(metadata)) = fs.metadata(&entry_path).await else {
                    return None;
                };
                if !metadata.is_dir {
                    return None;
                }
                let skill_file = entry_path.join(SKILL_FILE_NAME);
                fs.is_file(&skill_file).await.then_some(skill_file)
            }
        })
        .buffer_unordered(SKILL_IO_CONCURRENCY)
        .filter_map(|x| async move { x })
        .collect()
        .await
}

/// Read `skill_file_path` from disk and parse its frontmatter. The
/// SKILL.md body is parsed away by `parse_skill_frontmatter` and not
/// surfaced here; it's re-read on demand via `read_skill_body` when a
/// skill is actually being loaded for the model.
///
/// We load the whole file in one go rather than streaming up to the
/// closing `---`. `MAX_SKILL_FILE_SIZE` is 100KB and the metadata check
/// below caps the worst case at that, so the peak transient cost is
/// trivially small (≤ `MAX_SKILL_FILE_SIZE` × `SKILL_IO_CONCURRENCY`).
pub async fn load_skill_frontmatter(
    fs: Arc<dyn Fs>,
    skill_file_path: PathBuf,
    source: SkillSource,
) -> Result<Skill, SkillLoadError> {
    // Short-circuit on oversized files before reading any of their
    // contents, so a stray multi-GB file named `SKILL.md` can't OOM the
    // app. If metadata is unavailable, refuse to read.
    let metadata = fs
        .metadata(&skill_file_path)
        .await
        .map_err(|e| SkillLoadError {
            path: skill_file_path.clone(),
            message: format!("Failed to read SKILL.md metadata: {}", e),
        })?;
    if let Some(metadata) = metadata
        && metadata.len > MAX_SKILL_FILE_SIZE as u64
    {
        return Err(SkillLoadError {
            path: skill_file_path.clone(),
            message: format!(
                "SKILL.md file exceeds maximum size of {}KB",
                MAX_SKILL_FILE_SIZE / 1024
            ),
        });
    }

    let content = fs
        .load(&skill_file_path)
        .await
        .map_err(|e| SkillLoadError {
            path: skill_file_path.clone(),
            message: format!("Failed to read file: {}", e),
        })?;

    parse_skill_frontmatter(&skill_file_path, &content, source).map_err(|e| SkillLoadError {
        path: skill_file_path.clone(),
        message: e.to_string(),
    })
}

/// Read the body of a SKILL.md from disk — everything after the closing
/// `---`. Called only when a skill is being materialized for the model
/// (via `SkillTool` or a slash invocation). The body is intentionally
/// NOT kept in memory between materializations.
pub async fn read_skill_body(
    fs: &dyn Fs,
    skill_file_path: &Path,
) -> Result<String, SkillLoadError> {
    let content = fs.load(skill_file_path).await.map_err(|e| SkillLoadError {
        path: skill_file_path.to_path_buf(),
        message: format!("Failed to read file: {}", e),
    })?;

    read_skill_body_from_content(skill_file_path, &content)
}

pub fn read_skill_body_from_content(
    skill_file_path: &Path,
    content: &str,
) -> Result<String, SkillLoadError> {
    let (_metadata, body, _load_warnings) =
        parse_skill_file_content_for_loading(content).map_err(|e| SkillLoadError {
            path: skill_file_path.to_path_buf(),
            message: e.to_string(),
        })?;

    Ok(body.trim().to_string())
}
