use super::*;

impl LanguageServer {
    /// Sends a shutdown request to the language server process and prepares the [`LanguageServer`] to be dropped.
    pub fn shutdown(&self) -> Option<impl 'static + Send + Future<Output = Option<()>> + use<>> {
        let tasks = self.io_tasks.lock().take()?;

        let response_handlers = self.response_handlers.clone();
        let next_id = AtomicI32::new(self.next_id.load(SeqCst));
        let outbound_tx = self.outbound_tx.clone();
        let executor = self.executor.clone();
        let notification_serializers = self.notification_tx.clone();
        let mut output_done = self.output_done_rx.lock().take().unwrap();
        let shutdown_request = Self::request_internal::<request::Shutdown>(
            &next_id,
            &response_handlers,
            &outbound_tx,
            &notification_serializers,
            &executor,
            SERVER_SHUTDOWN_TIMEOUT,
            (),
        );

        let server = self.server.clone();
        let name = self.name.clone();
        let server_id = self.server_id;
        let mut timer = self.executor.timer(SERVER_SHUTDOWN_TIMEOUT).fuse();
        Some(async move {
            log::debug!("language server shutdown started");

            select! {
                request_result = shutdown_request.fuse() => {
                    match request_result {
                        ConnectionResult::Timeout => {
                            log::warn!("timeout waiting for language server {name} (id {server_id}) to shutdown");
                        },
                        ConnectionResult::ConnectionReset => {
                            log::warn!("language server {name} (id {server_id}) closed the shutdown request connection");
                        },
                        ConnectionResult::Result(Err(e)) => {
                            log::error!("Shutdown request failure, server {name} (id {server_id}): {e:#}");
                        },
                        ConnectionResult::Result(Ok(())) => {}
                    }
                }

                _ = timer => {
                    log::info!("timeout waiting for language server {name} (id {server_id}) to shutdown");
                },
            }

            response_handlers.lock().take();
            Self::notify_internal::<notification::Exit>(&notification_serializers, ()).ok();
            notification_serializers.close();
            output_done.recv().await;
            server.lock().take().map(|mut child| child.kill());
            drop(tasks);
            log::debug!("language server shutdown finished");
            Some(())
        })
    }

    /// Register a handler to handle incoming LSP notifications.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#notificationMessage)
    #[must_use]
    pub fn on_notification<T, F>(&self, f: F) -> Subscription
    where
        T: notification::Notification,
        F: 'static + Send + FnMut(T::Params, &mut AsyncApp),
    {
        self.on_custom_notification(T::METHOD, f)
    }

    /// Register a handler to handle incoming LSP requests.
    ///
    /// [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
    #[must_use]
    pub fn on_request<T, F, Fut>(&self, f: F) -> Subscription
    where
        T: request::Request,
        T::Params: 'static + Send,
        F: 'static + FnMut(T::Params, &mut AsyncApp) -> Fut + Send,
        Fut: 'static + Future<Output = Result<T::Result>>,
    {
        self.on_custom_request(T::METHOD, f)
    }

    /// Registers a handler to inspect all language server process stdio.
    #[must_use]
    pub fn on_io<F>(&self, f: F) -> Subscription
    where
        F: 'static + Send + FnMut(IoKind, &str),
    {
        let id = self.next_id.fetch_add(1, SeqCst);
        self.io_handlers.lock().insert(id, Box::new(f));
        Subscription::Io {
            id,
            io_handlers: Some(Arc::downgrade(&self.io_handlers)),
        }
    }

    /// Removes a request handler registers via [`Self::on_request`].
    pub fn remove_request_handler<T: request::Request>(&self) {
        self.notification_handlers.lock().remove(T::METHOD);
    }

    /// Removes a notification handler registers via [`Self::on_notification`].
    pub fn remove_notification_handler<T: notification::Notification>(&self) {
        self.notification_handlers.lock().remove(T::METHOD);
    }

    /// Checks if a notification handler has been registered via [`Self::on_notification`].
    pub fn has_notification_handler<T: notification::Notification>(&self) -> bool {
        self.notification_handlers.lock().contains_key(T::METHOD)
    }

    #[must_use]
    fn on_custom_notification<Params, F>(&self, method: &'static str, mut f: F) -> Subscription
    where
        F: 'static + FnMut(Params, &mut AsyncApp) + Send,
        Params: DeserializeOwned,
    {
        let prev_handler = self.notification_handlers.lock().insert(
            method,
            Box::new(move |_, params, cx| {
                if let Some(params) = serde_json::from_value(params).log_err() {
                    f(params, cx);
                }
            }),
        );
        assert!(
            prev_handler.is_none(),
            "registered multiple handlers for the same LSP method"
        );
        Subscription::Notification {
            method,
            notification_handlers: Some(Arc::downgrade(&self.notification_handlers)),
        }
    }

    #[must_use]
    fn on_custom_request<Params, Res, Fut, F>(&self, method: &'static str, mut f: F) -> Subscription
    where
        F: 'static + FnMut(Params, &mut AsyncApp) -> Fut + Send,
        Fut: 'static + Future<Output = Result<Res>>,
        Params: DeserializeOwned + Send + 'static,
        Res: Serialize,
    {
        let outbound_tx = self.outbound_tx.clone();
        let pending_respond_tasks = self.pending_respond_tasks.clone();
        let prev_handler = self.notification_handlers.lock().insert(
            method,
            Box::new(move |id, params, cx| {
                if let Some(id) = id {
                    match serde_json::from_value(params) {
                        Ok(params) => {
                            let response = f(params, cx);
                            let task = cx.foreground_executor().spawn({
                                let outbound_tx = outbound_tx.clone();
                                let pending_respond_tasks = pending_respond_tasks.clone();
                                let id = id.clone();
                                async move {
                                    let response = match response.await {
                                        Ok(result) => Response {
                                            jsonrpc: JSON_RPC_VERSION,
                                            id: id.clone(),
                                            value: LspResult::Ok(Some(result)),
                                        },
                                        Err(error) => Response {
                                            jsonrpc: JSON_RPC_VERSION,
                                            id: id.clone(),
                                            value: LspResult::Error(Some(Error {
                                                code: lsp_types::error_codes::REQUEST_FAILED,
                                                message: error.to_string(),
                                                data: None,
                                            })),
                                        },
                                    };
                                    if let Some(response) =
                                        serde_json::to_string(&response).log_err()
                                    {
                                        outbound_tx.try_send(response).ok();
                                    }
                                    pending_respond_tasks.lock().remove(&id);
                                }
                            });
                            pending_respond_tasks.lock().insert(id, task);
                        }

                        Err(error) => {
                            log::error!("error deserializing {} request: {:?}", method, error);
                            let response = AnyResponse {
                                jsonrpc: JSON_RPC_VERSION,
                                id,
                                result: None,
                                error: Some(Error {
                                    code: -32700, // Parse error
                                    message: error.to_string(),
                                    data: None,
                                }),
                            };
                            if let Some(response) = serde_json::to_string(&response).log_err() {
                                outbound_tx.try_send(response).ok();
                            }
                        }
                    }
                }
            }),
        );
        assert!(
            prev_handler.is_none(),
            "registered multiple handlers for the same LSP method"
        );
        Subscription::Notification {
            method,
            notification_handlers: Some(Arc::downgrade(&self.notification_handlers)),
        }
    }

    /// Get the name of the running language server.
    pub fn name(&self) -> LanguageServerName {
        self.name.clone()
    }

    /// Get the version of the running language server.
    pub fn version(&self) -> Option<SharedString> {
        self.version.clone()
    }

    /// Get the readable version of the running language server.
    pub fn readable_version(&self) -> Option<SharedString> {
        match self.name().as_ref() {
            "gopls" => {
                // Gopls returns a detailed JSON object as its version string; we must parse it to extract the semantic version.
                // Example: `{"GoVersion":"go1.26.0","Path":"golang.org/x/tools/gopls","Main":{},"Deps":[],"Settings":[],"Version":"v0.21.1"}`
                self.version
                    .as_ref()
                    .and_then(|obj| {
                        #[derive(Deserialize)]
                        struct GoplsVersion<'a> {
                            #[serde(rename = "Version")]
                            version: &'a str,
                        }
                        let parsed: GoplsVersion = serde_json::from_str(obj.as_str()).ok()?;
                        Some(parsed.version.trim_start_matches("v").to_owned().into())
                    })
                    .or_else(|| self.version.clone())
            }
            _ => self.version.clone(),
        }
    }

    /// Get the process name of the running language server.
    pub fn process_name(&self) -> &str {
        &self.process_name
    }

    /// Get the reported capabilities of the running language server.
    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities.read().clone()
    }

    /// Get the reported capabilities of the running language server and
    /// what we know on the client/adapter-side of its capabilities.
    pub fn adapter_server_capabilities(&self) -> AdapterServerCapabilities {
        AdapterServerCapabilities {
            server_capabilities: self.capabilities(),
            code_action_kinds: self.code_action_kinds(),
        }
    }

    /// Update the capabilities of the running language server.
    pub fn update_capabilities(&self, update: impl FnOnce(&mut ServerCapabilities)) {
        update(self.capabilities.write().deref_mut());
    }

    /// Get the individual configuration settings for the running language server.
    /// Does not include globally applied settings (which are stored in ProjectSettings::GlobalLspSettings).
    pub fn configuration(&self) -> &Value {
        &self.configuration.settings
    }

    /// Get the ID of the running language server.
    pub fn server_id(&self) -> LanguageServerId {
        self.server_id
    }

    /// Get the process ID of the running language server, if available.
    pub fn process_id(&self) -> Option<u32> {
        self.server.lock().as_ref().map(|child| child.id())
    }

    /// Get the binary information of the running language server.
    pub fn binary(&self) -> &LanguageServerBinary {
        &self.binary
    }
}
