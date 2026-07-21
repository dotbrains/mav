use super::*;

impl GitPanel {
    pub(super) fn change_all_files_stage(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(active_repository) = self.active_repository.clone() else {
            return;
        };
        cx.spawn({
            async move |this, cx| {
                let result = this
                    .update(cx, |_this, cx| {
                        active_repository.update(cx, |repo, cx| {
                            if stage {
                                repo.stage_all(cx)
                            } else {
                                repo.unstage_all(cx)
                            }
                        })
                    })?
                    .await;

                this.update(cx, |this, cx| {
                    if let Err(err) = result {
                        this.show_error_toast(if stage { "add" } else { "reset" }, err, cx);
                    }
                    this.update_counts(active_repository.read(cx));
                    cx.notify()
                })
            }
        })
        .detach();
    }

    pub(super) fn stage_status_for_entry(entry: &GitStatusEntry, repo: &Repository) -> StageStatus {
        // Checking for current staged/unstaged file status is a chained operation:
        // 1. first, we check for any pending operation recorded in repository
        // 2. if there are no pending ops either running or finished, we then ask the repository
        //    for the most up-to-date file status read from disk - we do this since `entry` arg to this function `render_entry`
        //    is likely to be staled, and may lead to weird artifacts in the form of subsecond auto-uncheck/check on
        //    the checkbox's state (or flickering) which is undesirable.
        // 3. finally, if there is no info about this `entry` in the repo, we fall back to whatever status is encoded
        //    in `entry` arg.
        repo.pending_ops_for_path(&entry.repo_path)
            .and_then(|ops| {
                // In case the last operation in the list of pending operations
                // failed, we can't assume the stage status for this entry and
                // need to fallback to the actual state in the repo.
                if ops.last_op_errored() {
                    return None;
                }

                if ops.staging() || ops.staged() {
                    Some(StageStatus::Staged)
                } else {
                    Some(StageStatus::Unstaged)
                }
            })
            .or_else(|| {
                repo.status_for_path(&entry.repo_path)
                    .map(|status| status.status.staging())
            })
            .unwrap_or(entry.staging)
    }

    pub(super) fn stage_status_for_directory(
        &self,
        entry: &GitTreeDirEntry,
        repo: &Repository,
    ) -> StageStatus {
        let GitPanelViewMode::Tree(tree_state) = &self.view_mode else {
            util::debug_panic!("We should never render a directory entry while in flat view mode");
            return StageStatus::Unstaged;
        };

        let Some(descendants) = tree_state.directory_descendants.get(&entry.key) else {
            return StageStatus::Unstaged;
        };

        let show_placeholders = self.show_placeholders && !self.has_staged_changes();
        let mut fully_staged_count = 0usize;
        let mut any_staged_or_partially_staged = false;

        for descendant in descendants {
            if show_placeholders && !descendant.status.is_created() {
                fully_staged_count += 1;
                any_staged_or_partially_staged = true;
            } else {
                match GitPanel::stage_status_for_entry(descendant, repo) {
                    StageStatus::Staged => {
                        fully_staged_count += 1;
                        any_staged_or_partially_staged = true;
                    }
                    StageStatus::PartiallyStaged => {
                        any_staged_or_partially_staged = true;
                    }
                    StageStatus::Unstaged => {}
                }
            }
        }

        if descendants.is_empty() {
            StageStatus::Unstaged
        } else if fully_staged_count == descendants.len() {
            StageStatus::Staged
        } else if any_staged_or_partially_staged {
            StageStatus::PartiallyStaged
        } else {
            StageStatus::Unstaged
        }
    }

    pub fn stage_all(&mut self, _: &StageAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.change_all_files_stage(true, cx);
    }

    pub fn unstage_all(&mut self, _: &UnstageAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.change_all_files_stage(false, cx);
    }

    pub(super) fn toggle_staged_for_entry(
        &mut self,
        entry: &GitListEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_repository) = self.active_repository.clone() else {
            return;
        };
        let mut set_anchor: Option<RepoPath> = None;
        let mut clear_anchor = None;

        let (stage, repo_paths) = {
            let repo = active_repository.read(cx);
            match entry {
                GitListEntry::Status(status_entry) => {
                    let repo_paths = vec![status_entry.clone()];
                    let stage = match GitPanel::stage_status_for_entry(status_entry, &repo) {
                        StageStatus::Staged => {
                            if let Some(op) = self.bulk_staging.clone()
                                && op.anchor == status_entry.repo_path
                            {
                                clear_anchor = Some(op.anchor);
                            }
                            false
                        }
                        StageStatus::Unstaged | StageStatus::PartiallyStaged => {
                            set_anchor = Some(status_entry.repo_path.clone());
                            true
                        }
                    };
                    (stage, repo_paths)
                }
                GitListEntry::TreeStatus(status_entry) => {
                    let repo_paths = vec![status_entry.entry.clone()];
                    let stage = match GitPanel::stage_status_for_entry(&status_entry.entry, &repo) {
                        StageStatus::Staged => {
                            if let Some(op) = self.bulk_staging.clone()
                                && op.anchor == status_entry.entry.repo_path
                            {
                                clear_anchor = Some(op.anchor);
                            }
                            false
                        }
                        StageStatus::Unstaged | StageStatus::PartiallyStaged => {
                            set_anchor = Some(status_entry.entry.repo_path.clone());
                            true
                        }
                    };
                    (stage, repo_paths)
                }
                GitListEntry::Header(section) => {
                    let goal_staged_state = !self.header_state(section.header).selected();
                    let entries = self
                        .entries
                        .iter()
                        .filter_map(|entry| entry.status_entry())
                        .filter(|status_entry| {
                            section.contains(status_entry, &repo)
                                && GitPanel::stage_status_for_entry(status_entry, &repo).as_bool()
                                    != Some(goal_staged_state)
                        })
                        .cloned()
                        .collect::<Vec<_>>();

                    (goal_staged_state, entries)
                }
                GitListEntry::Directory(entry) => {
                    let goal_staged_state = match self.stage_status_for_directory(entry, repo) {
                        StageStatus::Staged => StageStatus::Unstaged,
                        StageStatus::Unstaged | StageStatus::PartiallyStaged => StageStatus::Staged,
                    };
                    let goal_stage = goal_staged_state == StageStatus::Staged;

                    let entries = self
                        .view_mode
                        .tree_state()
                        .and_then(|state| state.directory_descendants.get(&entry.key))
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|status_entry| {
                            GitPanel::stage_status_for_entry(status_entry, &repo)
                                != goal_staged_state
                        })
                        .collect::<Vec<_>>();
                    (goal_stage, entries)
                }
            }
        };
        if let Some(anchor) = clear_anchor {
            if let Some(op) = self.bulk_staging.clone()
                && op.anchor == anchor
            {
                self.bulk_staging = None;
            }
        }
        if let Some(anchor) = set_anchor {
            self.set_bulk_staging_anchor(anchor, cx);
        }

        self.change_file_stage(stage, repo_paths, cx);
    }

    pub(super) fn change_file_stage(
        &mut self,
        stage: bool,
        entries: Vec<GitStatusEntry>,
        cx: &mut Context<Self>,
    ) {
        let Some(active_repository) = self.active_repository.clone() else {
            return;
        };
        cx.spawn({
            async move |this, cx| {
                let result = this
                    .update(cx, |this, cx| {
                        let task = active_repository.update(cx, |repo, cx| {
                            let repo_paths = entries
                                .iter()
                                .map(|entry| entry.repo_path.clone())
                                .collect();
                            if stage {
                                repo.stage_entries(repo_paths, cx)
                            } else {
                                repo.unstage_entries(repo_paths, cx)
                            }
                        });
                        this.update_counts(active_repository.read(cx));
                        cx.notify();
                        task
                    })?
                    .await;

                this.update(cx, |this, cx| {
                    if let Err(err) = result {
                        this.show_error_toast(if stage { "add" } else { "reset" }, err, cx);
                        this.update_counts(active_repository.read(cx));
                    }
                    cx.notify();
                })
            }
        })
        .detach();
    }

    pub fn total_staged_count(&self) -> usize {
        self.tracked_staged_count + self.new_staged_count + self.conflicted_staged_count
    }
}
