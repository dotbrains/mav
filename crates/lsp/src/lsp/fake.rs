use super::*;

/// Mock language server for use in tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone)]
pub struct FakeLanguageServer {
    pub binary: LanguageServerBinary,
    pub server: Arc<LanguageServer>,
    notifications_rx: channel::Receiver<(String, String)>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeLanguageServer {
    /// Construct a fake language server.
    pub fn new(
        server_id: LanguageServerId,
        binary: LanguageServerBinary,
        name: String,
        capabilities: ServerCapabilities,
        cx: &mut AsyncApp,
    ) -> (LanguageServer, FakeLanguageServer) {
        let (stdin_writer, stdin_reader) = async_pipe::pipe();
        let (stdout_writer, stdout_reader) = async_pipe::pipe();
        let (notifications_tx, notifications_rx) = channel::unbounded();

        let server_name = LanguageServerName(name.clone().into());
        let process_name = Arc::from(name.as_str());
        let root = Self::root_path();
        let workspace_folders: Arc<Mutex<BTreeSet<Uri>>> = Default::default();
        let mut server = LanguageServer::new_internal(
            server_id,
            server_name.clone(),
            stdin_writer,
            stdout_reader,
            None::<async_pipe::PipeReader>,
            Arc::new(Mutex::new(None)),
            None,
            None,
            binary.clone(),
            root,
            Some(workspace_folders.clone()),
            cx,
            |_| false,
        );
        server.process_name = process_name;
        let fake = FakeLanguageServer {
            binary: binary.clone(),
            server: Arc::new({
                let mut server = LanguageServer::new_internal(
                    server_id,
                    server_name,
                    stdout_writer,
                    stdin_reader,
                    None::<async_pipe::PipeReader>,
                    Arc::new(Mutex::new(None)),
                    None,
                    None,
                    binary,
                    Self::root_path(),
                    Some(workspace_folders),
                    cx,
                    move |msg| {
                        notifications_tx
                            .try_send((
                                msg.method.to_string(),
                                msg.params.as_ref().unwrap_or(&Value::Null).to_string(),
                            ))
                            .ok();
                        true
                    },
                );
                server.process_name = name.as_str().into();
                server
            }),
            notifications_rx,
        };
        fake.set_request_handler::<request::Initialize, _, _>({
            let capabilities = capabilities;
            move |_, _| {
                let capabilities = capabilities.clone();
                let name = name.clone();
                async move {
                    Ok(InitializeResult {
                        capabilities,
                        server_info: Some(ServerInfo {
                            name,
                            ..Default::default()
                        }),
                    })
                }
            }
        });

        fake.set_request_handler::<request::Shutdown, _, _>(|_, _| async move { Ok(()) });

        (server, fake)
    }
    #[cfg(target_os = "windows")]
    fn root_path() -> Uri {
        Uri::from_file_path("C:/").unwrap()
    }

    #[cfg(not(target_os = "windows"))]
    fn root_path() -> Uri {
        Uri::from_file_path("/").unwrap()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl LanguageServer {
    pub fn full_capabilities() -> ServerCapabilities {
        ServerCapabilities {
            document_highlight_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl FakeLanguageServer {
    /// See [`LanguageServer::notify`].
    pub fn notify<T: notification::Notification>(&self, params: T::Params) {
        self.server.notify::<T>(params).ok();
    }

    /// See [`LanguageServer::request`].
    pub async fn request<T>(
        &self,
        params: T::Params,
        timeout: Duration,
    ) -> ConnectionResult<T::Result>
    where
        T: request::Request,
        T::Result: 'static + Send,
    {
        self.server.request::<T>(params, timeout).await
    }

    /// Attempts [`Self::try_receive_notification`], unwrapping if it has not received the specified type yet.
    pub async fn receive_notification<T: notification::Notification>(&mut self) -> T::Params {
        self.try_receive_notification::<T>().await.unwrap()
    }

    /// Consumes the notification channel until it finds a notification for the specified type.
    pub async fn try_receive_notification<T: notification::Notification>(
        &mut self,
    ) -> Option<T::Params> {
        loop {
            let (method, params) = self.notifications_rx.recv().await.ok()?;
            if method == T::METHOD {
                return Some(serde_json::from_str::<T::Params>(&params).unwrap());
            } else {
                log::info!("skipping message in fake language server {:?}", params);
            }
        }
    }

    /// Registers a handler for a specific kind of request. Removes any existing handler for specified request type.
    pub fn set_request_handler<T, F, Fut>(
        &self,
        mut handler: F,
    ) -> futures::channel::mpsc::UnboundedReceiver<()>
    where
        T: 'static + request::Request,
        T::Params: 'static + Send,
        F: 'static + Send + FnMut(T::Params, gpui::AsyncApp) -> Fut,
        Fut: 'static + Future<Output = Result<T::Result>>,
    {
        let (responded_tx, responded_rx) = futures::channel::mpsc::unbounded();
        self.server.remove_request_handler::<T>();
        self.server
            .on_request::<T, _, _>(move |params, cx| {
                let result = handler(params, cx.clone());
                let responded_tx = responded_tx.clone();
                let executor = cx.background_executor().clone();
                async move {
                    let _guard = gpui_util::defer({
                        let responded_tx = responded_tx.clone();
                        move || {
                            responded_tx.unbounded_send(()).ok();
                        }
                    });
                    executor.simulate_random_delay().await;
                    result.await
                }
            })
            .detach();
        responded_rx
    }

    /// Registers a handler for a specific kind of notification. Removes any existing handler for specified notification type.
    pub fn handle_notification<T, F>(
        &self,
        mut handler: F,
    ) -> futures::channel::mpsc::UnboundedReceiver<()>
    where
        T: 'static + notification::Notification,
        T::Params: 'static + Send,
        F: 'static + Send + FnMut(T::Params, gpui::AsyncApp),
    {
        let (handled_tx, handled_rx) = futures::channel::mpsc::unbounded();
        self.server.remove_notification_handler::<T>();
        self.server
            .on_notification::<T, _>(move |params, cx| {
                handler(params, cx.clone());
                handled_tx.unbounded_send(()).ok();
            })
            .detach();
        handled_rx
    }

    /// Removes any existing handler for specified notification type.
    pub fn remove_request_handler<T>(&mut self)
    where
        T: 'static + request::Request,
    {
        self.server.remove_request_handler::<T>();
    }

    /// Simulate that the server has started work and notifies about its progress with the specified token.
    pub async fn start_progress(&self, token: impl Into<String>) {
        self.start_progress_with(token, Default::default(), Default::default())
            .await
    }

    pub async fn start_progress_with(
        &self,
        token: impl Into<String>,
        progress: WorkDoneProgressBegin,
        request_timeout: Duration,
    ) {
        let token = token.into();
        self.request::<request::WorkDoneProgressCreate>(
            WorkDoneProgressCreateParams {
                token: NumberOrString::String(token.clone()),
            },
            request_timeout,
        )
        .await
        .into_response()
        .unwrap();
        self.notify::<notification::Progress>(ProgressParams {
            token: NumberOrString::String(token),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(progress)),
        });
    }

    /// Simulate that the server has completed work and notifies about that with the specified token.
    pub fn end_progress(&self, token: impl Into<String>) {
        self.notify::<notification::Progress>(ProgressParams {
            token: NumberOrString::String(token.into()),
            value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(Default::default())),
        });
    }
}
