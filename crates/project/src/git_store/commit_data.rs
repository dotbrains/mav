use super::*;

impl Repository {
    pub fn fetch_commit_data(
        &mut self,
        sha: Oid,
        await_result: bool,
        cx: &mut Context<Self>,
    ) -> &CommitDataState {
        if self.commit_data.contains_key(&sha) {
            let data = &self.commit_data[&sha];

            if let CommitDataState::Loading(None) = data
                && await_result
            {
                let (tx, rx) = oneshot::channel();
                self.commit_data
                    .insert(sha, CommitDataState::Loading(Some(rx.shared())));

                let handler = self.get_handler(cx);
                handler.completion_senders.insert(sha, tx);
            }

            return &self.commit_data[&sha];
        }

        let (state, completer) = if await_result {
            let (tx, rx) = oneshot::channel();
            (CommitDataState::Loading(Some(rx.shared())), Some(tx))
        } else {
            (CommitDataState::Loading(None), None)
        };

        self.commit_data.insert(sha, state);

        let handler = self.get_handler(cx);
        if let Some(tx) = completer {
            handler.completion_senders.insert(sha, tx);
        }
        let mut has_failed = false;
        if handler.commit_data_request.try_send(sha).is_ok() {
            handler.pending_requests.insert(sha);
        } else {
            has_failed = true;
            handler.completion_senders.remove(&sha);
            debug_assert!(
                matches!(
                    self.commit_data.remove(&sha),
                    Some(CommitDataState::Loading(_))
                ),
                "Commit data should still be loading when enqueueing the request fails"
            );
        }

        &self.commit_data.get(&sha).unwrap_or_else(|| {
            debug_assert!(!has_failed, "This should always be inserted");
            &CommitDataState::Loading(None)
        })
    }

    fn get_handler(&mut self, cx: &mut Context<Self>) -> &mut CommitDataHandler {
        if matches!(self.commit_data_handler, CommitDataHandlerState::Closed) {
            self.commit_data_handler =
                CommitDataHandlerState::Open(self.open_commit_data_handler(cx));
        }

        match &mut self.commit_data_handler {
            CommitDataHandlerState::Open(handler) => handler,
            CommitDataHandlerState::Closed => unreachable!(),
        }
    }

    fn open_commit_data_handler(&self, cx: &Context<Self>) -> CommitDataHandler {
        let state = self.repository_state.clone();
        let (result_tx, result_rx) = async_channel::bounded::<(Oid, CommitData)>(64);
        let (request_tx, request_rx) = async_channel::unbounded::<Oid>();

        let foreground_task = cx.spawn(async move |this, cx| {
            while let Ok((sha, commit_data)) = result_rx.recv().await {
                let result = this.update(cx, |this, cx| {
                    let data = Arc::new(commit_data);

                    if let CommitDataHandlerState::Open(handler) = &mut this.commit_data_handler {
                        handler.pending_requests.remove(&sha);
                        if let Some(completion_sender) = handler.completion_senders.remove(&sha) {
                            completion_sender.send(data.clone()).ok();
                        }
                    } else {
                        debug_panic!("The handler state has to be open for this task to exist");
                    }

                    let old_value = this.commit_data.insert(sha, CommitDataState::Loaded(data));
                    debug_assert!(
                        !matches!(old_value, Some(CommitDataState::Loaded(_))),
                        "We should never overwrite commit data"
                    );

                    cx.notify();
                });
                if result.is_err() {
                    break;
                }
            }

            this.update(cx, |this, _cx| {
                let CommitDataHandlerState::Open(handler) = std::mem::replace(
                    &mut this.commit_data_handler,
                    CommitDataHandlerState::Closed,
                ) else {
                    debug_panic!("The handler state has to be open for this task to exist");
                    return;
                };

                for sha in handler.pending_requests {
                    this.commit_data.remove(&sha);
                }
            })
            .ok();
        });

        let request_tx_for_handler = request_tx;
        let repository_id = self.id;
        let background_executor = cx.background_executor().clone();

        cx.background_spawn(async move {
            match state.await {
                Ok(RepositoryState::Local(LocalRepositoryState { backend, .. })) => {
                    Self::local_commit_data_reader(
                        backend,
                        request_rx,
                        result_tx,
                        background_executor,
                    )
                    .await;
                }
                Ok(RepositoryState::Remote(RemoteRepositoryState { project_id, client })) => {
                    Self::remote_commit_data_reader(
                        project_id,
                        client,
                        repository_id,
                        request_rx,
                        result_tx,
                        background_executor,
                    )
                    .await;
                }
                Err(error) => {
                    log::error!("failed to get repository state: {error}");
                    return;
                }
            };
        })
        .detach();

        CommitDataHandler {
            _task: foreground_task,
            commit_data_request: request_tx_for_handler,
            completion_senders: HashMap::default(),
            pending_requests: HashSet::default(),
        }
    }

    async fn local_commit_data_reader(
        backend: Arc<dyn GitRepository>,
        request_rx: smol::channel::Receiver<Oid>,
        result_tx: smol::channel::Sender<(Oid, CommitData)>,
        background_executor: BackgroundExecutor,
    ) {
        async fn receive_commit_data_request(
            request_rx: &smol::channel::Receiver<Oid>,
        ) -> Option<Oid> {
            if request_rx.is_closed() && request_rx.is_empty() {
                future::pending().await
            } else {
                request_rx.recv().await.ok()
            }
        }

        let reader = match backend.commit_data_reader() {
            Ok(reader) => reader,
            Err(error) => {
                log::error!("failed to create commit data reader: {error:?}");
                return;
            }
        };

        let read_commit_data = |sha| reader.read(sha).map(move |result| (sha, result));
        let mut read_futures = FuturesUnordered::new();

        loop {
            if read_futures.is_empty() {
                let timeout = background_executor.timer(Duration::from_secs(10));

                futures::select_biased! {
                    sha = futures::FutureExt::fuse(receive_commit_data_request(&request_rx)) => {
                        if let Some(sha) = sha {
                            read_futures.push(read_commit_data(sha));
                        }
                    }
                    _ = futures::FutureExt::fuse(timeout) => {
                        break;
                    }
                }
            }

            let next_read = read_futures.next().fuse();
            futures::pin_mut!(next_read);

            futures::select_biased! {
                result = next_read => {
                    let Some((sha, result)) = result else {
                        continue;
                    };

                    match result {
                        Ok(commit_data) => {
                            if result_tx.send((sha, commit_data)).await.is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            log::error!("failed to read commit data for {sha}: {error:?}");
                        }
                    }
                }
                sha = futures::FutureExt::fuse(receive_commit_data_request(&request_rx)) => {
                    if let Some(sha) = sha {
                        read_futures.push(read_commit_data(sha));
                    }
                }
            }
        }

        drop(result_tx);
    }

    async fn remote_commit_data_reader(
        project_id: ProjectId,
        client: AnyProtoClient,
        repository_id: RepositoryId,
        request_rx: smol::channel::Receiver<Oid>,
        result_tx: smol::channel::Sender<(Oid, CommitData)>,
        background_executor: BackgroundExecutor,
    ) {
        let mut response_futures =
            FuturesUnordered::<BoxFuture<'static, Result<proto::GetCommitDataResponse>>>::new();
        let mut accept_requests = true;
        let mut next_request = Self::get_next_request(
            project_id,
            client.clone(),
            repository_id,
            &request_rx,
            &background_executor,
        )
        .boxed()
        .fuse();

        loop {
            if !accept_requests && response_futures.is_empty() {
                break;
            }

            if response_futures.is_empty() {
                match (&mut next_request).await {
                    NextCommitDataRequest::Request(request) => {
                        response_futures.push(request);
                        next_request = Self::get_next_request(
                            project_id,
                            client.clone(),
                            repository_id,
                            &request_rx,
                            &background_executor,
                        )
                        .boxed()
                        .fuse();
                    }
                    NextCommitDataRequest::Closed | NextCommitDataRequest::Idle => break,
                }
            }

            let next_response = response_futures.next().fuse();
            futures::pin_mut!(next_response);

            futures::select_biased! {
                request = next_request => {
                    match request {
                        NextCommitDataRequest::Request(request) => {
                            response_futures.push(request);
                        }
                        NextCommitDataRequest::Idle => {}
                        NextCommitDataRequest::Closed => {
                            accept_requests = false;
                        }
                    }

                    if accept_requests {
                        next_request = Self::get_next_request(
                            project_id,
                            client.clone(),
                            repository_id,
                            &request_rx,
                            &background_executor,
                        )
                        .boxed()
                        .fuse();
                    }
                }
                result = next_response => {
                    let Some(result) = result else {
                        continue;
                    };

                    if let Ok(commit_data) = result {
                        for commit in commit_data.commits {
                            let Ok(commit_data) = commit_data_from_proto(commit) else {
                                continue;
                            };

                            if result_tx
                                .send((commit_data.sha, commit_data))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
            }
        }

        drop(result_tx);
    }

    async fn get_next_request(
        project_id: ProjectId,
        client: AnyProtoClient,
        repository_id: RepositoryId,
        request_rx: &smol::channel::Receiver<Oid>,
        background_executor: &BackgroundExecutor,
    ) -> NextCommitDataRequest {
        let mut queued_shas = Vec::with_capacity(64);

        loop {
            if queued_shas.len() >= 64 {
                break;
            }

            let timeout = background_executor.timer(Duration::from_millis(5));

            futures::select_biased! {
                sha = futures::FutureExt::fuse(request_rx.recv()) => {
                    let Ok(sha) = sha else {
                        break;
                    };

                    queued_shas.push(sha);

                }
                _ = futures::FutureExt::fuse(timeout) => {
                    break;
                }
            }
        }

        if queued_shas.is_empty() && request_rx.is_closed() {
            NextCommitDataRequest::Closed
        } else if queued_shas.is_empty() {
            NextCommitDataRequest::Idle
        } else {
            NextCommitDataRequest::Request(
                client
                    .request(proto::GetCommitData {
                        project_id: project_id.to_proto(),
                        repository_id: repository_id.to_proto(),
                        shas: queued_shas.into_iter().map(|oid| oid.to_string()).collect(),
                    })
                    .boxed(),
            )
        }
    }
}
