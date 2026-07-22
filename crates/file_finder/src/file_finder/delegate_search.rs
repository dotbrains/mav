use super::*;

impl FileFinderDelegate {
    pub(super) fn new(
        file_finder: WeakEntity<FileFinder>,
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        currently_opened_path: Option<FoundPath>,
        history_items: Vec<FoundPath>,
        separate_history: bool,
        window: &mut Window,
        cx: &mut Context<FileFinder>,
    ) -> Self {
        Self::subscribe_to_updates(&project, window, cx);
        let channel_store = if FileFinderSettings::get_global(cx).include_channels {
            ChannelStore::try_global(cx)
        } else {
            None
        };
        Self {
            file_finder,
            workspace,
            project,
            channel_store,
            search_count: 0,
            latest_search_id: 0,
            latest_search_did_cancel: false,
            latest_search_query: None,
            currently_opened_path,
            matches: Matches::default(),
            has_changed_selected_index: false,
            selected_index: 0,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            search_in_flight: Arc::new(AtomicBool::new(false)),
            history_items,
            separate_history,
            first_update: true,
            focus_handle: cx.focus_handle(),
            include_ignored: FileFinderSettings::get_global(cx).include_ignored,
            include_ignored_refresh: Task::ready(()),
        }
    }

    pub(super) fn subscribe_to_updates(
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<FileFinder>,
    ) {
        cx.subscribe_in(project, window, |file_finder, _, event, window, cx| {
            match event {
                project::Event::WorktreeUpdatedEntries(_, _)
                | project::Event::WorktreeAdded(_)
                | project::Event::WorktreeRemoved(_) => file_finder
                    .picker
                    .update(cx, |picker, cx| picker.refresh(window, cx)),
                _ => {}
            };
        })
        .detach();
    }

    pub(super) fn spawn_search(
        &mut self,
        query: FileSearchQuery,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let relative_to = self
            .currently_opened_path
            .as_ref()
            .map(|found_path| Arc::clone(&found_path.project.path));
        let worktree_store = self.project.read(cx).worktree_store();
        let worktrees = worktree_store
            .read(cx)
            .visible_worktrees_and_single_files(cx)
            .collect::<Vec<_>>();
        let include_root_name = !should_hide_root_in_entry_path(&worktree_store, cx);
        let candidate_sets = worktrees
            .into_iter()
            .map(|worktree| {
                let worktree = worktree.read(cx);
                PathMatchCandidateSet {
                    snapshot: worktree.snapshot(),
                    include_ignored: self.include_ignored.unwrap_or_else(|| {
                        worktree.root_entry().is_some_and(|entry| entry.is_ignored)
                    }),
                    include_root_name,
                    candidates: project::Candidates::Files,
                }
            })
            .collect::<Vec<_>>();

        let search_id = util::post_inc(&mut self.search_count);
        self.cancel_flag.store(true, atomic::Ordering::Release);
        self.cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = self.cancel_flag.clone();
        cx.spawn_in(window, async move |picker, cx| {
            let matches = fuzzy_nucleo::match_path_sets(
                candidate_sets.as_slice(),
                query.path_query(),
                &relative_to,
                fuzzy_nucleo::Case::Ignore,
                100,
                &cancel_flag,
                cx.background_executor().clone(),
            )
            .await
            .into_iter()
            .map(ProjectPanelOrdMatch);
            let did_cancel = cancel_flag.load(atomic::Ordering::Acquire);
            picker
                .update(cx, |picker, cx| {
                    picker
                        .delegate
                        .set_search_matches(search_id, did_cancel, query, matches, cx)
                })
                .log_err();
        })
    }

    pub(super) fn set_search_matches(
        &mut self,
        search_id: usize,
        did_cancel: bool,
        query: FileSearchQuery,
        matches: impl IntoIterator<Item = ProjectPanelOrdMatch>,
        cx: &mut Context<Picker<Self>>,
    ) {
        if search_id >= self.latest_search_id {
            self.latest_search_id = search_id;
            let query_changed = Some(query.path_query())
                != self
                    .latest_search_query
                    .as_ref()
                    .map(|query| query.path_query());
            let extend_old_matches = self.latest_search_did_cancel && !query_changed;

            let selected_match = if query_changed {
                None
            } else {
                self.matches.get(self.selected_index).cloned()
            };

            let path_style = self.project.read(cx).path_style(cx);
            self.matches.push_new_matches(
                self.project.read(cx).worktree_store(),
                cx,
                &self.history_items,
                self.currently_opened_path.as_ref(),
                Some(&query),
                matches.into_iter(),
                extend_old_matches,
                path_style,
            );

            // Add channel matches
            if let Some(channel_store) = &self.channel_store {
                let channel_store = channel_store.read(cx);
                let channels: Vec<_> = channel_store.channels().cloned().collect();
                if !channels.is_empty() {
                    let candidates = channels
                        .iter()
                        .enumerate()
                        .map(|(id, channel)| StringMatchCandidate::new(id, &channel.name));
                    let channel_query = query.path_query();
                    let query_lower = channel_query.to_lowercase();
                    let mut channel_matches = Vec::new();
                    for candidate in candidates {
                        let channel_name = candidate.string;
                        let name_lower = channel_name.to_lowercase();

                        let mut positions = Vec::new();
                        let mut query_idx = 0;
                        for (name_idx, name_char) in name_lower.char_indices() {
                            if query_idx < query_lower.len() {
                                let query_char =
                                    query_lower[query_idx..].chars().next().unwrap_or_default();
                                if name_char == query_char {
                                    positions.push(name_idx);
                                    query_idx += query_char.len_utf8();
                                }
                            }
                        }

                        if query_idx == query_lower.len() {
                            let channel = &channels[candidate.id];
                            let score = if name_lower == query_lower {
                                1.0
                            } else if name_lower.starts_with(&query_lower) {
                                0.8
                            } else {
                                0.5 * (query_lower.len() as f64 / name_lower.len() as f64)
                            };
                            channel_matches.push(Match::Channel {
                                channel_id: channel.id,
                                channel_name: channel.name.clone(),
                                string_match: StringMatch {
                                    candidate_id: candidate.id,
                                    score,
                                    positions,
                                    string: channel_name,
                                },
                            });
                        }
                    }
                    for channel_match in channel_matches {
                        match self
                            .matches
                            .position(&channel_match, self.currently_opened_path.as_ref())
                        {
                            Ok(_duplicate) => {}
                            Err(ix) => self.matches.matches.insert(ix, channel_match),
                        }
                    }
                }
            }

            let query_path = query.raw_query.as_str();
            if let Ok(mut query_path) = RelPath::new(Path::new(query_path), path_style) {
                let available_worktree = self
                    .project
                    .read(cx)
                    .visible_worktrees(cx)
                    .filter(|worktree| !worktree.read(cx).is_single_file())
                    .collect::<Vec<_>>();
                let worktree_count = available_worktree.len();
                let mut expect_worktree = available_worktree.first().cloned();
                for worktree in &available_worktree {
                    let worktree_root = worktree.read(cx).root_name();
                    if worktree_count > 1 {
                        if let Ok(suffix) = query_path.strip_prefix(worktree_root) {
                            query_path = Cow::Owned(suffix.to_owned());
                            expect_worktree = Some(worktree.clone());
                            break;
                        }
                    }
                }

                if let Some(FoundPath { ref project, .. }) = self.currently_opened_path {
                    let worktree_id = project.worktree_id;
                    let focused_file_in_available_worktree = available_worktree
                        .iter()
                        .any(|wt| wt.read(cx).id() == worktree_id);

                    if focused_file_in_available_worktree {
                        expect_worktree = self.project.read(cx).worktree_for_id(worktree_id, cx);
                    }
                }

                if let Some(worktree) = expect_worktree {
                    let worktree = worktree.read(cx);
                    if worktree.entry_for_path(&query_path).is_none()
                        && !query.raw_query.ends_with("/")
                        && !(path_style.is_windows() && query.raw_query.ends_with("\\"))
                    {
                        self.matches.matches.push(Match::CreateNew(ProjectPath {
                            worktree_id: worktree.id(),
                            path: query_path.into_arc(),
                        }));
                    }
                }
            }

            self.selected_index = selected_match.map_or_else(
                || self.calculate_selected_index(cx),
                |m| {
                    self.matches
                        .position(&m, self.currently_opened_path.as_ref())
                        .unwrap_or(0)
                },
            );

            self.latest_search_query = Some(query);
            self.latest_search_did_cancel = did_cancel;

            cx.notify();
        }
    }
}
