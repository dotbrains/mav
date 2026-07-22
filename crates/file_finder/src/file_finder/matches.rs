use super::*;

/// Use a custom ordering for file finder: the regular one
/// defines max element with the highest score and the latest alphanumerical path (in case of a tie on other params), e.g:
/// `[{score: 0.5, path = "c/d" }, { score: 0.5, path = "/a/b" }]`
///
/// In the file finder, we would prefer to have the max element with the highest score and the earliest alphanumerical path, e.g:
/// `[{ score: 0.5, path = "/a/b" }, {score: 0.5, path = "c/d" }]`
/// as the files are shown in the project panel lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectPanelOrdMatch(PathMatch);

impl Ord for ProjectPanelOrdMatch {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0
            .score
            .partial_cmp(&other.0.score)
            .unwrap_or(cmp::Ordering::Equal)
            .then_with(|| self.0.worktree_id.cmp(&other.0.worktree_id))
            .then_with(|| {
                other
                    .0
                    .distance_to_relative_ancestor
                    .cmp(&self.0.distance_to_relative_ancestor)
            })
            .then_with(|| self.0.path.cmp(&other.0.path).reverse())
    }
}

impl PartialOrd for ProjectPanelOrdMatch {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub(super) struct Matches {
    separate_history: bool,
    matches: Vec<Match>,
}

#[derive(Debug, Clone)]
pub(super) enum Match {
    History {
        path: FoundPath,
        panel_match: Option<ProjectPanelOrdMatch>,
    },
    Search(ProjectPanelOrdMatch),
    Channel {
        channel_id: ChannelId,
        channel_name: SharedString,
        string_match: StringMatch,
    },
    CreateNew(ProjectPath),
}

impl Match {
    pub(super) fn relative_path(&self) -> Option<&Arc<RelPath>> {
        match self {
            Match::History { path, .. } => Some(&path.project.path),
            Match::Search(panel_match) => Some(&panel_match.0.path),
            Match::Channel { .. } | Match::CreateNew(_) => None,
        }
    }

    pub(super) fn abs_path(&self, project: &Entity<Project>, cx: &App) -> Option<PathBuf> {
        match self {
            Match::History { path, .. } => Some(path.absolute.clone()),
            Match::Search(ProjectPanelOrdMatch(path_match)) => Some(
                project
                    .read(cx)
                    .worktree_for_id(WorktreeId::from_usize(path_match.worktree_id), cx)?
                    .read(cx)
                    .absolutize(&path_match.path),
            ),
            Match::Channel { .. } | Match::CreateNew(_) => None,
        }
    }

    pub(super) fn panel_match(&self) -> Option<&ProjectPanelOrdMatch> {
        match self {
            Match::History { panel_match, .. } => panel_match.as_ref(),
            Match::Search(panel_match) => Some(panel_match),
            Match::Channel { .. } | Match::CreateNew(_) => None,
        }
    }
}

impl Matches {
    pub(super) fn len(&self) -> usize {
        self.matches.len()
    }

    pub(super) fn get(&self, index: usize) -> Option<&Match> {
        self.matches.get(index)
    }

    pub(super) fn position(
        &self,
        entry: &Match,
        currently_opened: Option<&FoundPath>,
    ) -> Result<usize, usize> {
        if let Match::History {
            path,
            panel_match: None,
        } = entry
        {
            // Slow case: linear search by path. Should not happen actually,
            // since we call `position` only if matches set changed, but the query has not changed.
            // And History entries do not have panel_match if query is empty, so there's no
            // reason for the matches set to change.
            self.matches
                .iter()
                .position(|m| match m.relative_path() {
                    Some(p) => path.project.path == *p,
                    None => false,
                })
                .ok_or(0)
        } else {
            self.matches.binary_search_by(|m| {
                // `reverse()` since if cmp_matches(a, b) == Ordering::Greater, then a is better than b.
                // And we want the better entries go first.
                Self::cmp_matches(self.separate_history, currently_opened, m, entry).reverse()
            })
        }
    }

    pub(super) fn push_new_matches<'a>(
        &'a mut self,
        worktree_store: Entity<WorktreeStore>,
        cx: &'a App,
        history_items: impl IntoIterator<Item = &'a FoundPath> + Clone,
        currently_opened: Option<&'a FoundPath>,
        query: Option<&FileSearchQuery>,
        new_search_matches: impl Iterator<Item = ProjectPanelOrdMatch>,
        extend_old_matches: bool,
        path_style: PathStyle,
    ) {
        let Some(query) = query else {
            // assuming that if there's no query, then there's no search matches.
            self.matches.clear();
            let path_to_entry = |found_path: &FoundPath| Match::History {
                path: found_path.clone(),
                panel_match: None,
            };

            self.matches
                .extend(history_items.into_iter().map(path_to_entry));
            return;
        };

        let worktree_name_by_id = worktree_names_for_history_matching(&worktree_store, cx);
        let new_history_matches = matching_history_items(
            history_items,
            currently_opened,
            worktree_name_by_id,
            query,
            path_style,
        );
        let new_search_matches: Vec<Match> = new_search_matches
            .filter(|path_match| {
                !new_history_matches.contains_key(&ProjectPath {
                    path: path_match.0.path.clone(),
                    worktree_id: WorktreeId::from_usize(path_match.0.worktree_id),
                })
            })
            .map(Match::Search)
            .collect();

        if extend_old_matches {
            // since we take history matches instead of new search matches
            // and history matches has not changed(since the query has not changed and we do not extend old matches otherwise),
            // old matches can't contain paths present in history_matches as well.
            self.matches.retain(|m| matches!(m, Match::Search(_)));
        } else {
            self.matches.clear();
        }

        // At this point we have an unsorted set of new history matches, an unsorted set of new search matches
        // and a sorted set of old search matches.
        // It is possible that the new search matches' paths contain some of the old search matches' paths.
        // History matches' paths are unique, since store in a HashMap by path.
        // We build a sorted Vec<Match>, eliminating duplicate search matches.
        // Search matches with the same paths should have equal `ProjectPanelOrdMatch`, so we should
        // not have any duplicates after building the final list.
        for new_match in new_history_matches.into_values().chain(new_search_matches) {
            match self.position(&new_match, currently_opened) {
                Ok(_duplicate) => continue,
                Err(i) => {
                    self.matches.insert(i, new_match);
                    if self.matches.len() == 100 {
                        break;
                    }
                }
            }
        }
    }

    /// If a < b, then a is a worse match, aligning with the `ProjectPanelOrdMatch` ordering.
    pub(super) fn cmp_matches(
        separate_history: bool,
        currently_opened: Option<&FoundPath>,
        a: &Match,
        b: &Match,
    ) -> cmp::Ordering {
        // Handle CreateNew variant - always put it at the end
        match (a, b) {
            (Match::CreateNew(_), _) => return cmp::Ordering::Less,
            (_, Match::CreateNew(_)) => return cmp::Ordering::Greater,
            _ => {}
        }

        match (&a, &b) {
            // bubble currently opened files to the top
            (Match::History { path, .. }, _) if Some(path) == currently_opened => {
                return cmp::Ordering::Greater;
            }
            (_, Match::History { path, .. }) if Some(path) == currently_opened => {
                return cmp::Ordering::Less;
            }

            _ => {}
        }

        if separate_history {
            match (a, b) {
                (Match::History { .. }, Match::Search(_)) => return cmp::Ordering::Greater,
                (Match::Search(_), Match::History { .. }) => return cmp::Ordering::Less,

                _ => {}
            }
        }

        // For file-vs-file matches, use the existing detailed comparison.
        if let (Some(a_panel), Some(b_panel)) = (a.panel_match(), b.panel_match()) {
            return a_panel.cmp(b_panel);
        }

        let a_score = Self::match_score(a);
        let b_score = Self::match_score(b);
        // When at least one side is a channel, compare by raw score.
        a_score
            .partial_cmp(&b_score)
            .unwrap_or(cmp::Ordering::Equal)
    }

    pub(super) fn match_score(m: &Match) -> f64 {
        match m {
            Match::History { panel_match, .. } => panel_match.as_ref().map_or(0.0, |pm| pm.0.score),
            Match::Search(pm) => pm.0.score,
            Match::Channel { string_match, .. } => string_match.score,
            Match::CreateNew(_) => 0.0,
        }
    }
}

fn matching_history_items<'a>(
    history_items: impl IntoIterator<Item = &'a FoundPath>,
    currently_opened: Option<&'a FoundPath>,
    worktree_name_by_id: Option<HashMap<WorktreeId, Arc<RelPath>>>,
    query: &FileSearchQuery,
    path_style: PathStyle,
) -> HashMap<ProjectPath, Match> {
    let mut candidates_paths = HashMap::default();

    let history_items_by_worktrees = history_items
        .into_iter()
        .chain(currently_opened)
        .map(|found_path| {
            // Only match history items names, otherwise their paths may match too many queries,
            // producing false positives. E.g. `foo` would match both `something/foo/bar.rs` and
            // `something/foo/foo.rs` and if the former is a history item, it would be shown first
            // always, despite the latter being a better match.
            let candidate = PathMatchCandidate::new(
                &found_path.project.path,
                false,
                worktree_name_by_id
                    .as_ref()
                    .and_then(|m| m.get(&found_path.project.worktree_id))
                    .map(|prefix| prefix.as_ref()),
            );
            candidates_paths.insert(&found_path.project, found_path);
            (found_path.project.worktree_id, candidate)
        })
        .fold(
            HashMap::default(),
            |mut candidates, (worktree_id, new_candidate)| {
                candidates
                    .entry(worktree_id)
                    .or_insert_with(Vec::new)
                    .push(new_candidate);
                candidates
            },
        );
    let mut matching_history_paths = HashMap::default();
    for (worktree, candidates) in history_items_by_worktrees {
        let max_results = candidates.len() + 1;
        let worktree_root_name = worktree_name_by_id
            .as_ref()
            .and_then(|w| w.get(&worktree).cloned());

        matching_history_paths.extend(
            fuzzy_nucleo::match_fixed_path_set(
                candidates,
                worktree.to_usize(),
                worktree_root_name,
                query.path_query(),
                fuzzy_nucleo::Case::Ignore,
                max_results,
                path_style,
            )
            .into_iter()
            // filter matches where at least one matched position is in filename portion, to prevent directory matches, nucleo scores them higher as history items are matched against their full path
            .filter(|path_match| {
                if let Some(filename) = path_match.path.file_name() {
                    let filename_start = path_match.path.as_unix_str().len() - filename.len();
                    path_match
                        .positions
                        .iter()
                        .any(|&pos| pos >= filename_start)
                } else {
                    true
                }
            })
            .filter_map(|path_match| {
                let worktree_id = WorktreeId::from_usize(path_match.worktree_id);
                let project_path = ProjectPath {
                    worktree_id,
                    path: Arc::clone(&path_match.path),
                };
                // For single-file worktrees, fuzzy_nucleo moves the worktree root name
                // into path_match.path (root_is_file handling), so the stored key of ""
                // won't match. Fall back to an empty-path lookup for those entries.
                let (_, found_path) =
                    candidates_paths.remove_entry(&project_path).or_else(|| {
                        candidates_paths.remove_entry(&ProjectPath {
                            worktree_id,
                            path: RelPath::empty_arc(),
                        })
                    })?;
                // Key with path_match.path so the deduplication check in push_new_matches
                // (which also uses path_match.path) correctly suppresses the search duplicate.
                Some((
                    project_path,
                    Match::History {
                        path: found_path.clone(),
                        panel_match: Some(ProjectPanelOrdMatch(path_match)),
                    },
                ))
            }),
        );
    }
    matching_history_paths
}

fn should_hide_root_in_entry_path(worktree_store: &Entity<WorktreeStore>, cx: &App) -> bool {
    let multiple_worktrees = worktree_store
        .read(cx)
        .visible_worktrees(cx)
        .filter(|worktree| !worktree.read(cx).is_single_file())
        .nth(1)
        .is_some();
    ProjectPanelSettings::get_global(cx).hide_root && !multiple_worktrees
}

fn worktree_names_for_history_matching(
    worktree_store: &Entity<WorktreeStore>,
    cx: &App,
) -> Option<HashMap<WorktreeId, Arc<RelPath>>> {
    let hide_root = should_hide_root_in_entry_path(worktree_store, cx);
    let names = worktree_store
        .read(cx)
        .worktrees()
        .filter_map(|worktree| {
            let worktree = worktree.read(cx);
            if hide_root && !worktree.is_single_file() {
                None
            } else {
                Some((worktree.id(), worktree.root_name().into()))
            }
        })
        .collect::<HashMap<_, _>>();

    if names.is_empty() { None } else { Some(names) }
}

fn project_path_for_search_match(
    project: &Entity<Project>,
    path_match: &PathMatch,
    cx: &App,
) -> ProjectPath {
    let worktree_id = WorktreeId::from_usize(path_match.worktree_id);
    let path = if project
        .read(cx)
        .worktree_for_id(worktree_id, cx)
        .is_some_and(|worktree| worktree.read(cx).is_single_file())
    {
        RelPath::empty_arc()
    } else {
        path_match.path.clone()
    };

    ProjectPath { worktree_id, path }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct FoundPath {
    project: ProjectPath,
    absolute: PathBuf,
}

impl FoundPath {
    pub(super) fn new(project: ProjectPath, absolute: PathBuf) -> Self {
        Self { project, absolute }
    }
}
