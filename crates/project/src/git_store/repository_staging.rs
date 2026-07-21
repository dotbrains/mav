use super::*;

impl Repository {
    pub(super) fn buffer_store(&self, cx: &App) -> Option<Entity<BufferStore>> {
        Some(self.git_store.upgrade()?.read(cx).buffer_store.clone())
    }

    pub(super) fn save_buffers<'a>(
        &self,
        entries: impl IntoIterator<Item = &'a RepoPath>,
        cx: &mut Context<Self>,
    ) -> Vec<Task<anyhow::Result<()>>> {
        let mut save_futures = Vec::new();
        if let Some(buffer_store) = self.buffer_store(cx) {
            buffer_store.update(cx, |buffer_store, cx| {
                for path in entries {
                    let Some(project_path) = self.repo_path_to_project_path(path, cx) else {
                        continue;
                    };
                    if let Some(buffer) = buffer_store.get_by_path(&project_path)
                        && buffer
                            .read(cx)
                            .file()
                            .is_some_and(|file| file.disk_state().exists())
                        && buffer.read(cx).has_unsaved_edits()
                    {
                        save_futures.push(buffer_store.save_buffer(buffer, cx));
                    }
                }
            })
        }
        save_futures
    }

    pub fn stage_entries(
        &mut self,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.stage_or_unstage_entries(true, entries, cx)
    }

    pub fn unstage_entries(
        &mut self,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.stage_or_unstage_entries(false, entries, cx)
    }

    pub(super) fn stage_or_unstage_entries(
        &mut self,
        stage: bool,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        if entries.is_empty() {
            return Task::ready(Ok(()));
        }
        let Some(git_store) = self.git_store.upgrade() else {
            return Task::ready(Ok(()));
        };
        let id = self.id;
        let save_tasks = self.save_buffers(&entries, cx);
        let paths = entries
            .iter()
            .map(|p| p.as_unix_str())
            .collect::<Vec<_>>()
            .join(" ");
        let status = if stage {
            format!("git add {paths}")
        } else {
            format!("git reset {paths}")
        };
        let job_key = GitJobKey::WriteIndex(entries.clone());

        self.spawn_job_with_tracking(
            entries.clone(),
            if stage {
                pending_op::GitStatus::Staged
            } else {
                pending_op::GitStatus::Unstaged
            },
            cx,
            async move |this, cx| {
                for save_task in save_tasks {
                    save_task.await?;
                }

                this.update(cx, |this, cx| {
                    let weak_this = cx.weak_entity();
                    this.send_keyed_job(
                        "stage_or_unstage_entries",
                        Some(job_key),
                        Some(status.into()),
                        move |git_repo, mut cx| async move {
                            let hunk_staging_operation_counts = weak_this
                                .update(&mut cx, |this, cx| {
                                    let mut hunk_staging_operation_counts = HashMap::default();
                                    for path in &entries {
                                        let Some(project_path) =
                                            this.repo_path_to_project_path(path, cx)
                                        else {
                                            continue;
                                        };
                                        let Some(buffer) = git_store
                                            .read(cx)
                                            .buffer_store
                                            .read(cx)
                                            .get_by_path(&project_path)
                                        else {
                                            continue;
                                        };
                                        let Some(diff_state) = git_store
                                            .read(cx)
                                            .diffs
                                            .get(&buffer.read(cx).remote_id())
                                            .cloned()
                                        else {
                                            continue;
                                        };
                                        let Some(uncommitted_diff) =
                                            diff_state.read(cx).uncommitted_diff.as_ref().and_then(
                                                |uncommitted_diff| uncommitted_diff.upgrade(),
                                            )
                                        else {
                                            continue;
                                        };
                                        let buffer_snapshot = buffer.read(cx).text_snapshot();
                                        let file_exists = buffer
                                            .read(cx)
                                            .file()
                                            .is_some_and(|file| file.disk_state().exists());
                                        let hunk_staging_operation_count =
                                            diff_state.update(cx, |diff_state, cx| {
                                                uncommitted_diff.update(
                                                    cx,
                                                    |uncommitted_diff, cx| {
                                                        uncommitted_diff
                                                            .stage_or_unstage_all_hunks(
                                                                stage,
                                                                &buffer_snapshot,
                                                                file_exists,
                                                                cx,
                                                            );
                                                    },
                                                );

                                                diff_state.hunk_staging_operation_count += 1;
                                                diff_state.hunk_staging_operation_count
                                            });
                                        hunk_staging_operation_counts.insert(
                                            diff_state.downgrade(),
                                            hunk_staging_operation_count,
                                        );
                                    }
                                    hunk_staging_operation_counts
                                })
                                .unwrap_or_default();

                            let result = match git_repo {
                                RepositoryState::Local(LocalRepositoryState {
                                    backend,
                                    environment,
                                    ..
                                }) => {
                                    if stage {
                                        backend.stage_paths(entries, environment.clone()).await
                                    } else {
                                        backend.unstage_paths(entries, environment.clone()).await
                                    }
                                }
                                RepositoryState::Remote(RemoteRepositoryState {
                                    project_id,
                                    client,
                                }) => {
                                    if stage {
                                        client
                                            .request(proto::Stage {
                                                project_id: project_id.0,
                                                repository_id: id.to_proto(),
                                                paths: entries
                                                    .into_iter()
                                                    .map(|repo_path| repo_path.to_proto())
                                                    .collect(),
                                            })
                                            .await
                                            .context("sending stage request")
                                            .map(|_| ())
                                    } else {
                                        client
                                            .request(proto::Unstage {
                                                project_id: project_id.0,
                                                repository_id: id.to_proto(),
                                                paths: entries
                                                    .into_iter()
                                                    .map(|repo_path| repo_path.to_proto())
                                                    .collect(),
                                            })
                                            .await
                                            .context("sending unstage request")
                                            .map(|_| ())
                                    }
                                }
                            };

                            for (diff_state, hunk_staging_operation_count) in
                                hunk_staging_operation_counts
                            {
                                diff_state
                                    .update(&mut cx, |diff_state, cx| {
                                        if result.is_ok() {
                                            diff_state.hunk_staging_operation_count_as_of_write =
                                                hunk_staging_operation_count;
                                        } else if let Some(uncommitted_diff) =
                                            &diff_state.uncommitted_diff
                                        {
                                            uncommitted_diff
                                                .update(cx, |uncommitted_diff, cx| {
                                                    uncommitted_diff.clear_pending_hunks(cx);
                                                })
                                                .ok();
                                        }
                                    })
                                    .ok();
                            }

                            result
                        },
                    )
                })?
                .await?
            },
        )
    }

    pub fn stage_all(&mut self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        let snapshot = self.snapshot.clone();
        let pending_ops = self.pending_ops.clone();
        let to_stage = cx.background_spawn(async move {
            snapshot
                .status()
                .filter_map(|entry| {
                    if let Some(ops) = pending_ops
                        .get(&PathKey(entry.repo_path.as_ref().clone()), ())
                        .filter(|ops| !ops.last_op_errored())
                    {
                        if ops.staging() || ops.staged() {
                            None
                        } else {
                            Some(entry.repo_path)
                        }
                    } else if entry.status.staging().is_fully_staged() {
                        None
                    } else {
                        Some(entry.repo_path)
                    }
                })
                .collect()
        });

        cx.spawn(async move |this, cx| {
            let to_stage = to_stage.await;
            this.update(cx, |this, cx| {
                this.stage_or_unstage_entries(true, to_stage, cx)
            })?
            .await
        })
    }

    pub fn unstage_all(&mut self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        let snapshot = self.snapshot.clone();
        let pending_ops = self.pending_ops.clone();
        let to_unstage = cx.background_spawn(async move {
            snapshot
                .status()
                .filter_map(|entry| {
                    if let Some(ops) = pending_ops
                        .get(&PathKey(entry.repo_path.as_ref().clone()), ())
                        .filter(|ops| !ops.last_op_errored())
                    {
                        if !ops.staging() && !ops.staged() {
                            None
                        } else {
                            Some(entry.repo_path)
                        }
                    } else if entry.status.staging().is_fully_unstaged() {
                        None
                    } else {
                        Some(entry.repo_path)
                    }
                })
                .collect()
        });

        cx.spawn(async move |this, cx| {
            let to_unstage = to_unstage.await;
            this.update(cx, |this, cx| {
                this.stage_or_unstage_entries(false, to_unstage, cx)
            })?
            .await
        })
    }

    pub(super) fn spawn_set_index_text_job(
        &mut self,
        path: RepoPath,
        content: Option<String>,
        hunk_staging_operation_count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<anyhow::Result<()>> {
        let id = self.id;
        let this = cx.weak_entity();
        let git_store = self.git_store.clone();
        let abs_path = self.snapshot.repo_path_to_abs_path(&path);
        self.send_keyed_job(
            "spawn_set_index_text_job",
            Some(GitJobKey::WriteIndex(vec![path.clone()])),
            None,
            move |git_repo, mut cx| async move {
                log::debug!(
                    "start updating index text for buffer {}",
                    path.as_unix_str()
                );

                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        fs,
                        backend,
                        environment,
                        ..
                    }) => {
                        let executable = match fs.metadata(&abs_path).await {
                            Ok(Some(meta)) => meta.is_executable,
                            Ok(None) => false,
                            Err(_err) => false,
                        };
                        backend
                            .set_index_text(path.clone(), content, environment.clone(), executable)
                            .await?;
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::SetIndexText {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                path: path.to_proto(),
                                text: content,
                            })
                            .await?;
                    }
                }
                log::debug!(
                    "finish updating index text for buffer {}",
                    path.as_unix_str()
                );

                if let Some(hunk_staging_operation_count) = hunk_staging_operation_count {
                    let project_path = this
                        .read_with(&cx, |this, cx| this.repo_path_to_project_path(&path, cx))
                        .ok()
                        .flatten();
                    git_store
                        .update(&mut cx, |git_store, cx| {
                            let buffer_id = git_store
                                .buffer_store
                                .read(cx)
                                .get_by_path(&project_path?)?
                                .read(cx)
                                .remote_id();
                            let diff_state = git_store.diffs.get(&buffer_id)?;
                            diff_state.update(cx, |diff_state, _| {
                                diff_state.hunk_staging_operation_count_as_of_write =
                                    hunk_staging_operation_count;
                            });
                            Some(())
                        })
                        .context("Git store dropped")?;
                }
                Ok(())
            },
        )
    }
}
