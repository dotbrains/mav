use super::*;

pub(crate) fn search_files(
    query: String,
    cancellation_flag: Arc<AtomicBool>,
    workspace: &Entity<Workspace>,
    cx: &App,
) -> Task<Vec<FileMatch>> {
    if query.is_empty() {
        let workspace = workspace.read(cx);
        let project = workspace.project().read(cx);
        let visible_worktrees = workspace.visible_worktrees(cx).collect::<Vec<_>>();
        let include_root_name = visible_worktrees.len() > 1;

        let recent_matches = workspace
            .recent_navigation_history(Some(10), cx)
            .into_iter()
            .map(|(project_path, _)| {
                let path_prefix = if include_root_name {
                    project
                        .worktree_for_id(project_path.worktree_id, cx)
                        .map(|wt| wt.read(cx).root_name().into())
                        .unwrap_or_else(|| RelPath::empty_arc())
                } else {
                    RelPath::empty_arc()
                };

                FileMatch {
                    mat: PathMatch {
                        score: 0.,
                        positions: Vec::new(),
                        worktree_id: project_path.worktree_id.to_usize(),
                        path: project_path.path,
                        path_prefix,
                        distance_to_relative_ancestor: 0,
                        is_dir: false,
                    },
                    is_recent: true,
                }
            });

        let file_matches = visible_worktrees.into_iter().flat_map(|worktree| {
            let worktree = worktree.read(cx);
            let path_prefix: Arc<RelPath> = if include_root_name {
                worktree.root_name().into()
            } else {
                RelPath::empty_arc()
            };
            worktree.entries(false, 0).map(move |entry| FileMatch {
                mat: PathMatch {
                    score: 0.,
                    positions: Vec::new(),
                    worktree_id: worktree.id().to_usize(),
                    path: entry.path.clone(),
                    path_prefix: path_prefix.clone(),
                    distance_to_relative_ancestor: 0,
                    is_dir: entry.is_dir(),
                },
                is_recent: false,
            })
        });

        Task::ready(recent_matches.chain(file_matches).collect())
    } else {
        let workspace = workspace.read(cx);
        let relative_to = workspace
            .recent_navigation_history_iter(cx)
            .next()
            .map(|(path, _)| path.path);
        let worktrees = workspace.visible_worktrees(cx).collect::<Vec<_>>();
        let include_root_name = worktrees.len() > 1;
        let candidate_sets = worktrees
            .into_iter()
            .map(|worktree| {
                let worktree = worktree.read(cx);

                PathMatchCandidateSet {
                    snapshot: worktree.snapshot(),
                    include_ignored: worktree.root_entry().is_some_and(|entry| entry.is_ignored),
                    include_root_name,
                    candidates: project::Candidates::Entries,
                }
            })
            .collect::<Vec<_>>();

        let executor = cx.background_executor().clone();
        cx.foreground_executor().spawn(async move {
            fuzzy::match_path_sets(
                candidate_sets.as_slice(),
                query.as_str(),
                &relative_to,
                false,
                100,
                &cancellation_flag,
                executor,
            )
            .await
            .into_iter()
            .map(|mat| FileMatch {
                mat,
                is_recent: false,
            })
            .collect::<Vec<_>>()
        })
    }
}

pub(crate) fn search_symbols(
    query: String,
    cancellation_flag: Arc<AtomicBool>,
    workspace: &Entity<Workspace>,
    cx: &mut App,
) -> Task<Vec<SymbolMatch>> {
    let symbols_task = workspace.update(cx, |workspace, cx| {
        workspace
            .project()
            .update(cx, |project, cx| project.symbols(&query, cx))
    });
    let project = workspace.read(cx).project().clone();
    cx.spawn(async move |cx| {
        let Some(symbols) = symbols_task.await.log_err() else {
            return Vec::new();
        };
        let (visible_match_candidates, external_match_candidates): (Vec<_>, Vec<_>) = project
            .update(cx, |project, cx| {
                symbols
                    .iter()
                    .enumerate()
                    .map(|(id, symbol)| StringMatchCandidate::new(id, symbol.label.filter_text()))
                    .partition(|candidate| match &symbols[candidate.id].path {
                        SymbolLocation::InProject(project_path) => project
                            .entry_for_path(project_path, cx)
                            .is_some_and(|e| !e.is_ignored),
                        SymbolLocation::OutsideProject { .. } => false,
                    })
            });
        // Try to support rust-analyzer's path based symbols feature which
        // allows to search by rust path syntax, in that case we only want to
        // filter names by the last segment
        // Ideally this was a first class LSP feature (rich queries)
        let query = query
            .rsplit_once("::")
            .map_or(&*query, |(_, suffix)| suffix)
            .to_owned();
        // Note if you make changes to this filtering below, also change `project_symbols::ProjectSymbolsDelegate::filter`
        const MAX_MATCHES: usize = 100;
        let mut visible_matches = cx.foreground_executor().block_on(fuzzy::match_strings(
            &visible_match_candidates,
            &query,
            false,
            true,
            MAX_MATCHES,
            &cancellation_flag,
            cx.background_executor().clone(),
        ));
        let mut external_matches = cx.foreground_executor().block_on(fuzzy::match_strings(
            &external_match_candidates,
            &query,
            false,
            true,
            MAX_MATCHES - visible_matches.len().min(MAX_MATCHES),
            &cancellation_flag,
            cx.background_executor().clone(),
        ));
        let sort_key_for_match = |mat: &StringMatch| {
            let symbol = &symbols[mat.candidate_id];
            (Reverse(OrderedFloat(mat.score)), symbol.label.filter_text())
        };

        visible_matches.sort_unstable_by_key(sort_key_for_match);
        external_matches.sort_unstable_by_key(sort_key_for_match);
        let mut matches = visible_matches;
        matches.append(&mut external_matches);

        matches
            .into_iter()
            .map(|mut mat| {
                let symbol = symbols[mat.candidate_id].clone();
                let filter_start = symbol.label.filter_range.start;
                for position in &mut mat.positions {
                    *position += filter_start;
                }
                SymbolMatch { symbol }
            })
            .collect()
    })
}

pub(super) fn collect_session_matches(cx: &App) -> Vec<SessionMatch> {
    let Some(store) = ThreadMetadataStore::try_global(cx) else {
        return Vec::new();
    };
    let mut entries: Vec<&ThreadMetadata> = store
        .read(cx)
        .entries()
        .filter(|t| !t.archived && t.agent_id == *agent::MAV_AGENT_ID)
        .collect();
    entries.sort_by_key(|t| Reverse(t.updated_at));
    entries
        .into_iter()
        .map(|metadata| {
            let info = acp_thread::AgentSessionInfo::from(metadata);
            SessionMatch {
                session_id: info.session_id,
                title: session_title(info.title),
            }
        })
        .collect()
}

pub(super) fn filter_sessions_by_query(
    query: String,
    cancellation_flag: Arc<AtomicBool>,
    sessions: Vec<SessionMatch>,
    cx: &mut App,
) -> Task<Vec<SessionMatch>> {
    if query.is_empty() {
        return Task::ready(sessions);
    }
    let executor = cx.background_executor().clone();
    cx.background_spawn(async move {
        filter_sessions(query, cancellation_flag, sessions, executor).await
    })
}

async fn filter_sessions(
    query: String,
    cancellation_flag: Arc<AtomicBool>,
    sessions: Vec<SessionMatch>,
    executor: BackgroundExecutor,
) -> Vec<SessionMatch> {
    let titles = sessions
        .iter()
        .map(|session| session.title.clone())
        .collect::<Vec<_>>();
    let candidates = titles
        .iter()
        .enumerate()
        .map(|(id, title)| StringMatchCandidate::new(id, title.as_ref()))
        .collect::<Vec<_>>();
    let matches = fuzzy::match_strings(
        &candidates,
        &query,
        false,
        true,
        100,
        &cancellation_flag,
        executor,
    )
    .await;

    matches
        .into_iter()
        .map(|mat| sessions[mat.candidate_id].clone())
        .collect()
}

pub(crate) fn search_skills(
    query: String,
    cancellation_flag: Arc<AtomicBool>,
    skills: Vec<AvailableSkill>,
    cx: &mut App,
) -> Task<Vec<AvailableSkill>> {
    if skills.is_empty() {
        return Task::ready(Vec::new());
    }
    let executor = cx.background_executor().clone();
    cx.background_spawn(async move {
        let candidates = skills
            .iter()
            .enumerate()
            .map(|(id, skill)| StringMatchCandidate::new(id, &skill.name))
            .collect::<Vec<_>>();
        let matches = fuzzy::match_strings(
            &candidates,
            &query,
            false,
            true,
            100,
            &cancellation_flag,
            executor,
        )
        .await;
        matches
            .into_iter()
            .map(|mat| skills[mat.candidate_id].clone())
            .collect()
    })
}

pub struct SymbolMatch {
    pub symbol: Symbol,
}

pub struct FileMatch {
    pub mat: PathMatch,
    pub is_recent: bool,
}

pub fn extract_file_name_and_directory(
    path: &RelPath,
    path_prefix: &RelPath,
    path_style: PathStyle,
) -> (SharedString, Option<SharedString>) {
    // If path is empty, this means we're matching with the root directory itself
    // so we use the path_prefix as the name
    if path.is_empty() && !path_prefix.is_empty() {
        return (path_prefix.display(path_style).to_string().into(), None);
    }

    let full_path = path_prefix.join(path);
    let file_name = full_path.file_name().unwrap_or_default();
    let display_path = full_path.display(path_style);
    let (directory, file_name) = display_path.split_at(display_path.len() - file_name.len());
    (
        file_name.to_string().into(),
        Some(SharedString::new(directory)).filter(|dir| !dir.is_empty()),
    )
}
