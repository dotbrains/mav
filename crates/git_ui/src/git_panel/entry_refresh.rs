use super::*;

impl GitPanel {
    pub(super) fn fill_co_authors(&mut self, message: &mut String, cx: &mut Context<Self>) {
        const CO_AUTHOR_PREFIX: &str = "Co-authored-by: ";

        let existing_text = message.to_ascii_lowercase();
        let lowercase_co_author_prefix = CO_AUTHOR_PREFIX.to_lowercase();
        let mut ends_with_co_authors = false;
        let existing_co_authors = existing_text
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.starts_with(&lowercase_co_author_prefix) {
                    ends_with_co_authors = true;
                    Some(line)
                } else {
                    ends_with_co_authors = false;
                    None
                }
            })
            .collect::<HashSet<_>>();

        let new_co_authors = self
            .potential_co_authors(cx)
            .into_iter()
            .filter(|(_, email)| {
                !existing_co_authors
                    .iter()
                    .any(|existing| existing.contains(email.as_str()))
            })
            .collect::<Vec<_>>();

        if new_co_authors.is_empty() {
            return;
        }

        if !ends_with_co_authors {
            message.push('\n');
        }
        for (name, email) in new_co_authors {
            message.push('\n');
            message.push_str(CO_AUTHOR_PREFIX);
            message.push_str(&name);
            message.push_str(" <");
            message.push_str(&email);
            message.push('>');
        }
        message.push('\n');
    }

    pub(super) fn schedule_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = cx.entity().downgrade();
        let new_active_repository = self.project.read(cx).active_repository(cx);
        let active_repository_changed = self.active_repository.as_ref().map(Entity::entity_id)
            != new_active_repository.as_ref().map(Entity::entity_id);
        if active_repository_changed && self.amend_pending {
            // Leaving a repository with a pending amend: undo it so the amend
            // state doesn't carry over to the newly active repository. The
            // commit editor still holds the previous repository's buffer here
            // (`reopen_commit_buffer` swaps it asynchronously below), so this
            // restores the pre-amend draft into that repository's buffer.
            self.set_amend_pending(false, cx);
        }
        self.active_repository = new_active_repository;
        self.reopen_commit_buffer(window, cx);
        self.preload_commit_history(cx);
        if self.active_tab == GitPanelTab::History {
            self.load_commit_history(cx);
        }
        self.update_visible_entries_task = cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(UPDATE_DEBOUNCE).await;
            if let Some(git_panel) = handle.upgrade() {
                git_panel
                    .update_in(cx, |git_panel, window, cx| {
                        git_panel.update_visible_entries(window, cx);
                    })
                    .ok();
            }
        });
    }

    pub(super) fn reopen_commit_buffer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_repo) = self.active_repository.as_ref() else {
            self.reopen_commit_buffer_task = Task::ready(());
            return;
        };
        let active_repository_abs_path = active_repo
            .read(cx)
            .work_directory_abs_path
            .to_string_lossy()
            .into_owned();
        let load_buffer = active_repo.update(cx, |active_repo, cx| {
            let project = self.project.read(cx);
            active_repo.open_commit_buffer(
                Some(project.languages().clone()),
                project.buffer_store().clone(),
                cx,
            )
        });
        let load_template = self.load_commit_template(cx);

        self.reopen_commit_buffer_task = cx.spawn_in(window, async move |git_panel, cx| {
            let result = async {
                let buffer = load_buffer.await?;
                let template = load_template.await?;

                git_panel.update_in(cx, move |git_panel, window, cx| {
                    git_panel.commit_template = template;
                    let restored_commit_message = git_panel
                        .pending_commit_message_restores
                        .remove(&active_repository_abs_path);
                    if let Some(restored_commit_message) = restored_commit_message {
                        git_panel.amend_pending = restored_commit_message.amend_pending;
                        git_panel.original_commit_message =
                            restored_commit_message.original_message;
                        cx.notify();
                        if let Some(message) = restored_commit_message.message
                            && buffer.read(cx).text().trim().is_empty()
                        {
                            buffer.update(cx, |buffer, cx| {
                                let start = buffer.anchor_before(0);
                                let end = buffer.anchor_after(buffer.len());
                                buffer.edit([(start..end, message)], None, cx);
                            });
                        }
                    }
                    if buffer.read(cx).text().trim().is_empty() {
                        let template_text = git_panel
                            .commit_template
                            .as_ref()
                            .map(|t| t.template.clone())
                            .unwrap_or_default();
                        if !template_text.is_empty() {
                            buffer.update(cx, |buffer, cx| {
                                let start = buffer.anchor_before(0);
                                let end = buffer.anchor_after(buffer.len());
                                buffer.edit([(start..end, template_text)], None, cx);
                            });
                        }
                    }

                    if git_panel
                        .commit_editor
                        .read(cx)
                        .buffer()
                        .read(cx)
                        .as_singleton()
                        .as_ref()
                        != Some(&buffer)
                    {
                        git_panel.commit_editor = cx.new(|cx| {
                            commit_message_editor(
                                buffer.clone(),
                                git_panel.suggest_commit_message(cx).map(SharedString::from),
                                git_panel.project.clone(),
                                true,
                                window,
                                cx,
                            )
                        });
                    }

                    git_panel._commit_message_buffer_subscription =
                        Some(cx.subscribe(&buffer, |this, _, event, cx| {
                            if matches!(event, BufferEvent::Edited { .. }) {
                                this.serialize(cx);
                            }
                        }));
                })?;
                anyhow::Ok(())
            }
            .await;
            result.log_err();
        });
    }

    pub(super) fn update_visible_entries(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_style = self.project.read(cx).path_style(cx);
        let bulk_staging = self.bulk_staging.take();
        let last_staged_path_prev_index = bulk_staging
            .as_ref()
            .and_then(|op| self.entry_by_path(&op.anchor));

        self.active_repository = self.project.read(cx).active_repository(cx);
        self.entries.clear();
        self.entries_indices.clear();
        self.single_staged_entry.take();
        self.single_tracked_entry.take();
        self.conflicted_count = 0;
        self.conflicted_staged_count = 0;
        self.changes_count = 0;
        self.diff_stat_total = DiffStat::default();
        self.new_count = 0;
        self.tracked_count = 0;
        self.new_staged_count = 0;
        self.tracked_staged_count = 0;
        self.entry_count = 0;
        self.max_width_item_index = None;
        self.git_access = GitAccess::Yes;

        let settings = GitPanelSettings::get_global(cx);
        let sort_by = settings.sort_by;
        let group_by_status = settings.group_by == GitPanelGroupBy::Status;
        let is_tree_view = matches!(self.view_mode, GitPanelViewMode::Tree(_));

        if let Some(active_repo) = self.active_repository.as_ref() {
            let access = active_repo.update(cx, |active_repo, cx| active_repo.access(cx));

            cx.spawn_in(window, async move |git_panel, cx| {
                // When the user does not own the `.git` folder, the
                // `GitStore.spawn_local_git_worker` will fail to create the
                // receiver for Git jobs, so this access check will be
                // cancelled.
                //
                // We assume `GitAccess::No` on cancellation. I believe this is
                // imprecise, other failures could also cause cancellation, but
                // the consequence is just showing the "unsafe repo" UI, which
                // seems acceptable for this edge case.
                let access = match access.await {
                    Ok(access) => access,
                    Err(Canceled) => GitAccess::No,
                };

                git_panel.update(cx, |this, _cx| {
                    this.git_access = access;
                })
            })
            .detach_and_log_err(cx);
        }

        let mut changed_entries = Vec::new();
        let mut new_entries = Vec::new();
        let mut conflict_entries = Vec::new();
        let mut single_staged_entry = None;
        let mut staged_count = 0;
        let mut seen_directories = HashSet::default();
        let mut max_width_estimate = 0usize;
        let mut max_width_item_index = None;

        let Some(repo) = self.active_repository.as_ref() else {
            // Just clear entries if no repository is active.
            cx.notify();
            return;
        };

        let repo = repo.read(cx);

        self.stash_entries = repo.cached_stash();

        for entry in repo.cached_status() {
            self.changes_count += 1;
            let is_conflict = repo.had_conflict_on_last_merge_head_change(&entry.repo_path);
            let is_new = entry.status.is_created();
            let staging = entry.status.staging();

            if let Some(pending) = repo.pending_ops_for_path(&entry.repo_path)
                && pending
                    .ops
                    .iter()
                    .any(|op| op.git_status == pending_op::GitStatus::Reverted && op.finished())
            {
                continue;
            }

            let entry = GitStatusEntry {
                repo_path: entry.repo_path.clone(),
                status: entry.status,
                staging,
                diff_stat: entry.diff_stat,
            };

            if staging.has_staged() {
                staged_count += 1;
                single_staged_entry = Some(entry.clone());
            }

            if group_by_status && is_conflict {
                conflict_entries.push(entry);
            } else if group_by_status && is_new {
                new_entries.push(entry);
            } else {
                changed_entries.push(entry);
            }
        }

        if conflict_entries.is_empty() {
            if staged_count == 1
                && let Some(entry) = single_staged_entry.as_ref()
            {
                if let Some(ops) = repo.pending_ops_for_path(&entry.repo_path) {
                    if ops.staged() {
                        self.single_staged_entry = single_staged_entry;
                    }
                } else {
                    self.single_staged_entry = single_staged_entry;
                }
            } else if repo.pending_ops_summary().item_summary.staging_count == 1
                && let Some(ops) = repo.pending_ops().find(|ops| ops.staging())
            {
                self.single_staged_entry =
                    repo.status_for_path(&ops.repo_path)
                        .map(|status| GitStatusEntry {
                            repo_path: ops.repo_path.clone(),
                            status: status.status,
                            staging: StageStatus::Staged,
                            diff_stat: status.diff_stat,
                        });
            }
        }

        if conflict_entries.is_empty() && changed_entries.len() == 1 {
            self.single_tracked_entry = changed_entries.first().cloned();
        }

        if !is_tree_view {
            let sort_entries = |entries: &mut Vec<GitStatusEntry>| match sort_by {
                GitPanelSortBy::Path => entries.sort_by(|a, b| a.repo_path.cmp(&b.repo_path)),
                GitPanelSortBy::Name => entries.sort_by(|a, b| {
                    a.repo_path
                        .file_name()
                        .cmp(&b.repo_path.file_name())
                        .then_with(|| a.repo_path.cmp(&b.repo_path))
                }),
            };

            sort_entries(&mut conflict_entries);
            sort_entries(&mut changed_entries);
            sort_entries(&mut new_entries);
        }

        let mut push_entry =
            |this: &mut Self,
             entry: GitListEntry,
             is_visible: bool,
             logical_indices: Option<&mut Vec<usize>>| {
                if let Some(estimate) =
                    this.width_estimate_for_list_entry(is_tree_view, &entry, path_style)
                {
                    if estimate > max_width_estimate {
                        max_width_estimate = estimate;
                        max_width_item_index = Some(this.entries.len());
                    }
                }

                if let Some(repo_path) = entry.status_entry().map(|status| status.repo_path.clone())
                {
                    this.entries_indices.insert(repo_path, this.entries.len());
                }

                if let (Some(indices), true) = (logical_indices, is_visible) {
                    indices.push(this.entries.len());
                }

                this.entries.push(entry);
            };

        macro_rules! take_section_entries {
            () => {
                [
                    (Section::Conflict, std::mem::take(&mut conflict_entries)),
                    (Section::Tracked, std::mem::take(&mut changed_entries)),
                    (Section::New, std::mem::take(&mut new_entries)),
                ]
            };
        }

        match &mut self.view_mode {
            GitPanelViewMode::Tree(tree_state) => {
                tree_state.logical_indices.clear();
                tree_state.directory_descendants.clear();

                // This is just to get around the borrow checker
                // because push_entry mutably borrows self
                let mut tree_state = std::mem::take(tree_state);

                for (section, entries) in take_section_entries!() {
                    if entries.is_empty() {
                        continue;
                    }

                    if section != Section::Tracked || group_by_status {
                        push_entry(
                            self,
                            GitListEntry::Header(GitHeaderEntry { header: section }),
                            true,
                            Some(&mut tree_state.logical_indices),
                        );
                    }

                    for (entry, is_visible) in
                        tree_state.build_tree_entries(section, entries, &mut seen_directories)
                    {
                        push_entry(
                            self,
                            entry,
                            is_visible,
                            Some(&mut tree_state.logical_indices),
                        );
                    }
                }

                let seen_directory_paths = seen_directories
                    .iter()
                    .map(|directory| directory.path.clone())
                    .collect::<HashSet<_>>();
                tree_state
                    .expanded_dirs
                    .retain(|path, _| seen_directory_paths.contains(path));
                self.tree_expanded_dirs = tree_state.expanded_dirs.clone();
                self.view_mode = GitPanelViewMode::Tree(tree_state);
            }
            GitPanelViewMode::Flat => {
                for (section, entries) in take_section_entries!() {
                    if entries.is_empty() {
                        continue;
                    }

                    if section != Section::Tracked || group_by_status {
                        push_entry(
                            self,
                            GitListEntry::Header(GitHeaderEntry { header: section }),
                            true,
                            None,
                        );
                    }

                    for entry in entries {
                        push_entry(self, GitListEntry::Status(entry), true, None);
                    }
                }
            }
        }

        self.max_width_item_index = max_width_item_index;

        self.update_counts(repo);

        let bulk_staging_anchor_new_index = bulk_staging
            .as_ref()
            .filter(|op| op.repo_id == repo.id)
            .and_then(|op| self.entry_by_path(&op.anchor));
        if bulk_staging_anchor_new_index == last_staged_path_prev_index
            && let Some(index) = bulk_staging_anchor_new_index
            && let Some(entry) = self.entries.get(index)
            && let Some(entry) = entry.status_entry()
            && GitPanel::stage_status_for_entry(entry, &repo)
                .as_bool()
                .unwrap_or(false)
        {
            self.bulk_staging = bulk_staging;
        }

        self.select_first_entry_if_none(window, cx);
        self.select_last_entry_if_out_of_bounds(window, cx);

        let suggested_commit_message = self.suggest_commit_message(cx);
        let placeholder_text = suggested_commit_message.unwrap_or("Enter commit message".into());

        self.commit_editor.update(cx, |editor, cx| {
            editor.set_placeholder_text(&placeholder_text, window, cx)
        });

        cx.notify();
    }
}
