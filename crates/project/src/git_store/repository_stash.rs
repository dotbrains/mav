use super::*;

impl Repository {
    pub fn stash_all(&mut self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        let to_stash = self.cached_status().map(|entry| entry.repo_path).collect();

        self.stash_entries(to_stash, cx)
    }

    pub fn stash_entries(
        &mut self,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let id = self.id;

        cx.spawn(async move |this, cx| {
            this.update(cx, |this, _| {
                this.send_job("stash_entries", None, move |git_repo, _cx| async move {
                    match git_repo {
                        RepositoryState::Local(LocalRepositoryState {
                            backend,
                            environment,
                            ..
                        }) => backend.stash_paths(entries, environment).await,
                        RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                            client
                                .request(proto::Stash {
                                    project_id: project_id.0,
                                    repository_id: id.to_proto(),
                                    paths: entries
                                        .into_iter()
                                        .map(|repo_path| repo_path.to_proto())
                                        .collect(),
                                })
                                .await?;
                            Ok(())
                        }
                    }
                })
            })?
            .await??;
            Ok(())
        })
    }

    pub fn stash_pop(
        &mut self,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let id = self.id;
        cx.spawn(async move |this, cx| {
            this.update(cx, |this, _| {
                this.send_job("stash_pop", None, move |git_repo, _cx| async move {
                    match git_repo {
                        RepositoryState::Local(LocalRepositoryState {
                            backend,
                            environment,
                            ..
                        }) => backend.stash_pop(index, environment).await,
                        RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                            client
                                .request(proto::StashPop {
                                    project_id: project_id.0,
                                    repository_id: id.to_proto(),
                                    stash_index: index.map(|i| i as u64),
                                })
                                .await
                                .context("sending stash pop request")?;
                            Ok(())
                        }
                    }
                })
            })?
            .await??;
            Ok(())
        })
    }

    pub fn stash_apply(
        &mut self,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let id = self.id;
        cx.spawn(async move |this, cx| {
            this.update(cx, |this, _| {
                this.send_job("stash_apply", None, move |git_repo, _cx| async move {
                    match git_repo {
                        RepositoryState::Local(LocalRepositoryState {
                            backend,
                            environment,
                            ..
                        }) => backend.stash_apply(index, environment).await,
                        RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                            client
                                .request(proto::StashApply {
                                    project_id: project_id.0,
                                    repository_id: id.to_proto(),
                                    stash_index: index.map(|i| i as u64),
                                })
                                .await
                                .context("sending stash apply request")?;
                            Ok(())
                        }
                    }
                })
            })?
            .await??;
            Ok(())
        })
    }

    pub fn add_path_to_gitignore(
        &mut self,
        repo_path: &RepoPath,
        is_dir: bool,
    ) -> oneshot::Receiver<Result<()>> {
        let work_dir = self.snapshot.work_directory_abs_path.clone();
        let path_display = repo_path.as_ref().display(PathStyle::Posix);
        let file_path_str = if is_dir {
            format!("{}/", path_display)
        } else {
            path_display.to_string()
        };

        self.send_job(
            "add_path_to_gitignore",
            None,
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState { fs, .. }) => {
                        super::repository_helpers::append_pattern_to_ignore_file(
                            fs,
                            work_dir.join(".gitignore"),
                            file_path_str,
                        )
                        .await
                    }
                    RepositoryState::Remote(_) => Err(anyhow::anyhow!(
                        "Cannot modify .gitignore on remote repository"
                    )),
                }
            },
        )
    }

    pub fn add_path_to_git_info_exclude(
        &mut self,
        repo_path: &RepoPath,
        is_dir: bool,
    ) -> oneshot::Receiver<Result<()>> {
        let repository_dir = self.snapshot.repository_dir_abs_path.clone();
        let path_display = repo_path.as_ref().display(PathStyle::Posix);
        let file_path_str = if is_dir {
            format!("{}/", path_display)
        } else {
            path_display.to_string()
        };

        self.send_job(
            "add_path_to_git_info_exclude",
            None,
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState { fs, .. }) => {
                        super::repository_helpers::append_pattern_to_ignore_file(
                            fs,
                            repository_dir.join(git::REPO_EXCLUDE),
                            file_path_str,
                        )
                        .await
                    }
                    RepositoryState::Remote(_) => Err(anyhow::anyhow!(
                        "Cannot modify .git/info/exclude on remote repository"
                    )),
                }
            },
        )
    }

    pub fn stash_drop(
        &mut self,
        index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<anyhow::Result<()>> {
        let id = self.id;
        let updates_tx = self
            .git_store()
            .and_then(|git_store| match &git_store.read(cx).state {
                GitStoreState::Local { downstream, .. } => downstream
                    .as_ref()
                    .map(|downstream| downstream.updates_tx.clone()),
                _ => None,
            });
        let this = cx.weak_entity();
        self.send_job("stash_drop", None, move |git_repo, mut cx| async move {
            match git_repo {
                RepositoryState::Local(LocalRepositoryState {
                    backend,
                    environment,
                    ..
                }) => {
                    // TODO would be nice to not have to do this manually
                    let result = backend.stash_drop(index, environment).await;
                    if result.is_ok()
                        && let Ok(stash_entries) = backend.stash_entries().await
                    {
                        let snapshot = this.update(&mut cx, |this, cx| {
                            this.snapshot.stash_entries = stash_entries;
                            cx.emit(RepositoryEvent::StashEntriesChanged);
                            this.snapshot.clone()
                        })?;
                        if let Some(updates_tx) = updates_tx {
                            updates_tx
                                .unbounded_send(DownstreamUpdate::UpdateRepository(snapshot))
                                .ok();
                        }
                    }

                    result
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    client
                        .request(proto::StashDrop {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            stash_index: index.map(|i| i as u64),
                        })
                        .await
                        .context("sending stash pop request")?;
                    Ok(())
                }
            }
        })
    }
}
