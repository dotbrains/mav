use super::*;

impl Repository {
    pub(super) fn clear_pending_ops(&mut self, cx: &mut Context<Self>) {
        let updated = SumTree::from_iter(
            self.pending_ops.iter().filter_map(|ops| {
                let inner_ops: Vec<PendingOp> =
                    ops.ops.iter().filter(|op| op.running()).cloned().collect();
                if inner_ops.is_empty() {
                    None
                } else {
                    Some(PendingOps {
                        repo_path: ops.repo_path.clone(),
                        ops: inner_ops,
                    })
                }
            }),
            (),
        );

        if updated != self.pending_ops {
            cx.emit(RepositoryEvent::PendingOpsChanged {
                pending_ops: self.pending_ops.clone(),
            })
        }

        self.pending_ops = updated;
    }

    pub(super) fn schedule_scan(
        &mut self,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let _ = self.send_keyed_job(
            "schedule_scan",
            Some(GitJobKey::ReloadGitState),
            None,
            |state, mut cx| async move {
                log::debug!("run scheduled git status scan");

                let Some(this) = this.upgrade() else {
                    return Ok(());
                };
                let RepositoryState::Local(LocalRepositoryState { backend, .. }) = state else {
                    bail!("not a local repository")
                };
                let snapshot = compute_snapshot(this.clone(), backend.clone(), &mut cx).await;
                this.update(&mut cx, |this, cx| {
                    this.clear_pending_ops(cx);
                });
                if let Some(updates_tx) = updates_tx {
                    updates_tx
                        .unbounded_send(DownstreamUpdate::UpdateRepository(snapshot))
                        .ok();
                }
                Ok(())
            },
        );
    }

    pub(super) fn spawn_local_git_worker(
        state: Shared<Task<Result<LocalRepositoryState, String>>>,
        cx: &mut Context<Self>,
    ) -> (mpsc::UnboundedSender<GitJob>, Task<()>) {
        let (job_tx, mut job_rx) = mpsc::unbounded::<GitJob>();

        let worker_task = cx.spawn(async move |this, cx| {
            let Some(state) = state.await.log_err() else {
                return;
            };
            if let Some(git_hosting_provider_registry) =
                cx.update(|cx| GitHostingProviderRegistry::try_global(cx))
            {
                git_hosting_providers::register_additional_providers(
                    git_hosting_provider_registry,
                    state.backend.clone(),
                )
                .await;
            }
            let state = RepositoryState::Local(state);
            let mut jobs = VecDeque::new();
            loop {
                while let Ok(next_job) = job_rx.try_recv() {
                    jobs.push_back(next_job);
                }

                if let Some(job) = jobs.pop_front() {
                    if let Some(current_key) = &job.key
                        && jobs
                            .iter()
                            .any(|other_job| other_job.key.as_ref() == Some(current_key))
                    {
                        let skipped_job_id = job.id;
                        this.update(cx, |repo, _| {
                            repo.job_debug_queue.mark_complete(
                                skipped_job_id,
                                job_debug_queue::CompletedJobStatus::Skipped,
                            );
                        })
                        .ok();
                        continue;
                    }
                    (job.job)(state.clone(), cx).await;
                } else if let Some(job) = job_rx.next().await {
                    jobs.push_back(job);
                } else {
                    break;
                }
            }
        });

        (job_tx, worker_task)
    }

    pub(super) fn spawn_remote_git_worker(
        state: RemoteRepositoryState,
        cx: &mut Context<Self>,
    ) -> (mpsc::UnboundedSender<GitJob>, Task<()>) {
        let (job_tx, mut job_rx) = mpsc::unbounded::<GitJob>();

        let worker_task = cx.spawn(async move |this, cx| {
            let result: Result<()> = async {
                let state = RepositoryState::Remote(state);
                let mut jobs = VecDeque::new();
                loop {
                    while let Ok(next_job) = job_rx.try_recv() {
                        jobs.push_back(next_job);
                    }

                    if let Some(job) = jobs.pop_front() {
                        if let Some(current_key) = &job.key
                            && jobs
                                .iter()
                                .any(|other_job| other_job.key.as_ref() == Some(current_key))
                        {
                            let skipped_job_id = job.id;
                            this.update(cx, |repo, _| {
                                repo.job_debug_queue.mark_complete(
                                    skipped_job_id,
                                    job_debug_queue::CompletedJobStatus::Skipped,
                                );
                            })
                            .ok();
                            continue;
                        }
                        (job.job)(state.clone(), cx).await;
                    } else if let Some(job) = job_rx.next().await {
                        jobs.push_back(job);
                    } else {
                        break;
                    }
                }
                anyhow::Ok(())
            }
            .await;
            result.log_err();
        });

        (job_tx, worker_task)
    }
}
