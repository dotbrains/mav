use super::*;

impl WorktreePickerDelegate {
    fn update_matches_impl(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let repo_worktrees = self.all_repo_worktrees().to_vec();

        let normalized_query = query.replace(' ', "-");
        let main_worktree_path = self
            .all_worktrees
            .iter()
            .find(|wt| wt.is_main)
            .map(|wt| wt.path.clone());
        let has_named_worktree = self.all_worktrees.iter().any(|worktree| {
            worktree.directory_name(main_worktree_path.as_deref()) == normalized_query
        });
        let create_named_disabled_reason: Option<String> = if self.has_multiple_repositories {
            Some("Cannot create a named worktree in a project with multiple repositories".into())
        } else if has_named_worktree {
            Some("A worktree with this name already exists".into())
        } else {
            None
        };

        let show_default_branch_create =
            !self.has_multiple_repositories && self.default_branch.is_some();
        let default_branch = self.default_branch.clone();

        if query.is_empty() {
            let mut matches = self.build_fixed_entries();

            if !repo_worktrees.is_empty() {
                let main_worktree_path = repo_worktrees
                    .iter()
                    .find(|wt| wt.is_main)
                    .map(|wt| wt.path.clone());

                let project_paths = &self.project_worktree_paths;

                let sort_by_name = |a: &GitWorktree, b: &GitWorktree| {
                    a.directory_name(main_worktree_path.as_deref())
                        .cmp(&b.directory_name(main_worktree_path.as_deref()))
                };

                let (mut open_here, mut others): (Vec<_>, Vec<_>) = repo_worktrees
                    .into_iter()
                    .partition(|worktree| project_paths.contains(&worktree.path));
                open_here.sort_by(sort_by_name);
                others.sort_by(sort_by_name);

                matches.push(WorktreeEntry::Separator);

                if open_here.len() > 1 {
                    matches.push(WorktreeEntry::SectionHeader("This Window".into()));
                    for worktree in open_here {
                        matches.push(WorktreeEntry::Worktree {
                            worktree,
                            positions: Vec::new(),
                        });
                    }

                    if !others.is_empty() {
                        matches.push(WorktreeEntry::Separator);
                    }

                    for worktree in others {
                        matches.push(WorktreeEntry::Worktree {
                            worktree,
                            positions: Vec::new(),
                        });
                    }
                } else {
                    for worktree in open_here.into_iter().chain(others) {
                        matches.push(WorktreeEntry::Worktree {
                            worktree,
                            positions: Vec::new(),
                        });
                    }
                }
            }

            self.matches = matches;
            self.sync_selected_index(false);
            return Task::ready(());
        }

        let main_worktree_path = repo_worktrees
            .iter()
            .find(|wt| wt.is_main)
            .map(|wt| wt.path.clone());
        let candidates: Vec<_> = repo_worktrees
            .iter()
            .enumerate()
            .map(|(ix, worktree)| {
                StringMatchCandidate::new(
                    ix,
                    &worktree.directory_name(main_worktree_path.as_deref()),
                )
            })
            .collect();

        let executor = cx.background_executor().clone();

        let task = cx.background_executor().spawn(async move {
            fuzzy::match_strings(
                &candidates,
                &query,
                true,
                true,
                10000,
                &Default::default(),
                executor,
            )
            .await
        });

        let repo_worktrees_clone = repo_worktrees;
        cx.spawn_in(window, async move |picker, cx| {
            let fuzzy_matches = task.await;

            picker
                .update_in(cx, |picker, _window, cx| {
                    let mut new_matches: Vec<WorktreeEntry> = Vec::new();

                    for candidate in &fuzzy_matches {
                        new_matches.push(WorktreeEntry::Worktree {
                            worktree: repo_worktrees_clone[candidate.candidate_id].clone(),
                            positions: candidate.positions.clone(),
                        });
                    }

                    if !new_matches.is_empty() {
                        new_matches.push(WorktreeEntry::Separator);
                    }
                    if show_default_branch_create {
                        if let Some(ref default_branch) = default_branch {
                            new_matches.push(WorktreeEntry::CreateNamed {
                                name: normalized_query.clone(),
                                from_branch: Some(default_branch.clone()),
                                disabled_reason: create_named_disabled_reason.clone(),
                            });
                        }
                    } else {
                        new_matches.push(WorktreeEntry::CreateNamed {
                            name: normalized_query.clone(),
                            from_branch: None,
                            disabled_reason: create_named_disabled_reason.clone(),
                        });
                    }

                    picker.delegate.matches = new_matches;
                    picker.delegate.sync_selected_index(true);

                    cx.notify();
                })
                .log_err();
        })
    }
}
