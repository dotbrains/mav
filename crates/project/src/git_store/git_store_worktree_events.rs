use super::*;

impl GitStore {
    pub(super) fn on_worktree_store_event(
        &mut self,
        worktree_store: Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        let GitStoreState::Local {
            project_environment,
            downstream,
            next_repository_id,
            fs,
            ..
        } = &self.state
        else {
            return;
        };

        match event {
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, updated_entries) => {
                if let Some(worktree) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(*worktree_id, cx)
                {
                    let paths_by_git_repo =
                        self.process_updated_entries(&worktree, updated_entries, cx);
                    let downstream = downstream
                        .as_ref()
                        .map(|downstream| downstream.updates_tx.clone());
                    cx.spawn(async move |_, cx| {
                        let paths_by_git_repo = paths_by_git_repo.await;
                        for (repo, paths) in paths_by_git_repo {
                            repo.update(cx, |repo, cx| {
                                repo.paths_changed(paths, downstream.clone(), cx);
                            });
                        }
                    })
                    .detach();
                }
            }
            WorktreeStoreEvent::WorktreeUpdatedGitRepositories(worktree_id, changed_repos) => {
                let Some(worktree) = worktree_store.read(cx).worktree_for_id(*worktree_id, cx)
                else {
                    return;
                };
                log::debug!("received worktree update for repositories: {changed_repos:?}");
                self.update_repositories_from_worktree(
                    *worktree_id,
                    project_environment.clone(),
                    next_repository_id.clone(),
                    downstream
                        .as_ref()
                        .map(|downstream| downstream.updates_tx.clone()),
                    changed_repos.clone(),
                    fs.clone(),
                    cx,
                );
                self.local_worktree_git_repos_changed(worktree, changed_repos, cx);
            }
            WorktreeStoreEvent::WorktreeRemoved(_entity_id, worktree_id) => {
                let repos_without_worktree: Vec<RepositoryId> = self
                    .worktree_ids
                    .iter_mut()
                    .filter_map(|(repo_id, worktree_ids)| {
                        worktree_ids.remove(worktree_id);
                        if worktree_ids.is_empty() {
                            Some(*repo_id)
                        } else {
                            None
                        }
                    })
                    .collect();
                let is_active_repo_removed = repos_without_worktree
                    .iter()
                    .any(|repo_id| self.active_repo_id == Some(*repo_id));

                for repo_id in repos_without_worktree {
                    self.repositories.remove(&repo_id);
                    self.worktree_ids.remove(&repo_id);
                    if let Some(updates_tx) =
                        downstream.as_ref().map(|downstream| &downstream.updates_tx)
                    {
                        updates_tx
                            .unbounded_send(DownstreamUpdate::RemoveRepository(repo_id))
                            .ok();
                    }
                }

                if is_active_repo_removed {
                    if let Some((&repo_id, _)) = self.repositories.iter().next() {
                        self.active_repo_id = Some(repo_id);
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(repo_id)));
                    } else {
                        self.active_repo_id = None;
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn on_repository_event(
        &mut self,
        repo: Entity<Repository>,
        event: &RepositoryEvent,
        cx: &mut Context<Self>,
    ) {
        let id = repo.read(cx).id;
        let repo_snapshot = repo.read(cx).snapshot.clone();
        for (buffer_id, diff) in self.diffs.iter() {
            if let Some((buffer_repo, repo_path)) =
                self.repository_and_path_for_buffer_id(*buffer_id, cx)
                && buffer_repo == repo
            {
                diff.update(cx, |diff, cx| {
                    if let Some(conflict_set) = &diff.conflict_set {
                        let conflict_status_changed =
                            conflict_set.update(cx, |conflict_set, cx| {
                                let has_conflict = repo_snapshot.has_conflict(&repo_path);
                                conflict_set.set_has_conflict(has_conflict, cx)
                            })?;
                        if conflict_status_changed {
                            let buffer_store = self.buffer_store.read(cx);
                            if let Some(buffer) = buffer_store.get(*buffer_id) {
                                let _ = diff
                                    .reparse_conflict_markers(buffer.read(cx).text_snapshot(), cx);
                            }
                        }
                    }
                    anyhow::Ok(())
                })
                .ok();
            }
        }
        cx.emit(GitStoreEvent::RepositoryUpdated(
            id,
            event.clone(),
            self.active_repo_id == Some(id),
        ))
    }

    pub(super) fn on_jobs_updated(
        &mut self,
        _: Entity<Repository>,
        _: &JobsUpdated,
        cx: &mut Context<Self>,
    ) {
        cx.emit(GitStoreEvent::JobsUpdated)
    }
}
