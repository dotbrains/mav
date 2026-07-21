use super::*;

impl Repository {
    pub fn get_graph_data(
        &self,
        log_source: LogSource,
        log_order: LogOrder,
    ) -> Option<&InitialGitGraphData> {
        self.initial_graph_data.get(&(log_source, log_order))
    }

    pub fn search_commits(
        &mut self,
        log_source: LogSource,
        search_args: SearchCommitArgs,
        request_tx: async_channel::Sender<Oid>,
        cx: &mut Context<Self>,
    ) {
        let repository_state = self.repository_state.clone();
        let repository_id = self.id;

        cx.background_spawn(async move {
            let repo_state = repository_state.await;

            match repo_state {
                Ok(RepositoryState::Local(LocalRepositoryState { backend, .. })) => {
                    backend
                        .search_commits(log_source, search_args, request_tx)
                        .await
                        .log_err();
                }

                Ok(RepositoryState::Remote(RemoteRepositoryState { client, project_id })) => {
                    let result = client
                        .request_stream(proto::SearchCommits {
                            project_id: project_id.to_proto(),
                            repository_id: repository_id.to_proto(),
                            log_source: Some(log_source_to_proto(&log_source)),
                            query: search_args.query.to_string(),
                            case_sensitive: search_args.case_sensitive,
                        })
                        .await;

                    let mut stream = match result {
                        Ok(stream) => stream,
                        Err(error) => {
                            log::error!("failed to search commits remotely: {error:?}");
                            return;
                        }
                    };

                    while let Some(response) = stream.next().await {
                        let response = match response {
                            Ok(response) => response,
                            Err(error) => {
                                log::error!(
                                    "failed to receive remote commit search results: {error:?}"
                                );
                                return;
                            }
                        };

                        for sha in &response.shas {
                            let Ok(oid) = Oid::from_str(sha) else {
                                return;
                            };
                            if request_tx.send(oid).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                Err(error) => {
                    log::error!("failed to get repository state for commit search: {error}");
                }
            };
        })
        .detach();
    }

    pub fn graph_data(
        &mut self,
        log_source: LogSource,
        log_order: LogOrder,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> GraphDataResponse<'_> {
        let initial_commit_data = self
            .initial_graph_data
            .entry((log_source.clone(), log_order))
            .or_insert_with(|| {
                let state = self.repository_state.clone();
                let log_source = log_source.clone();

                let fetch_task = cx.spawn(async move |repository, cx| {
                    let state = state.await;
                    let result = match state {
                        Ok(RepositoryState::Local(LocalRepositoryState { backend, .. })) => {
                            Self::local_git_graph_data(
                                repository.clone(),
                                backend,
                                log_source.clone(),
                                log_order,
                                cx,
                            )
                            .await
                        }
                        Ok(RepositoryState::Remote(remote)) => {
                            Self::remote_git_graph_data(
                                repository.clone(),
                                remote,
                                log_source.clone(),
                                log_order,
                                cx,
                            )
                            .await
                        }
                        Err(e) => Err(SharedString::from(e)),
                    };

                    repository
                        .update(cx, |repository, cx| {
                            if let Some(data) = repository
                                .initial_graph_data
                                .get_mut(&(log_source.clone(), log_order))
                            {
                                match &result {
                                    Ok(()) => {
                                        cx.emit(RepositoryEvent::GraphEvent(
                                            (log_source.clone(), log_order),
                                            GitGraphEvent::FullyLoaded,
                                        ));
                                    }
                                    Err(fetch_task_error) => {
                                        data.subscribers.retain(|sender| {
                                            sender.try_send(Err(fetch_task_error.clone())).is_ok()
                                        });
                                        data.error = Some(fetch_task_error.clone());
                                        cx.emit(RepositoryEvent::GraphEvent(
                                            (log_source.clone(), log_order),
                                            GitGraphEvent::LoadingError,
                                        ));
                                    }
                                }
                                data.subscribers.clear();
                            } else {
                                debug_panic!(
                                    "This task would be dropped if this entry doesn't exist"
                                );
                            }
                        })
                        .log_err();
                });

                InitialGitGraphData {
                    fetch_task,
                    error: None,
                    commit_data: Vec::new(),
                    commit_oid_to_index: HashMap::default(),
                    subscribers: Vec::new(),
                }
            });

        let max_start = initial_commit_data.commit_data.len().saturating_sub(1);
        let max_end = initial_commit_data.commit_data.len();

        GraphDataResponse {
            commits: &initial_commit_data.commit_data
                [range.start.min(max_start)..range.end.min(max_end)],
            is_loading: !initial_commit_data.fetch_task.is_ready(),
            error: initial_commit_data.error.clone(),
        }
    }

    async fn append_initial_graph_commits(
        this: &WeakEntity<Self>,
        graph_data_key: &(LogSource, LogOrder),
        initial_graph_commit_data: Vec<Arc<InitialGraphCommitData>>,
        cx: &mut AsyncApp,
    ) {
        this.update(cx, |repository, cx| {
            let graph_data = repository
                .initial_graph_data
                .entry(graph_data_key.clone())
                .and_modify(|graph_data| {
                    if !graph_data.subscribers.is_empty() {
                        graph_data.subscribers.retain(|sender| {
                            sender
                                .try_send(Ok(initial_graph_commit_data.clone()))
                                .is_ok()
                        });
                    }

                    for commit_data in initial_graph_commit_data {
                        graph_data
                            .commit_oid_to_index
                            .insert(commit_data.sha, graph_data.commit_data.len());
                        graph_data.commit_data.push(commit_data);
                    }
                    cx.emit(RepositoryEvent::GraphEvent(
                        graph_data_key.clone(),
                        GitGraphEvent::CountUpdated(graph_data.commit_data.len()),
                    ));
                });

            match &graph_data {
                Entry::Occupied(_) => {}
                Entry::Vacant(_) => {
                    debug_panic!("This task should be dropped if data doesn't exist");
                }
            }
        })
        .log_err();
    }

    async fn local_git_graph_data(
        this: WeakEntity<Self>,
        backend: Arc<dyn GitRepository>,
        log_source: LogSource,
        log_order: LogOrder,
        cx: &mut AsyncApp,
    ) -> Result<(), SharedString> {
        let (request_tx, request_rx) =
            async_channel::unbounded::<Vec<Arc<InitialGraphCommitData>>>();

        let task = cx.background_executor().spawn({
            let log_source = log_source.clone();
            async move {
                backend
                    .initial_graph_data(log_source, log_order, request_tx)
                    .await
                    .map_err(|err| SharedString::from(err.to_string()))
            }
        });

        let graph_data_key = (log_source, log_order);

        while let Ok(initial_graph_commit_data) = request_rx.recv().await {
            Self::append_initial_graph_commits(
                &this,
                &graph_data_key,
                initial_graph_commit_data,
                cx,
            )
            .await;
        }

        task.await?;
        Ok(())
    }

    async fn remote_git_graph_data(
        this: WeakEntity<Self>,
        remote: RemoteRepositoryState,
        log_source: LogSource,
        log_order: LogOrder,
        cx: &mut AsyncApp,
    ) -> Result<(), SharedString> {
        let repository_id = this
            .update(cx, |repository, _| repository.id)
            .map_err(|err| SharedString::from(err.to_string()))?;
        let graph_data_key = (log_source.clone(), log_order);
        let mut response = remote
            .client
            .request_stream(proto::GetInitialGraphData {
                project_id: remote.project_id.to_proto(),
                repository_id: repository_id.to_proto(),
                log_source: Some(log_source_to_proto(&log_source)),
                log_order: log_order_to_proto(log_order),
            })
            .await
            .map_err(|err| SharedString::from(err.to_string()))?;

        while let Some(response) = response.next().await {
            let response = response.map_err(|err| SharedString::from(err.to_string()))?;
            let commits = response
                .commits
                .into_iter()
                .map(initial_graph_commit_from_proto)
                .collect::<Result<Vec<_>>>()
                .map_err(|err| SharedString::from(err.to_string()))?;
            Self::append_initial_graph_commits(&this, &graph_data_key, commits, cx).await;
        }

        Ok(())
    }
}
