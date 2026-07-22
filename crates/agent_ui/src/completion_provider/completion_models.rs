use super::*;

pub(crate) enum Match {
    File(FileMatch),
    Symbol(SymbolMatch),
    Thread(SessionMatch),
    RecentThread(SessionMatch),
    Fetch(SharedString),
    Skill(AvailableSkill),
    Entry(EntryMatch),
    BranchDiff(BranchDiffMatch),
}

#[derive(Debug, Clone)]
pub struct BranchDiffMatch {
    pub base_ref: SharedString,
}

impl Match {
    pub fn score(&self) -> f64 {
        match self {
            Match::File(file) => file.mat.score,
            Match::Entry(mode) => mode.mat.as_ref().map(|mat| mat.score).unwrap_or(1.),
            Match::Thread(_) => 1.,
            Match::RecentThread(_) => 1.,
            Match::Symbol(_) => 1.,
            Match::Skill(_) => 1.,
            Match::Fetch(_) => 1.,
            Match::BranchDiff(_) => 1.,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SessionMatch {
    session_id: acp::SessionId,
    title: SharedString,
}

pub(super) struct EntryMatch {
    mat: Option<StringMatch>,
    entry: PromptContextEntry,
}

pub(super) fn session_title(title: Option<SharedString>) -> SharedString {
    title
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| SharedString::new_static(DEFAULT_THREAD_TITLE))
}

#[derive(Debug, Clone)]
pub struct AvailableSkill {
    pub name: Arc<str>,
    pub description: Arc<str>,
    /// Scope prefix for this skill: empty for global skills, or the
    /// worktree root name for project-local skills.
    pub source: SharedString,
    pub skill_file_path: PathBuf,
    pub warning: Option<SharedString>,
}

pub(super) fn skill_completion_icon_path(
    skill: &AvailableSkill,
    uri: &MentionUri,
    cx: &mut App,
) -> SharedString {
    if skill.warning.is_some() {
        IconName::Warning.path().into()
    } else {
        uri.icon_path(cx)
    }
}

pub(super) fn skill_completion_icon_color(skill: &AvailableSkill, cx: &App) -> Option<Hsla> {
    skill.warning.is_some().then(|| cx.theme().status().warning)
}

pub(super) fn skill_completion_documentation(skill: &AvailableSkill) -> CompletionDocumentation {
    let text = match &skill.warning {
        Some(warning) => warning.clone(),
        None => skill.description.to_string().into(),
    };
    CompletionDocumentation::MultiLinePlainText(text)
}

#[derive(Debug, Clone)]
pub struct AvailableCommand {
    pub name: Arc<str>,
    pub description: Arc<str>,
    pub requires_argument: bool,
    pub source: Option<SharedString>,
    /// Source category used to group the command in the slash popup. `None`
    /// means the command came from an external ACP agent.
    pub category: Option<acp_thread::CommandCategory>,
}

impl AvailableCommand {
    fn category_order(&self) -> u8 {
        match self.category {
            Some(acp_thread::CommandCategory::Native) => 0,
            Some(acp_thread::CommandCategory::Mcp) => 1,
            None => 2,
        }
    }

    /// Completion group key and header label for this command's category.
    fn group(&self) -> CompletionGroup {
        let (key, label) = match self.category {
            Some(acp_thread::CommandCategory::Native) => ("commands", "Commands"),
            Some(acp_thread::CommandCategory::Mcp) => ("mcp-commands", "MCP Server Commands"),
            None => ("acp-commands", "Commands"),
        };
        CompletionGroup {
            key: key.into(),
            label: Some(label.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum SlashCompletionCandidate {
    Skill(AvailableSkill),
    Command(AvailableCommand),
}

impl SlashCompletionCandidate {
    fn name(&self) -> &Arc<str> {
        match self {
            Self::Skill(skill) => &skill.name,
            Self::Command(command) => &command.name,
        }
    }
}

/// Stable group identity for a slash completion: skills are one group, commands
/// are grouped by category. This identifies which section header an entry sits
/// under; the order the groups appear in is decided by relevance (see
/// [`group_by_relevance`]).
pub(super) fn slash_completion_group_key(candidate: &SlashCompletionCandidate) -> u32 {
    match candidate {
        SlashCompletionCandidate::Skill(_) => 0,
        SlashCompletionCandidate::Command(command) => 1 + command.category_order() as u32,
    }
}

/// Reorders `items` (which must already be in relevance/score order, best
/// first) so that each group's entries stay contiguous while the groups
/// themselves are ordered by their best-ranked member. The sort is stable, so
/// within a group the original order is preserved.
pub(super) fn group_by_relevance<T>(items: &mut [T], group_key: impl Fn(&T) -> u32) {
    let mut group_best_rank: collections::HashMap<u32, usize> = collections::HashMap::default();
    for (rank, item) in items.iter().enumerate() {
        group_best_rank.entry(group_key(item)).or_insert(rank);
    }
    items.sort_by_key(|item| group_best_rank[&group_key(item)]);
}
