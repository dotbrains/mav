use super::*;

impl GitStore {
    pub fn repositories(&self) -> &HashMap<RepositoryId, Entity<Repository>> {
        &self.repositories
    }

    /// Returns the main repository working directory for the given worktree.
    /// For normal checkouts this equals the worktree's own path. For linked
    /// worktrees it points back to the main worktree, if one exists. Linked
    /// worktrees attached to a bare repository have no main worktree path.
    pub fn original_repo_path_for_worktree(
        &self,
        worktree_id: WorktreeId,
        cx: &App,
    ) -> Option<Arc<Path>> {
        self.active_repo_id
            .iter()
            .chain(self.worktree_ids.keys())
            .find(|repo_id| {
                self.worktree_ids
                    .get(repo_id)
                    .is_some_and(|ids| ids.contains(&worktree_id))
            })
            .and_then(|repo_id| self.repositories.get(repo_id))
            .and_then(|repo| {
                repo.read(cx)
                    .snapshot()
                    .main_worktree_abs_path()
                    .map(Arc::from)
            })
    }

    pub fn status_for_buffer_id(&self, buffer_id: BufferId, cx: &App) -> Option<FileStatus> {
        let (repo, path) = self.repository_and_path_for_buffer_id(buffer_id, cx)?;
        let status = repo.read(cx).snapshot.status_for_path(&path)?;
        Some(status.status)
    }

    pub fn repository_and_path_for_buffer_id(
        &self,
        buffer_id: BufferId,
        cx: &App,
    ) -> Option<(Entity<Repository>, RepoPath)> {
        let buffer = self.buffer_store.read(cx).get(buffer_id)?;
        let project_path = buffer.read(cx).project_path(cx)?;
        self.repository_and_path_for_project_path(&project_path, cx)
    }

    pub fn repository_and_path_for_project_path(
        &self,
        path: &ProjectPath,
        cx: &App,
    ) -> Option<(Entity<Repository>, RepoPath)> {
        let abs_path = self.worktree_store.read(cx).absolutize(path, cx)?;
        self.repositories
            .values()
            .filter_map(|repo| {
                let repo_path = repo.read(cx).abs_path_to_repo_path(&abs_path)?;
                Some((repo.clone(), repo_path))
            })
            .max_by_key(|(repo, _)| repo.read(cx).work_directory_abs_path.clone())
    }

    pub fn git_init(
        &self,
        path: Arc<Path>,
        fallback_branch_name: String,
        cx: &App,
    ) -> Task<Result<()>> {
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_init(&path, fallback_branch_name).await })
            }
            GitStoreState::Remote {
                upstream_client,
                upstream_project_id: project_id,
                ..
            } => {
                let client = upstream_client.clone();
                let project_id = *project_id;
                cx.background_executor().spawn(async move {
                    client
                        .request(proto::GitInit {
                            project_id: project_id,
                            abs_path: path.to_string_lossy().into_owned(),
                            fallback_branch_name,
                        })
                        .await?;
                    Ok(())
                })
            }
        }
    }

    pub fn git_clone(
        &self,
        repo: String,
        path: impl Into<Arc<std::path::Path>>,
        cx: &App,
    ) -> Task<Result<()>> {
        let path = path.into();
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_clone(&path, &repo).await })
            }
            GitStoreState::Remote {
                upstream_client,
                upstream_project_id,
                ..
            } => {
                if upstream_client.is_via_collab() {
                    return Task::ready(Err(anyhow!(
                        "Git Clone isn't supported for project guests"
                    )));
                }
                let request = upstream_client.request(proto::GitClone {
                    project_id: *upstream_project_id,
                    abs_path: path.to_string_lossy().into_owned(),
                    remote_repo: repo,
                });

                cx.background_spawn(async move {
                    let result = request.await?;

                    match result.success {
                        true => Ok(()),
                        false => Err(anyhow!("Git Clone failed")),
                    }
                })
            }
        }
    }

    pub fn git_config(&self, path: Arc<Path>, args: Vec<String>, cx: &App) -> Task<Result<String>> {
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_config(&path, args).await })
            }
            GitStoreState::Remote {
                upstream_client, ..
            } => {
                // Prevent running git config commands for collab.
                if upstream_client.is_via_collab() {
                    return Task::ready(Err(anyhow!(
                        "Git Config isn't support for project guests"
                    )));
                }

                // TODO: Implement this for remote repositories.
                Task::ready(Err(anyhow!(
                    "Git Config isn't yet supported for remote projects"
                )))
            }
        }
    }
}
