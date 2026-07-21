use super::*;

impl Repository {
    pub fn set_as_active_repository(&self, cx: &mut Context<Self>) {
        let Some(git_store) = self.git_store.upgrade() else {
            return;
        };
        let entity = cx.entity();
        git_store.update(cx, |git_store, cx| {
            let Some((&id, _)) = git_store
                .repositories
                .iter()
                .find(|(_, handle)| *handle == &entity)
            else {
                return;
            };
            git_store.active_repo_id = Some(id);
            cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
        });
    }

    pub fn cached_status(&self) -> impl '_ + Iterator<Item = StatusEntry> {
        self.snapshot.status()
    }

    pub fn diff_stat_for_path(&self, path: &RepoPath) -> Option<DiffStat> {
        self.snapshot.diff_stat_for_path(path)
    }

    pub fn cached_stash(&self) -> GitStash {
        self.snapshot.stash_entries.clone()
    }

    pub fn repo_path_to_project_path(&self, path: &RepoPath, cx: &App) -> Option<ProjectPath> {
        let git_store = self.git_store.upgrade()?;
        let worktree_store = git_store.read(cx).worktree_store.read(cx);
        let abs_path = self.snapshot.repo_path_to_abs_path(path);
        let abs_path = SanitizedPath::new(&abs_path);
        let (worktree, relative_path) = worktree_store.find_worktree(abs_path, cx)?;
        Some(ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path: relative_path,
        })
    }

    pub fn project_path_to_repo_path(&self, path: &ProjectPath, cx: &App) -> Option<RepoPath> {
        let git_store = self.git_store.upgrade()?;
        let worktree_store = git_store.read(cx).worktree_store.read(cx);
        let abs_path = worktree_store.absolutize(path, cx)?;
        self.snapshot.abs_path_to_repo_path(&abs_path)
    }

    pub fn contains_sub_repo(&self, other: &Entity<Self>, cx: &App) -> bool {
        other
            .read(cx)
            .snapshot
            .work_directory_abs_path
            .starts_with(&self.snapshot.work_directory_abs_path)
    }

    pub fn commit_message_buffer(&self) -> Option<&Entity<Buffer>> {
        self.commit_message_buffer.as_ref()
    }

    pub fn open_commit_buffer(
        &mut self,
        languages: Option<Arc<LanguageRegistry>>,
        buffer_store: Entity<BufferStore>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        let id = self.id;
        if let Some(buffer) = self.commit_message_buffer.clone() {
            return Task::ready(Ok(buffer));
        }
        let this = cx.weak_entity();

        let rx = self.send_job(
            "open_commit_buffer",
            None,
            move |state, mut cx| async move {
                let Some(this) = this.upgrade() else {
                    bail!("git store was dropped");
                };
                match state {
                    RepositoryState::Local(..) => {
                        this.update(&mut cx, |_, cx| {
                            Self::open_local_commit_buffer(languages, buffer_store, cx)
                        })
                        .await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        let request = client.request(proto::OpenCommitMessageBuffer {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        });
                        let response = request.await.context("requesting to open commit buffer")?;
                        let buffer_id = BufferId::new(response.buffer_id)?;
                        let buffer = buffer_store
                            .update(&mut cx, |buffer_store, cx| {
                                buffer_store.wait_for_remote_buffer(buffer_id, cx)
                            })
                            .await?;
                        if let Some(language_registry) = languages {
                            let git_commit_language =
                                language_registry.language_for_name("Git Commit").await?;
                            buffer.update(&mut cx, |buffer, cx| {
                                buffer.set_language(Some(git_commit_language), cx);
                            });
                        }
                        this.update(&mut cx, |this, _| {
                            this.commit_message_buffer = Some(buffer.clone());
                        });
                        Ok(buffer)
                    }
                }
            },
        );

        cx.spawn(|_, _: &mut AsyncApp| async move { rx.await? })
    }

    fn open_local_commit_buffer(
        language_registry: Option<Arc<LanguageRegistry>>,
        buffer_store: Entity<BufferStore>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        cx.spawn(async move |repository, cx| {
            let git_commit_language = match language_registry {
                Some(language_registry) => {
                    Some(language_registry.language_for_name("Git Commit").await?)
                }
                None => None,
            };
            let buffer = buffer_store
                .update(cx, |buffer_store, cx| {
                    buffer_store.create_buffer(git_commit_language, false, cx)
                })
                .await?;

            repository.update(cx, |repository, _| {
                repository.commit_message_buffer = Some(buffer.clone());
            })?;
            Ok(buffer)
        })
    }

    pub fn checkout_files(
        &mut self,
        commit: &str,
        paths: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let commit = commit.to_string();
        let id = self.id;

        self.spawn_job_with_tracking(
            paths.clone(),
            pending_op::GitStatus::Reverted,
            cx,
            async move |this, cx| {
                this.update(cx, |this, _cx| {
                    this.send_job(
                        "checkout_files",
                        Some(format!("git checkout {}", commit).into()),
                        move |git_repo, _| async move {
                            match git_repo {
                                RepositoryState::Local(LocalRepositoryState {
                                    backend,
                                    environment,
                                    ..
                                }) => {
                                    backend
                                        .checkout_files(commit, paths, environment.clone())
                                        .await
                                }
                                RepositoryState::Remote(RemoteRepositoryState {
                                    project_id,
                                    client,
                                }) => {
                                    client
                                        .request(proto::GitCheckoutFiles {
                                            project_id: project_id.0,
                                            repository_id: id.to_proto(),
                                            commit,
                                            paths: paths
                                                .into_iter()
                                                .map(|p| p.to_proto())
                                                .collect(),
                                        })
                                        .await?;

                                    Ok(())
                                }
                            }
                        },
                    )
                })?
                .await?
            },
        )
    }

    pub fn reset(
        &mut self,
        commit: String,
        reset_mode: ResetMode,
        _cx: &mut App,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;

        self.send_job("reset", None, move |git_repo, _| async move {
            match git_repo {
                RepositoryState::Local(LocalRepositoryState {
                    backend,
                    environment,
                    ..
                }) => backend.reset(commit, reset_mode, environment).await,
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    client
                        .request(proto::GitReset {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            commit,
                            mode: match reset_mode {
                                ResetMode::Soft => git_reset::ResetMode::Soft.into(),
                                ResetMode::Mixed => git_reset::ResetMode::Mixed.into(),
                            },
                        })
                        .await?;

                    Ok(())
                }
            }
        })
    }

    pub fn show(&mut self, commit: String) -> oneshot::Receiver<Result<CommitDetails>> {
        let id = self.id;
        self.send_job("show", None, move |git_repo, _cx| async move {
            match git_repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.show(commit).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let resp = client
                        .request(proto::GitShow {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            commit,
                        })
                        .await?;

                    Ok(CommitDetails {
                        sha: resp.sha.into(),
                        message: resp.message.into(),
                        commit_timestamp: resp.commit_timestamp,
                        author_email: resp.author_email.into(),
                        author_name: resp.author_name.into(),
                    })
                }
            }
        })
    }

    pub fn load_commit_diff(&mut self, commit: String) -> oneshot::Receiver<Result<CommitDiff>> {
        let id = self.id;
        self.send_job("load_commit_diff", None, move |git_repo, cx| async move {
            match git_repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.load_commit(commit, cx).await
                }
                RepositoryState::Remote(RemoteRepositoryState {
                    client, project_id, ..
                }) => {
                    let response = client
                        .request(proto::LoadCommitDiff {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            commit,
                        })
                        .await?;
                    Ok(CommitDiff {
                        files: response
                            .files
                            .into_iter()
                            .map(|file| {
                                Ok(CommitFile {
                                    path: RepoPath::from_proto(&file.path)?,
                                    old_text: file.old_text,
                                    new_text: file.new_text,
                                    is_binary: file.is_binary,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?,
                    })
                }
            }
        })
    }

    pub fn file_history_changed_files(
        &mut self,
        paths: Vec<RepoPath>,
        commit_limit: usize,
    ) -> oneshot::Receiver<Result<Vec<FileHistoryChangedFileSets>>> {
        self.send_job(
            "file_history_changed_files",
            None,
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend
                            .file_history_changed_files(paths, commit_limit)
                            .await
                    }
                    RepositoryState::Remote(_) => {
                        anyhow::bail!("file history changed files is only supported locally")
                    }
                }
            },
        )
    }
}
