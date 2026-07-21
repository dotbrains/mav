use super::*;

impl GitStore {
    pub(super) fn repository_is_trusted(
        &self,
        repository_id: RepositoryId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(worktree_ids) = self.worktree_ids.get(&repository_id) else {
            return false;
        };
        let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) else {
            return false;
        };

        worktree_ids.iter().any(|worktree_id| {
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.can_trust(&self.worktree_store, *worktree_id, cx)
            })
        })
    }

    /// Update our list of repositories and schedule git scans in response to a notification from a worktree,
    pub(super) fn update_repositories_from_worktree(
        &mut self,
        worktree_id: WorktreeId,
        project_environment: Entity<ProjectEnvironment>,
        next_repository_id: Arc<AtomicU64>,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        updated_git_repositories: UpdatedGitRepositoriesSet,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) {
        let mut removed_ids = Vec::new();
        for update in updated_git_repositories.iter() {
            if let Some((id, existing)) = self.repositories.iter().find(|(_, repo)| {
                let existing_work_directory_abs_path =
                    repo.read(cx).work_directory_abs_path.clone();
                Some(&existing_work_directory_abs_path)
                    == update.old_work_directory_abs_path.as_ref()
                    || Some(&existing_work_directory_abs_path)
                        == update.new_work_directory_abs_path.as_ref()
            }) {
                let repo_id = *id;
                if let Some(new_work_directory_abs_path) =
                    update.new_work_directory_abs_path.clone()
                {
                    self.worktree_ids
                        .entry(repo_id)
                        .or_insert_with(HashSet::new)
                        .insert(worktree_id);
                    let path_changed = update.old_work_directory_abs_path.as_ref()
                        != update.new_work_directory_abs_path.as_ref();
                    if path_changed
                        && let Some(dot_git_abs_path) = update.dot_git_abs_path.clone()
                        && let Some(repository_dir_abs_path) =
                            update.repository_dir_abs_path.clone()
                        && let Some(common_dir_abs_path) = update.common_dir_abs_path.clone()
                    {
                        let is_trusted = TrustedWorktrees::try_get_global(cx)
                            .map(|trusted_worktrees| {
                                trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                                    trusted_worktrees.can_trust(
                                        &self.worktree_store,
                                        worktree_id,
                                        cx,
                                    )
                                })
                            })
                            .unwrap_or(false);
                        existing.update(cx, |existing, cx| {
                            existing.reinitialize_local_backend(
                                new_work_directory_abs_path,
                                dot_git_abs_path,
                                repository_dir_abs_path,
                                common_dir_abs_path,
                                project_environment.downgrade(),
                                fs.clone(),
                                is_trusted,
                                cx,
                            );
                            existing.schedule_scan(updates_tx.clone(), cx);
                        });
                    } else {
                        existing.update(cx, |existing, cx| {
                            existing.snapshot.work_directory_abs_path = new_work_directory_abs_path;
                            existing.schedule_scan(updates_tx.clone(), cx);
                        });
                    }
                } else {
                    if let Some(worktree_ids) = self.worktree_ids.get_mut(&repo_id) {
                        worktree_ids.remove(&worktree_id);
                        if worktree_ids.is_empty() {
                            removed_ids.push(repo_id);
                        }
                    }
                }
            } else if let UpdatedGitRepository {
                new_work_directory_abs_path: Some(work_directory_abs_path),
                dot_git_abs_path: Some(dot_git_abs_path),
                repository_dir_abs_path: Some(repository_dir_abs_path),
                common_dir_abs_path: Some(common_dir_abs_path),
                ..
            } = update
            {
                let repository_dir_abs_path = repository_dir_abs_path.clone();
                let common_dir_abs_path = common_dir_abs_path.clone();
                let id = RepositoryId(next_repository_id.fetch_add(1, atomic::Ordering::Release));
                let is_trusted = TrustedWorktrees::try_get_global(cx)
                    .map(|trusted_worktrees| {
                        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                            trusted_worktrees.can_trust(&self.worktree_store, worktree_id, cx)
                        })
                    })
                    .unwrap_or(false);
                let git_store = cx.weak_entity();
                let repo = cx.new(|cx| {
                    let mut repo = Repository::local(
                        id,
                        work_directory_abs_path.clone(),
                        repository_dir_abs_path.clone(),
                        common_dir_abs_path.clone(),
                        dot_git_abs_path.clone(),
                        project_environment.downgrade(),
                        fs.clone(),
                        is_trusted,
                        git_store,
                        cx,
                    );
                    if let Some(updates_tx) = updates_tx.as_ref() {
                        // trigger an empty `UpdateRepository` to ensure remote active_repo_id is set correctly
                        updates_tx
                            .unbounded_send(DownstreamUpdate::UpdateRepository(repo.snapshot()))
                            .ok();
                    }
                    repo.schedule_scan(updates_tx.clone(), cx);
                    repo
                });
                self._subscriptions
                    .push(cx.subscribe(&repo, Self::on_repository_event));
                self._subscriptions
                    .push(cx.subscribe(&repo, Self::on_jobs_updated));
                self.repositories.insert(id, repo);
                self.worktree_ids.insert(id, HashSet::from([worktree_id]));
                cx.emit(GitStoreEvent::RepositoryAdded);
                self.active_repo_id.get_or_insert_with(|| {
                    cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
                    id
                });
            }
        }

        for id in removed_ids {
            if self.active_repo_id == Some(id) {
                self.active_repo_id = None;
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
            }
            self.repositories.remove(&id);
            if let Some(updates_tx) = updates_tx.as_ref() {
                updates_tx
                    .unbounded_send(DownstreamUpdate::RemoveRepository(id))
                    .ok();
            }
        }
    }

    pub(super) fn on_trusted_worktrees_event(
        &mut self,
        _: Entity<TrustedWorktreesStore>,
        event: &TrustedWorktreesEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.state, GitStoreState::Local { .. }) {
            return;
        }

        let (is_trusted, event_paths) = match event {
            TrustedWorktreesEvent::Trusted(_, trusted_paths) => (true, trusted_paths),
            TrustedWorktreesEvent::Restricted(_, restricted_paths) => (false, restricted_paths),
        };

        for (repo_id, worktree_ids) in &self.worktree_ids {
            if worktree_ids
                .iter()
                .any(|worktree_id| event_paths.contains(&PathTrust::Worktree(*worktree_id)))
            {
                if let Some(repo) = self.repositories.get(repo_id) {
                    let repository_state = repo.read(cx).repository_state.clone();
                    cx.background_spawn(async move {
                        if let Ok(RepositoryState::Local(state)) = repository_state.await {
                            state.backend.set_trusted(is_trusted);
                        }
                    })
                    .detach();
                }
            }
        }
    }

    pub(super) fn on_buffer_store_event(
        &mut self,
        _: Entity<BufferStore>,
        event: &BufferStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                cx.subscribe(buffer, |this, buffer, event, cx| {
                    if let BufferEvent::LanguageChanged(_) = event {
                        let buffer_id = buffer.read(cx).remote_id();
                        if let Some(diff_state) = this.diffs.get(&buffer_id) {
                            diff_state.update(cx, |diff_state, cx| {
                                diff_state.buffer_language_changed(buffer, cx);
                            });
                        }
                    }
                })
                .detach();
            }
            BufferStoreEvent::SharedBufferClosed(peer_id, buffer_id) => {
                if let Some(diffs) = self.shared_diffs.get_mut(peer_id) {
                    diffs.remove(buffer_id);
                }
            }
            BufferStoreEvent::BufferDropped(buffer_id) => {
                self.diffs.remove(buffer_id);
                for diffs in self.shared_diffs.values_mut() {
                    diffs.remove(buffer_id);
                }
            }
            BufferStoreEvent::BufferChangedFilePath { buffer, .. } => {
                // Whenever a buffer's file path changes, it's possible that the
                // new path is actually a path that is being tracked by a git
                // repository. In that case, we'll want to update the buffer's
                // `BufferDiffState`, in case it already has one.
                let buffer_id = buffer.read(cx).remote_id();
                let diff_state = self.diffs.get(&buffer_id);
                let repo = self.repository_and_path_for_buffer_id(buffer_id, cx);

                if let Some(diff_state) = diff_state
                    && let Some((repo, repo_path)) = repo
                {
                    let buffer = buffer.clone();
                    let diff_state = diff_state.clone();
                    let is_symlink = Self::buffer_is_symlink(&buffer, cx);

                    cx.spawn(async move |_git_store, cx| {
                        async {
                            let diff_bases_change = if is_symlink {
                                DiffBasesChange::SetBoth(None)
                            } else {
                                repo.update(cx, |repo, cx| {
                                    repo.load_committed_text(buffer_id, repo_path, cx)
                                })
                                .await?
                            };

                            diff_state.update(cx, |diff_state, cx| {
                                let buffer_snapshot = buffer.read(cx).text_snapshot();
                                diff_state.diff_bases_changed(
                                    buffer_snapshot,
                                    Some(diff_bases_change),
                                    cx,
                                );
                            });
                            anyhow::Ok(())
                        }
                        .await
                        .log_err();
                    })
                    .detach();
                }
            }
        }
    }

    pub fn recalculate_buffer_diffs(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let mut futures = Vec::new();
        for buffer in buffers {
            if let Some(diff_state) = self.diffs.get_mut(&buffer.read(cx).remote_id()) {
                let buffer = buffer.read(cx).text_snapshot();
                diff_state.update(cx, |diff_state, cx| {
                    diff_state.recalculate_diffs(buffer.clone(), cx);
                    futures.extend(diff_state.wait_for_recalculation().map(FutureExt::boxed));
                });
                futures.push(diff_state.update(cx, |diff_state, cx| {
                    diff_state
                        .reparse_conflict_markers(buffer, cx)
                        .map(|_| {})
                        .boxed()
                }));
            }
        }
        async move {
            futures::future::join_all(futures).await;
        }
    }

    pub(super) fn on_buffer_diff_event(
        &mut self,
        diff: Entity<buffer_diff::BufferDiff>,
        event: &BufferDiffEvent,
        cx: &mut Context<Self>,
    ) {
        if let BufferDiffEvent::HunksStagedOrUnstaged(new_index_text) = event {
            let buffer_id = diff.read(cx).buffer_id;
            if let Some(diff_state) = self.diffs.get(&buffer_id) {
                let new_index_text = new_index_text.as_ref().map(|rope| rope.to_string());
                if new_index_text.as_deref() == diff_state.read(cx).index_text.as_deref() {
                    return;
                }
                let hunk_staging_operation_count = diff_state.update(cx, |diff_state, _| {
                    diff_state.hunk_staging_operation_count += 1;
                    diff_state.hunk_staging_operation_count
                });
                if let Some((repo, path)) = self.repository_and_path_for_buffer_id(buffer_id, cx) {
                    let recv = repo.update(cx, |repo, cx| {
                        log::debug!("hunks changed for {}", path.as_unix_str());
                        repo.spawn_set_index_text_job(
                            path,
                            new_index_text,
                            Some(hunk_staging_operation_count),
                            cx,
                        )
                    });
                    let diff = diff.downgrade();
                    cx.spawn(async move |this, cx| {
                        if let Ok(Err(error)) = cx.background_spawn(recv).await {
                            diff.update(cx, |diff, cx| {
                                diff.clear_pending_hunks(cx);
                            })
                            .ok();
                            this.update(cx, |_, cx| cx.emit(GitStoreEvent::IndexWriteError(error)))
                                .ok();
                        }
                    })
                    .detach();
                }
            }
        }
    }

    pub(super) fn local_worktree_git_repos_changed(
        &mut self,
        worktree: Entity<Worktree>,
        changed_repos: &UpdatedGitRepositoriesSet,
        cx: &mut Context<Self>,
    ) {
        log::debug!("local worktree repos changed");
        debug_assert!(worktree.read(cx).is_local());

        for repository in self.repositories.values() {
            repository.update(cx, |repository, cx| {
                let repo_abs_path = &repository.work_directory_abs_path;
                if changed_repos.iter().any(|update| {
                    update.old_work_directory_abs_path.as_ref() == Some(repo_abs_path)
                        || update.new_work_directory_abs_path.as_ref() == Some(repo_abs_path)
                }) {
                    repository.reload_buffer_diff_bases(cx);
                }
            });
        }
    }
}
