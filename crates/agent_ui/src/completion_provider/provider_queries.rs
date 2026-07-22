use super::*;

impl<T: PromptCompletionProviderDelegate> PromptCompletionProvider<T> {
    fn search_slash_commands(
        &self,
        query: String,
        cx: &mut App,
    ) -> Task<Vec<SlashCompletionCandidate>> {
        // Notify the delegate that slash autocomplete is being
        // invoked, so it can lazily kick off any work that produces
        // additional commands or skills. Whatever it produces won't be
        // visible in the current autocomplete pass (we read available
        // items synchronously below), but will appear on the next
        // invocation.
        self.source.slash_autocomplete_invoked(cx);

        let mut candidates = self
            .source
            .available_skills(cx)
            .into_iter()
            .map(SlashCompletionCandidate::Skill)
            .collect::<Vec<_>>();
        candidates.extend(
            self.source
                .available_commands(cx)
                .into_iter()
                .map(SlashCompletionCandidate::Command),
        );
        if candidates.is_empty() {
            return Task::ready(Vec::new());
        }

        cx.spawn(async move |cx| {
            let string_match_candidates = candidates
                .iter()
                .enumerate()
                .map(|(id, candidate)| StringMatchCandidate::new(id, candidate.name()))
                .collect::<Vec<_>>();

            let matches = fuzzy::match_strings(
                &string_match_candidates,
                &query,
                false,
                true,
                100,
                &Arc::new(AtomicBool::default()),
                cx.background_executor().clone(),
            )
            .await;

            matches
                .into_iter()
                .map(|mat| candidates[mat.candidate_id].clone())
                .collect()
        })
    }

    fn fetch_branch_diff_match(
        &self,
        workspace: &Entity<Workspace>,
        cx: &mut App,
    ) -> Option<Task<Option<BranchDiffMatch>>> {
        let project = workspace.read(cx).project().clone();
        let repo = project.read(cx).active_repository(cx)?;

        let default_branch_receiver = repo.update(cx, |repo, _| repo.default_branch(true));

        Some(cx.spawn(async move |_cx| {
            let base_ref = default_branch_receiver
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()?;

            Some(BranchDiffMatch { base_ref })
        }))
    }

    fn search_mentions(
        &self,
        mode: Option<PromptContextType>,
        query: String,
        cancellation_flag: Arc<AtomicBool>,
        cx: &mut App,
    ) -> Task<Vec<Match>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Vec::default());
        };
        match mode {
            Some(PromptContextType::File) => {
                let search_files_task = search_files(query, cancellation_flag, &workspace, cx);
                cx.background_spawn(async move {
                    search_files_task
                        .await
                        .into_iter()
                        .map(Match::File)
                        .collect()
                })
            }

            Some(PromptContextType::Symbol) => {
                let search_symbols_task = search_symbols(query, cancellation_flag, &workspace, cx);
                cx.background_spawn(async move {
                    search_symbols_task
                        .await
                        .into_iter()
                        .map(Match::Symbol)
                        .collect()
                })
            }

            Some(PromptContextType::Thread) => {
                let sessions = collect_session_matches(cx);
                if !sessions.is_empty() {
                    let search_task =
                        filter_sessions_by_query(query, cancellation_flag, sessions, cx);
                    cx.spawn(async move |_cx| {
                        search_task.await.into_iter().map(Match::Thread).collect()
                    })
                } else {
                    Task::ready(Vec::new())
                }
            }

            Some(PromptContextType::Fetch) => {
                if !query.is_empty() {
                    Task::ready(vec![Match::Fetch(query.into())])
                } else {
                    Task::ready(Vec::new())
                }
            }

            Some(PromptContextType::Skill) => {
                let skills = self.source.available_skills(cx);
                let search_skills_task = search_skills(query, cancellation_flag, skills, cx);
                cx.background_spawn(async move {
                    search_skills_task
                        .await
                        .into_iter()
                        .map(Match::Skill)
                        .collect::<Vec<_>>()
                })
            }

            Some(PromptContextType::Diagnostics) => Task::ready(Vec::new()),

            Some(PromptContextType::BranchDiff) => Task::ready(Vec::new()),

            None if query.is_empty() => {
                let recent_task = self.recent_context_picker_entries(&workspace, cx);
                let entries = self
                    .available_context_picker_entries(&workspace, cx)
                    .into_iter()
                    .map(|mode| {
                        Match::Entry(EntryMatch {
                            entry: mode,
                            mat: None,
                        })
                    })
                    .collect::<Vec<_>>();

                let branch_diff_task = if self
                    .source
                    .supports_context(PromptContextType::BranchDiff, cx)
                {
                    self.fetch_branch_diff_match(&workspace, cx)
                } else {
                    None
                };

                cx.spawn(async move |_cx| {
                    let mut matches = recent_task.await;
                    matches.extend(entries);

                    if let Some(branch_diff_task) = branch_diff_task {
                        if let Some(branch_diff_match) = branch_diff_task.await {
                            matches.push(Match::BranchDiff(branch_diff_match));
                        }
                    }

                    matches
                })
            }
            None => {
                let executor = cx.background_executor().clone();

                let search_files_task =
                    search_files(query.clone(), cancellation_flag, &workspace, cx);

                let entries = self.available_context_picker_entries(&workspace, cx);
                let entry_candidates = entries
                    .iter()
                    .enumerate()
                    .map(|(ix, entry)| StringMatchCandidate::new(ix, entry.keyword()))
                    .collect::<Vec<_>>();

                let branch_diff_task = if self
                    .source
                    .supports_context(PromptContextType::BranchDiff, cx)
                {
                    self.fetch_branch_diff_match(&workspace, cx)
                } else {
                    None
                };

                cx.spawn(async move |cx| {
                    let mut matches = search_files_task
                        .await
                        .into_iter()
                        .map(Match::File)
                        .collect::<Vec<_>>();

                    let entry_matches = fuzzy::match_strings(
                        &entry_candidates,
                        &query,
                        false,
                        true,
                        100,
                        &Arc::new(AtomicBool::default()),
                        executor,
                    )
                    .await;

                    matches.extend(entry_matches.into_iter().map(|mat| {
                        Match::Entry(EntryMatch {
                            entry: entries[mat.candidate_id],
                            mat: Some(mat),
                        })
                    }));

                    if let Some(branch_diff_task) = branch_diff_task {
                        let branch_diff_keyword = PromptContextType::BranchDiff.keyword();
                        let branch_diff_matches = fuzzy::match_strings(
                            &[StringMatchCandidate::new(0, branch_diff_keyword)],
                            &query,
                            false,
                            true,
                            1,
                            &Arc::new(AtomicBool::default()),
                            cx.background_executor().clone(),
                        )
                        .await;

                        if !branch_diff_matches.is_empty() {
                            if let Some(branch_diff_match) = branch_diff_task.await {
                                matches.push(Match::BranchDiff(branch_diff_match));
                            }
                        }
                    }

                    matches.sort_by(|a, b| {
                        b.score()
                            .partial_cmp(&a.score())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    matches
                })
            }
        }
    }

    fn recent_context_picker_entries(
        &self,
        workspace: &Entity<Workspace>,
        cx: &mut App,
    ) -> Task<Vec<Match>> {
        let mut recent = Vec::with_capacity(6);

        let mut mentions = self
            .mention_set
            .read_with(cx, |store, _cx| store.mentions());
        let workspace = workspace.read(cx);
        let project = workspace.project().read(cx);
        let include_root_name = workspace.visible_worktrees(cx).count() > 1;

        if let Some(agent_panel) = workspace.panel::<AgentPanel>(cx)
            && let Some(thread) = agent_panel.read(cx).active_agent_thread(cx)
            && let Some(title) = thread.read(cx).title()
        {
            mentions.insert(MentionUri::Thread {
                id: thread.read(cx).session_id().clone(),
                name: title.to_string(),
            });
        }

        recent.extend(
            workspace
                .recent_navigation_history_iter(cx)
                .filter(|(_, abs_path)| {
                    abs_path.as_ref().is_none_or(|path| {
                        !mentions.contains(&MentionUri::File {
                            abs_path: path.clone(),
                        })
                    })
                })
                .take(4)
                .filter_map(|(project_path, _)| {
                    project
                        .worktree_for_id(project_path.worktree_id, cx)
                        .map(|worktree| {
                            let path_prefix = if include_root_name {
                                worktree.read(cx).root_name().into()
                            } else {
                                RelPath::empty_arc()
                            };
                            Match::File(FileMatch {
                                mat: fuzzy::PathMatch {
                                    score: 1.,
                                    positions: Vec::new(),
                                    worktree_id: project_path.worktree_id.to_usize(),
                                    path: project_path.path,
                                    path_prefix,
                                    is_dir: false,
                                    distance_to_relative_ancestor: 0,
                                },
                                is_recent: true,
                            })
                        })
                }),
        );

        if !self.source.supports_context(PromptContextType::Thread, cx) {
            return Task::ready(recent);
        }

        let sessions = collect_session_matches(cx);
        const RECENT_COUNT: usize = 2;
        recent.extend(
            sessions
                .into_iter()
                .filter(|session| {
                    let uri = MentionUri::Thread {
                        id: session.session_id.clone(),
                        name: session.title.to_string(),
                    };
                    !mentions.contains(&uri)
                })
                .take(RECENT_COUNT)
                .map(Match::RecentThread),
        );

        Task::ready(recent)
    }

    fn available_context_picker_entries(
        &self,
        workspace: &Entity<Workspace>,
        cx: &mut App,
    ) -> Vec<PromptContextEntry> {
        let mut entries = vec![
            PromptContextEntry::Mode(PromptContextType::File),
            PromptContextEntry::Mode(PromptContextType::Symbol),
        ];

        if self.source.supports_context(PromptContextType::Thread, cx) {
            entries.push(PromptContextEntry::Mode(PromptContextType::Thread));
        }

        let has_active_selection = workspace.update(cx, |workspace, cx| {
            AgentContextSource::from_active(workspace, cx)
                .and_then(|source| source.read_selection(workspace, false, cx))
                .is_some()
        });
        if has_active_selection {
            entries.push(PromptContextEntry::Action(
                PromptContextAction::AddSelections,
            ));
        }

        if self.source.supports_context(PromptContextType::Skill, cx)
            && !self.source.available_skills(cx).is_empty()
        {
            entries.push(PromptContextEntry::Mode(PromptContextType::Skill));
        }

        if self.source.supports_context(PromptContextType::Fetch, cx) {
            entries.push(PromptContextEntry::Mode(PromptContextType::Fetch));
        }

        if self
            .source
            .supports_context(PromptContextType::Diagnostics, cx)
        {
            let summary = workspace
                .read(cx)
                .project()
                .read(cx)
                .diagnostic_summary(false, cx);
            if summary.error_count > 0 || summary.warning_count > 0 {
                entries.push(PromptContextEntry::Mode(PromptContextType::Diagnostics));
            }
        }

        entries
    }
}
