use super::*;

impl GitStore {
    pub(super) async fn handle_update_repository(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateRepository>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let path_style = this.worktree_store.read(cx).path_style();
            let mut update = envelope.payload;

            let id = RepositoryId::from_proto(update.id);
            let client = this.upstream_client().context("no upstream client")?;

            let repository_dir_abs_path: Option<Arc<Path>> = update
                .repository_dir_abs_path
                .as_deref()
                .map(|p| Path::new(p).into());
            let common_dir_abs_path: Option<Arc<Path>> = update
                .common_dir_abs_path
                .as_deref()
                .map(|p| Path::new(p).into());

            let mut repo_subscription = None;
            let repo = this.repositories.entry(id).or_insert_with(|| {
                let git_store = cx.weak_entity();
                let repo = cx.new(|cx| {
                    Repository::remote(
                        id,
                        Path::new(&update.abs_path).into(),
                        repository_dir_abs_path.clone(),
                        common_dir_abs_path.clone(),
                        path_style,
                        ProjectId(update.project_id),
                        client,
                        git_store,
                        cx,
                    )
                });
                repo_subscription = Some(cx.subscribe(&repo, Self::on_repository_event));
                cx.emit(GitStoreEvent::RepositoryAdded);
                repo
            });
            this._subscriptions.extend(repo_subscription);

            repo.update(cx, {
                let update = update.clone();
                |repo, cx| repo.apply_remote_update(update, cx)
            })?;

            this.active_repo_id.get_or_insert_with(|| {
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
                id
            });

            if let Some((client, project_id)) = this.downstream_client() {
                update.project_id = project_id.to_proto();
                client.send(update).log_err();
            }
            Ok(())
        })
    }

    pub(super) async fn handle_remove_repository(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RemoveRepository>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let mut update = envelope.payload;
            let id = RepositoryId::from_proto(update.id);
            this.repositories.remove(&id);
            if let Some((client, project_id)) = this.downstream_client() {
                update.project_id = project_id.to_proto();
                client.send(update).log_err();
            }
            if this.active_repo_id == Some(id) {
                this.active_repo_id = None;
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
            }
            cx.emit(GitStoreEvent::RepositoryRemoved(id));
        });
        Ok(())
    }

    pub(super) async fn handle_git_init(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitInit>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let path: Arc<Path> = PathBuf::from(envelope.payload.abs_path).into();
        let name = envelope.payload.fallback_branch_name;
        cx.update(|cx| this.read(cx).git_init(path, name, cx))
            .await?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_git_clone(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitClone>,
        cx: AsyncApp,
    ) -> Result<proto::GitCloneResponse> {
        let path: Arc<Path> = PathBuf::from(envelope.payload.abs_path).into();
        let repo_name = envelope.payload.remote_repo;
        let result = cx
            .update(|cx| this.read(cx).git_clone(repo_name, path, cx))
            .await;

        Ok(proto::GitCloneResponse {
            success: result.is_ok(),
        })
    }

    pub(super) async fn handle_fetch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Fetch>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let fetch_options = FetchOptions::from_proto(envelope.payload.remote);
        let askpass_id = envelope.payload.askpass_id;

        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let remote_output = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.fetch(fetch_options, askpass, cx)
            })
            .await??;

        Ok(proto::RemoteMessageResponse {
            stdout: remote_output.stdout,
            stderr: remote_output.stderr,
        })
    }

    pub(super) async fn handle_push(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Push>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let askpass_id = envelope.payload.askpass_id;
        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let options = envelope
            .payload
            .options
            .as_ref()
            .map(|_| match envelope.payload.options() {
                proto::push::PushOptions::SetUpstream => git::repository::PushOptions::SetUpstream,
                proto::push::PushOptions::Force => git::repository::PushOptions::Force,
            });

        let branch_name = envelope.payload.branch_name.into();
        let remote_branch_name = envelope.payload.remote_branch_name.into();
        let remote_name = envelope.payload.remote_name.into();

        let remote_output = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.push(
                    branch_name,
                    remote_branch_name,
                    remote_name,
                    options,
                    askpass,
                    cx,
                )
            })
            .await??;
        Ok(proto::RemoteMessageResponse {
            stdout: remote_output.stdout,
            stderr: remote_output.stderr,
        })
    }

    pub(super) async fn handle_pull(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Pull>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let askpass_id = envelope.payload.askpass_id;
        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let branch_name = envelope.payload.branch_name.map(|name| name.into());
        let remote_name = envelope.payload.remote_name.into();
        let rebase = envelope.payload.rebase;

        let remote_message = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.pull(branch_name, remote_name, rebase, askpass, cx)
            })
            .await??;

        Ok(proto::RemoteMessageResponse {
            stdout: remote_message.stdout,
            stderr: remote_message.stderr,
        })
    }

    pub(super) async fn handle_get_commit_data(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetCommitData>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetCommitDataResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let shas: Vec<Oid> = envelope
            .payload
            .shas
            .iter()
            .filter_map(|s| Oid::from_str(s).ok())
            .collect();

        let mut commits = Vec::with_capacity(shas.len());
        let mut receivers = Vec::new();

        repository_handle.update(&mut cx, |repository, cx| {
            for &sha in &shas {
                match repository.fetch_commit_data(sha, true, cx) {
                    CommitDataState::Loaded(data) => {
                        commits.push(commit_data_to_proto(data));
                    }
                    CommitDataState::Loading(Some(shared)) => {
                        receivers.push(shared.clone());
                    }
                    CommitDataState::Loading(None) => {
                        // todo(git_graph) this could happen if the request fails, we should encode an error case
                        debug_panic!(
                            "This should never happen since we passed true into fetch commit data"
                        );
                    }
                }
            }
        });

        let results = future::join_all(receivers).await;

        commits.extend(
            results
                .into_iter()
                .filter_map(|result| result.ok())
                .map(|data| commit_data_to_proto(&data)),
        );

        Ok(proto::GetCommitDataResponse { commits })
    }

    pub(super) async fn handle_get_initial_graph_data(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetInitialGraphData>,
        mut cx: AsyncApp,
    ) -> Result<impl Stream<Item = Result<proto::GetInitialGraphDataResponse>>> {
        const CHUNK_SIZE: usize = git::repository::GRAPH_CHUNK_SIZE;
        let payload = envelope.payload;

        let repository_id = RepositoryId::from_proto(payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let log_order = log_order_from_proto(payload.log_order());
        let log_source = log_source_from_proto(
            payload
                .log_source
                .context("missing initial graph data log source")?,
        )?;

        let (subscriber_sender, subscriber_receiver) = async_channel::unbounded();
        let (cached_commits, error, is_loading) =
            repository_handle.update(&mut cx, |repository, cx| {
                let response =
                    repository.graph_data(log_source.clone(), log_order, 0..usize::MAX, cx);
                let cached_commits = response.commits.to_vec();
                let error = response.error.clone();
                let is_loading = response.is_loading;

                if is_loading {
                    if let Some(graph_data) = repository
                        .initial_graph_data
                        .get_mut(&(log_source.clone(), log_order))
                    {
                        graph_data.subscribers.push(subscriber_sender);
                    }
                }

                (cached_commits, error, is_loading)
            });

        let (mut response_tx, response_rx) = mpsc::unbounded();
        cx.background_spawn(async move {
            if let Some(error) = error {
                if response_tx
                    .send(Err(anyhow!(error.to_string())))
                    .await
                    .is_err()
                {
                    return;
                }
                return;
            }

            for commits in cached_commits.chunks(CHUNK_SIZE) {
                let response = proto::GetInitialGraphDataResponse {
                    commits: commits
                        .iter()
                        .map(|commit| initial_graph_commit_to_proto(commit))
                        .collect(),
                };
                if response_tx.send(Ok(response)).await.is_err() {
                    return;
                }
            }

            if !is_loading {
                return;
            }

            while let Ok(chunk_result) = subscriber_receiver.recv().await {
                let commits = match chunk_result {
                    Ok(commits) => commits,
                    Err(error) => {
                        response_tx
                            .send(Err(anyhow!(error.to_string())))
                            .await
                            .context("Failed to send error")
                            .log_err();
                        return;
                    }
                };

                for commits in commits.chunks(CHUNK_SIZE) {
                    let response = proto::GetInitialGraphDataResponse {
                        commits: commits
                            .iter()
                            .map(|commit| initial_graph_commit_to_proto(commit))
                            .collect(),
                    };
                    if response_tx.send(Ok(response)).await.is_err() {
                        return;
                    }
                }
            }
        })
        .detach();

        Ok(response_rx)
    }

    pub(super) async fn handle_search_commits(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::SearchCommits>,
        mut cx: AsyncApp,
    ) -> Result<impl Stream<Item = Result<proto::SearchCommitsResponse>>> {
        const CHUNK_SIZE: usize = 100;

        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let log_source = log_source_from_proto(
            envelope
                .payload
                .log_source
                .context("missing search commit log source")?,
        )?;
        let search_args = SearchCommitArgs {
            query: SharedString::from(envelope.payload.query),
            case_sensitive: envelope.payload.case_sensitive,
        };

        let (request_tx, request_rx) = async_channel::unbounded();
        repository_handle.update(&mut cx, |repository, cx| {
            repository.search_commits(log_source, search_args, request_tx, cx);
        });

        let (mut response_tx, response_rx) = mpsc::unbounded();
        cx.background_spawn(async move {
            let mut shas = Vec::new();

            while let Ok(sha) = request_rx.recv().await {
                shas.push(sha.to_string());

                if shas.len() >= CHUNK_SIZE {
                    if response_tx
                        .send(Ok(proto::SearchCommitsResponse {
                            shas: mem::take(&mut shas),
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }

            if !shas.is_empty() {
                response_tx
                    .send(Ok(proto::SearchCommitsResponse { shas }))
                    .await
                    .ok();
            }
        })
        .detach();

        Ok(response_rx)
    }

    pub(super) async fn handle_edit_ref(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitEditRef>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let ref_name = envelope.payload.ref_name;
        let commit = match envelope.payload.action {
            Some(proto::git_edit_ref::Action::UpdateToCommit(sha)) => Some(sha),
            Some(proto::git_edit_ref::Action::Delete(_)) => None,
            None => anyhow::bail!("GitEditRef missing action"),
        };

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.edit_ref(ref_name, commit)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_repair_worktrees(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRepairWorktrees>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.repair_worktrees()
            })
            .await??;

        Ok(proto::Ack {})
    }
}
