use super::*;

impl LanguageServer {
    /// Send a RPC request to the language server.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
    pub fn request<T: request::Request>(
        &self,
        params: T::Params,
        request_timeout: Duration,
    ) -> impl LspRequestFuture<T::Result> + use<T>
    where
        T::Result: 'static + Send,
    {
        Self::request_internal::<T>(
            &self.next_id,
            &self.response_handlers,
            &self.outbound_tx,
            &self.notification_tx,
            &self.executor,
            request_timeout,
            params,
        )
    }

    /// Send a RPC request to the language server with a custom timer.
    /// Once the attached future becomes ready, the request will time out with the provided output message.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
    pub fn request_with_timer<T: request::Request, U: Future<Output = String>>(
        &self,
        params: T::Params,
        timer: U,
    ) -> impl LspRequestFuture<T::Result> + use<T, U>
    where
        T::Result: 'static + Send,
    {
        Self::request_internal_with_timer::<T, U>(
            &self.next_id,
            &self.response_handlers,
            &self.outbound_tx,
            &self.notification_tx,
            &self.executor,
            timer,
            params,
        )
    }

    pub(super) fn request_internal_with_timer<T, U>(
        next_id: &AtomicI32,
        response_handlers: &Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        outbound_tx: &channel::Sender<String>,
        notification_serializers: &channel::Sender<NotificationSerializer>,
        executor: &BackgroundExecutor,
        timer: U,
        params: T::Params,
    ) -> impl LspRequestFuture<T::Result> + use<T, U>
    where
        T::Result: 'static + Send,
        T: request::Request,
        U: Future<Output = String>,
    {
        let id = next_id.fetch_add(1, SeqCst);
        let message = serde_json::to_string(&protocol::Request {
            jsonrpc: JSON_RPC_VERSION,
            id: RequestId::Int(id),
            method: T::METHOD,
            params,
        })
        .expect("LSP message should be serializable to JSON");

        let (tx, rx) = oneshot::channel();
        let handle_response = response_handlers
            .lock()
            .as_mut()
            .context("server shut down")
            .map(|handlers| {
                let executor = executor.clone();
                handlers.insert(
                    RequestId::Int(id),
                    Box::new(move |result| {
                        executor
                            .spawn(async move {
                                let response = match result {
                                    Ok(response) => match serde_json::from_str(&response) {
                                        Ok(deserialized) => Ok(deserialized),
                                        Err(error) => {
                                            log::error!("failed to deserialize response from language server: {}. response from language server: {:?}", error, response);
                                            Err(error).context("failed to deserialize response")
                                        }
                                    }
                                    Err(error) => Err(anyhow!("{}", error.message)),
                                };
                                tx.send(response).ok();
                            })
                    }),
                );
            });

        let send = outbound_tx
            .try_send(message)
            .context("failed to write to language server's stdin");

        let response_handlers = Arc::clone(response_handlers);
        let notification_serializers = notification_serializers.downgrade();
        let started = Instant::now();
        LspRequest::new(id, async move {
            if let Err(e) = handle_response {
                return ConnectionResult::Result(Err(e));
            }
            if let Err(e) = send {
                return ConnectionResult::Result(Err(e));
            }

            let cancel_on_drop = util::defer(move || {
                if let Some(notification_serializers) = notification_serializers.upgrade() {
                    Self::notify_internal::<notification::Cancel>(
                        &notification_serializers,
                        CancelParams {
                            id: NumberOrString::Number(id),
                        },
                    )
                    .ok();
                }
            });

            let method = T::METHOD;
            select! {
                response = rx.fuse() => {
                    let elapsed = started.elapsed();
                    log::trace!("Took {elapsed:?} to receive response to {method:?} id {id}");
                    cancel_on_drop.abort();
                    match response {
                        Ok(response_result) => ConnectionResult::Result(response_result),
                        Err(Canceled) => {
                            log::error!("Server reset connection for a request {method:?} id {id}");
                            ConnectionResult::ConnectionReset
                        },
                    }
                }

                message = timer.fuse() => {
                    log::error!("Cancelled LSP request task for {method:?} id {id} {message}");
                    match response_handlers
                        .lock()
                        .as_mut()
                        .context("server shut down") {
                            Ok(handlers) => {
                                handlers.remove(&RequestId::Int(id));
                                ConnectionResult::Timeout
                            }
                            Err(e) => ConnectionResult::Result(Err(e)),
                        }
                }
            }
        })
    }

    pub(super) fn request_internal<T>(
        next_id: &AtomicI32,
        response_handlers: &Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        outbound_tx: &channel::Sender<String>,
        notification_serializers: &channel::Sender<NotificationSerializer>,
        executor: &BackgroundExecutor,
        request_timeout: Duration,
        params: T::Params,
    ) -> impl LspRequestFuture<T::Result> + use<T>
    where
        T::Result: 'static + Send,
        T: request::Request,
    {
        Self::request_internal_with_timer::<T, _>(
            next_id,
            response_handlers,
            outbound_tx,
            notification_serializers,
            executor,
            Self::request_timeout_future(executor.clone(), request_timeout),
            params,
        )
    }

    /// Internal function to return a Future from a configured timeout duration.
    /// If the duration is zero or `Duration::MAX`, the returned future never completes.
    fn request_timeout_future(
        executor: BackgroundExecutor,
        request_timeout: Duration,
    ) -> impl Future<Output = String> {
        if request_timeout == Duration::MAX || request_timeout == Duration::ZERO {
            return Either::Left(future::pending::<String>());
        }

        Either::Right(
            executor
                .timer(request_timeout)
                .map(move |_| format!("which took over {request_timeout:?}")),
        )
    }

    /// Obtain a request timer for the LSP.
    pub fn request_timer(&self, timeout: Duration) -> impl Future<Output = String> {
        Self::request_timeout_future(self.executor.clone(), timeout)
    }

    /// Sends a RPC notification to the language server.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#notificationMessage)
    pub fn notify<T: notification::Notification>(&self, params: T::Params) -> Result<()> {
        let outbound = self.notification_tx.clone();
        Self::notify_internal::<T>(&outbound, params)
    }

    pub(super) fn notify_internal<T: notification::Notification>(
        outbound_tx: &channel::Sender<NotificationSerializer>,
        params: T::Params,
    ) -> Result<()> {
        let serializer = NotificationSerializer(Box::new(move || {
            serde_json::to_string(&Notification {
                jsonrpc: JSON_RPC_VERSION,
                method: T::METHOD,
                params,
            })
            .unwrap()
        }));

        outbound_tx.send_blocking(serializer)?;
        Ok(())
    }
}
