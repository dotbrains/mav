use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkillLoadingIssueKind {
    LoadFailed,
    DescriptionTooLong,
    CatalogBudgetExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SkillLoadingIssue {
    pub project_id: EntityId,
    pub path: PathBuf,
    pub message: SharedString,
    pub kind: SkillLoadingIssueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SkillLoadingIssueData {
    pub(super) path: PathBuf,
    pub(super) message: String,
    pub(super) kind: SkillLoadingIssueKind,
}

impl SkillLoadingIssueData {
    pub(super) fn from_load_error(error: SkillLoadError) -> Self {
        Self {
            path: error.path,
            message: error.message,
            kind: SkillLoadingIssueKind::LoadFailed,
        }
    }

    pub(super) fn from_load_warning(skill: &Skill, warning: &SkillLoadWarning) -> Self {
        let kind = match warning {
            SkillLoadWarning::DescriptionTooLong { .. } => {
                SkillLoadingIssueKind::DescriptionTooLong
            }
        };
        Self {
            path: skill.skill_file_path.clone(),
            message: warning.message(),
            kind,
        }
    }

    pub(super) fn catalog_budget_exceeded(path: PathBuf, message: String) -> Self {
        Self {
            path,
            message,
            kind: SkillLoadingIssueKind::CatalogBudgetExceeded,
        }
    }
}

/// Emitted whenever the set of skill loading issues for a project changes.
/// The `issues` field is the full replacement list; subscribers should treat
/// it as a snapshot rather than appending. An empty `issues` list means all
/// previously-reported issues have been resolved.
#[derive(Clone, Debug)]
pub struct SkillLoadingIssuesUpdated {
    pub project_id: EntityId,
    pub issues: Vec<SkillLoadingIssue>,
}

#[derive(Clone, Debug)]
pub struct NativeAvailableSkill {
    pub name: String,
    pub description: String,
    pub source: SharedString,
    pub skill_file_path: PathBuf,
    pub warning: Option<SharedString>,
}

impl From<&Skill> for NativeAvailableSkill {
    fn from(skill: &Skill) -> Self {
        Self {
            name: skill.name.clone(),
            description: skill.description.clone(),
            source: skill.source.display_label().to_string().into(),
            skill_file_path: skill.skill_file_path.clone(),
            warning: skill
                .load_warnings
                .first()
                .map(|warning| warning.message().into()),
        }
    }
}

pub(super) struct ProjectSkillFile {
    pub(super) relative_path: Arc<RelPath>,
    pub(super) display_path: PathBuf,
    pub(super) size: u64,
}

async fn expand_worktree_directory(
    worktree: &Entity<Worktree>,
    path: &RelPath,
    cx: &mut AsyncApp,
) -> Result<()> {
    let expand_task = worktree.update(cx, |worktree, cx| {
        let entry_id = worktree
            .entry_for_path(path)
            .filter(|entry| entry.is_dir())
            .map(|entry| entry.id);
        entry_id.and_then(|entry_id| worktree.expand_entry(entry_id, cx))
    });

    if let Some(expand_task) = expand_task {
        expand_task.await?;
    }

    Ok(())
}

pub(super) async fn expand_project_skills_directories(
    worktree: &Entity<Worktree>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let agents_dir = RelPath::unix(AGENTS_DIR_NAME)?;
    let Some(skills_prefix) = SKILLS_PREFIX.as_ref() else {
        return Ok(());
    };

    expand_worktree_directory(worktree, agents_dir, cx).await?;
    expand_worktree_directory(worktree, skills_prefix, cx).await?;

    let skill_dirs = worktree.update(cx, |worktree, _cx| {
        worktree
            .child_entries(skills_prefix)
            .filter(|entry| entry.is_dir())
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>()
    });
    for skill_dir in skill_dirs {
        expand_worktree_directory(worktree, &skill_dir, cx).await?;
    }

    Ok(())
}

pub(super) fn project_skill_files_from_worktree(worktree: &Worktree) -> Vec<ProjectSkillFile> {
    let Some(skills_prefix) = SKILLS_PREFIX.as_ref() else {
        return Vec::new();
    };
    let Ok(skill_file_name) = RelPath::unix(SKILL_FILE_NAME) else {
        return Vec::new();
    };

    let mut skill_files = Vec::new();
    for skill_dir in worktree.child_entries(skills_prefix) {
        if !skill_dir.is_dir() {
            continue;
        }

        let relative_path = skill_dir.path.join(skill_file_name);
        let Some(skill_file) = worktree.entry_for_path(&relative_path) else {
            continue;
        };
        if !skill_file.is_file() {
            continue;
        }

        skill_files.push(ProjectSkillFile {
            display_path: worktree.absolutize(&relative_path),
            relative_path,
            size: skill_file.size,
        });
    }

    skill_files.sort_by(|a, b| {
        a.relative_path
            .as_unix_str()
            .cmp(b.relative_path.as_unix_str())
    });
    skill_files
}

/// Build the catalog the model sees in its system prompt: filter out hidden
/// (`disable_model_invocation`) skills, then drop the rest if they would push
/// the catalog past the description budget.
///
/// Returns `SkillSummary` values rather than full `Skill`s so that the
/// (potentially ~100KB) skill bodies aren't cloned just to be discarded by
/// `ProjectContext::new`, which only needs the summary fields.
pub(super) fn select_catalog_skills(
    skills: &[Skill],
) -> (Vec<SkillSummary>, Vec<SkillLoadingIssueData>) {
    let mut kept = Vec::new();
    let mut issues = Vec::new();
    let mut dropped: Vec<&Skill> = Vec::new();
    let mut total_size = 0usize;
    let mut budget_exceeded = false;

    for skill in skills {
        if skill.disable_model_invocation {
            continue;
        }

        let entry_size = skill.name.len() + skill.description.len();
        if !budget_exceeded && total_size.saturating_add(entry_size) <= MAX_SKILL_DESCRIPTIONS_SIZE
        {
            total_size += entry_size;
            kept.push(SkillSummary::from(skill));
        } else {
            // Once any model-invocable skill overflows the budget, stop
            // packing entirely so the cutoff is deterministic by sort order
            // rather than dependent on which skills happen to be small
            // enough to fit in the remaining space.
            budget_exceeded = true;
            dropped.push(skill);
        }
    }

    if !dropped.is_empty() {
        let budget_kb = MAX_SKILL_DESCRIPTIONS_SIZE / 1024;
        let first = dropped[0];
        let message = if dropped.len() == 1 {
            let entry_size = first.name.len() + first.description.len();
            format!(
                "Skill '{}' ({:.1}KB description) was dropped from the catalog because the previous skills already used the entire {}KB description budget.",
                first.name,
                entry_size as f64 / 1024.0,
                budget_kb,
            )
        } else {
            let mut message = format!(
                "{} skills were dropped from the catalog because they exceeded the {}KB description budget:",
                dropped.len(),
                budget_kb,
            );
            for skill in &dropped {
                let entry_size = skill.name.len() + skill.description.len();
                message.push('\n');
                message.push_str(&format!(
                    "- {} ({:.1}KB description)",
                    skill.name,
                    entry_size as f64 / 1024.0,
                ));
            }
            message
        };
        issues.push(SkillLoadingIssueData::catalog_budget_exceeded(
            first.skill_file_path.clone(),
            message,
        ));
    }

    (kept, issues)
}

/// Build a closure that, when called, reads the latest `state.skills`
/// for the given project from the `NativeAgent` and applies
/// project-overrides-global so the `SkillTool` resolves a name to the
/// same entry the model sees in its catalog. Run at invocation time
/// (not thread-build time) so skill changes after thread construction
/// become visible without re-registering the tool.
pub fn skills_resolver_for_project(
    weak_agent: WeakEntity<NativeAgent>,
    project_id: EntityId,
) -> impl Fn(&App) -> Arc<Vec<Skill>> + Send + Sync + 'static {
    move |cx: &App| {
        weak_agent
            .upgrade()
            .and_then(|agent| {
                agent
                    .read(cx)
                    .projects
                    .get(&project_id)
                    .map(|state| Arc::new(apply_skill_overrides(&state.skills)))
            })
            .unwrap_or_else(|| Arc::new(Vec::new()))
    }
}

pub fn skill_body_resolver_for_project(
    project: Entity<Project>,
    fs: Arc<dyn Fs>,
) -> impl Fn(Skill, &mut AsyncApp) -> Task<Result<String>> + Send + Sync + 'static {
    move |skill, cx| match skill.source.clone() {
        SkillSource::ProjectLocal { worktree_id, .. } => {
            let project = project.clone();
            cx.spawn(async move |cx| {
                let worktree_id = WorktreeId::from_usize(worktree_id.0);
                let worktree = project
                    .update(cx, |project, cx| project.worktree_for_id(worktree_id, cx))
                    .context("no such worktree")?;
                expand_project_skills_directories(&worktree, cx).await?;
                let relative_path = worktree.update(cx, |worktree, _cx| {
                    let worktree_root = worktree.abs_path();
                    worktree
                        .path_style()
                        .strip_prefix(&skill.skill_file_path, &worktree_root)
                        .map(|relative_path| relative_path.into_arc())
                        .context("skill file is not inside its worktree")
                })?;

                let buffer = project
                    .update(cx, |project, cx| {
                        project.open_buffer((worktree_id, relative_path), cx)
                    })
                    .await?;
                let content =
                    cx.update(|cx| buffer.read(cx).as_text_snapshot().as_rope().to_string());

                read_skill_body_from_content(&skill.skill_file_path, &content).map_err(Into::into)
            })
        }
        SkillSource::BuiltIn | SkillSource::Global => {
            let fs = fs.clone();
            cx.background_spawn(async move {
                agent_skills::read_skill_body(fs.as_ref(), &skill.skill_file_path)
                    .await
                    .map_err(Into::into)
            })
        }
    }
}

/// Collect successfully-loaded global and project-local skills into a
/// single list, preserving every entry — even when two skills share a
/// name. The autocomplete popup shows the full list with origin labels
/// so users can tell same-named skills apart; override resolution
/// (project-local wins over global) happens later via
/// [`apply_skill_overrides`] at the boundaries where the model
/// interacts with skills (system-prompt catalog, `SkillTool` lookup,
/// slash-command invocation).
///
/// Global versions of skills will be before the local versions
pub(super) fn combine_skills(
    global: Vec<Result<Skill, SkillLoadError>>,
    project: impl Iterator<Item = Result<Skill, SkillLoadError>>,
) -> (Vec<Skill>, Vec<SkillLoadError>) {
    // Built-in skills go first (lowest priority) so that global and
    // project-local skills with the same name shadow them.
    let mut skills = builtin_skills();
    let mut errors = Vec::new();
    for result in global.into_iter().chain(project) {
        match result {
            Ok(skill) => skills.push(skill),
            Err(e) => errors.push(e),
        }
    }
    log_skill_conflicts(&skills);
    (skills, errors)
}

/// Emit a warning for each name collision between skills. Called once
/// per skill load (not per query), so the log isn't spammed by repeated
/// catalog rebuilds.
fn log_skill_conflicts(skills: &[Skill]) {
    let mut by_name: HashMap<&str, &Skill> = HashMap::default();
    for skill in skills {
        match by_name.get(skill.name.as_str()) {
            Some(existing) => {
                if skill.source.precedence() > existing.source.precedence() {
                    log::warn!(
                        "Skill '{}' at '{}' overrides skill at '{}' for the model; both appear in the slash-command popup with their source",
                        skill.name,
                        skill.skill_file_path.display(),
                        existing.skill_file_path.display(),
                    );
                    by_name.insert(skill.name.as_str(), skill);
                } else {
                    log::warn!(
                        "Skill '{}' at '{}' conflicts with skill at '{}'; the model will see the first one, but both appear in the slash-command popup with their source",
                        skill.name,
                        skill.skill_file_path.display(),
                        existing.skill_file_path.display(),
                    );
                }
            }
            None => {
                by_name.insert(skill.name.as_str(), skill);
            }
        }
    }
}

/// Project-local skills override same-named global skills. Returns a
/// new list with at most one entry per name. Two skills of the same
/// source colliding (e.g. two globals or two project-locals) keep the
/// first one to match the historical behavior.
///
/// This is the projection of `state.skills` used by everything the
/// model interacts with: the system-prompt catalog, the `SkillTool`'s
/// name resolver, and slash-command invocation. The autocomplete popup
/// deliberately does *not* go through this — it shows the full list so
/// users can see what's shadowed.
pub(super) fn apply_skill_overrides(skills: &[Skill]) -> Vec<Skill> {
    let mut result: Vec<Skill> = Vec::new();
    // Borrow names from the input slice so the dedup index doesn't
    // need to allocate a `String` per skill. The borrow is valid for
    // the body of the function because `skills` outlives `indices`.
    let mut indices: HashMap<&str, usize> = HashMap::default();
    for skill in skills {
        match indices.get(skill.name.as_str()).copied() {
            Some(idx) => {
                if skill.source.precedence() > result[idx].source.precedence() {
                    result[idx] = skill.clone();
                }
            }
            None => {
                indices.insert(skill.name.as_str(), result.len());
                result.push(skill.clone());
            }
        }
    }
    result
}
