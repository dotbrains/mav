use super::*;

impl LanguageServer {
    /// Starts a language server process.
    /// A request_timeout of zero or Duration::MAX indicates an indefinite timeout.
    pub fn new(
        stderr_capture: Arc<Mutex<Option<String>>>,
        server_id: LanguageServerId,
        server_name: LanguageServerName,
        binary: LanguageServerBinary,
        root_path: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
        workspace_folders: Option<Arc<Mutex<BTreeSet<Uri>>>>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };
        let root_uri = Uri::from_file_path(&working_dir)
            .map_err(|()| anyhow!("{working_dir:?} is not a valid URI"))?;
        log::info!(
            "starting language server process. binary path: \
            {:?}, working directory: {:?}, args: {:?}",
            binary.path,
            working_dir,
            &binary.arguments
        );
        let mut command = util::command::new_command(&binary.path);
        command
            .current_dir(working_dir)
            .args(&binary.arguments)
            .envs(binary.env.clone().unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut server = command
            .spawn()
            .with_context(|| format!("failed to spawn command {command:?}",))?;

        let stdin = server.stdin.take().unwrap();
        let stdout = server.stdout.take().unwrap();
        let stderr = server.stderr.take().unwrap();
        let server = Self::new_internal(
            server_id,
            server_name,
            stdin,
            stdout,
            Some(stderr),
            stderr_capture,
            Some(server),
            code_action_kinds,
            binary,
            root_uri,
            workspace_folders,
            cx,
            move |notification| {
                log::info!(
                    "Language server with id {} sent unhandled notification {}:\n{}",
                    server_id,
                    notification.method,
                    serde_json::to_string_pretty(&notification.params).unwrap(),
                );
                false
            },
        );

        Ok(server)
    }

    fn new_internal<Stdin, Stdout, Stderr, F>(
        server_id: LanguageServerId,
        server_name: LanguageServerName,
        stdin: Stdin,
        stdout: Stdout,
        stderr: Option<Stderr>,
        stderr_capture: Arc<Mutex<Option<String>>>,
        server: Option<Child>,
        code_action_kinds: Option<Vec<CodeActionKind>>,
        binary: LanguageServerBinary,
        root_uri: Uri,
        workspace_folders: Option<Arc<Mutex<BTreeSet<Uri>>>>,
        cx: &mut AsyncApp,
        on_unhandled_notification: F,
    ) -> Self
    where
        Stdin: AsyncWrite + Unpin + Send + 'static,
        Stdout: AsyncRead + Unpin + Send + 'static,
        Stderr: AsyncRead + Unpin + Send + 'static,
        F: Fn(&NotificationOrRequest) -> bool + 'static + Send + Sync + Clone,
    {
        let (outbound_tx, outbound_rx) = channel::unbounded::<String>();
        let (output_done_tx, output_done_rx) = barrier::channel();
        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));
        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));
        let pending_respond_tasks = PendingRespondTasks::default();
        let io_handlers = Arc::new(Mutex::new(HashMap::default()));

        let stdout_input_task = cx.spawn({
            let unhandled_notification_wrapper = {
                let response_channel = outbound_tx.clone();
                async move |msg: NotificationOrRequest| {
                    let did_handle = on_unhandled_notification(&msg);
                    if !did_handle && let Some(message_id) = msg.id {
                        let response = AnyResponse {
                            jsonrpc: JSON_RPC_VERSION,
                            id: message_id,
                            error: Some(Error {
                                code: -32601,
                                message: format!("Unrecognized method `{}`", msg.method),
                                data: None,
                            }),
                            result: None,
                        };
                        if let Ok(response) = serde_json::to_string(&response) {
                            response_channel.send(response).await.ok();
                        }
                    }
                }
            };
            let notification_handlers = notification_handlers.clone();
            let response_handlers = response_handlers.clone();
            let io_handlers = io_handlers.clone();
            let pending_respond_tasks = pending_respond_tasks.clone();
            async move |cx| {
                Self::handle_incoming_messages(
                    stdout,
                    unhandled_notification_wrapper,
                    notification_handlers,
                    response_handlers,
                    pending_respond_tasks,
                    io_handlers,
                    cx,
                )
                .log_err()
                .await
            }
        });
        let stderr_input_task = stderr
            .map(|stderr| {
                let io_handlers = io_handlers.clone();
                let stderr_captures = stderr_capture.clone();
                cx.background_spawn(async move {
                    Self::handle_stderr(stderr, io_handlers, stderr_captures)
                        .log_err()
                        .await
                })
            })
            .unwrap_or_else(|| Task::ready(None));
        let input_task = cx.background_spawn(async move {
            let (stdout, stderr) = futures::join!(stdout_input_task, stderr_input_task);
            stdout.or(stderr)
        });
        let output_task = cx.background_spawn({
            Self::handle_outgoing_messages(
                stdin,
                outbound_rx,
                output_done_tx,
                response_handlers.clone(),
                io_handlers.clone(),
            )
            .log_err()
        });

        let configuration = DidChangeConfigurationParams {
            settings: Value::Null,
        }
        .into();

        let (notification_tx, notification_rx) = channel::unbounded::<NotificationSerializer>();
        cx.background_spawn({
            let outbound_tx = outbound_tx.clone();
            async move {
                while let Ok(serializer) = notification_rx.recv().await {
                    let serialized = (serializer.0)();
                    let Ok(_) = outbound_tx.send(serialized).await else {
                        return;
                    };
                }
                outbound_tx.close();
            }
        })
        .detach();
        Self {
            server_id,
            notification_handlers,
            notification_tx,
            response_handlers,
            pending_respond_tasks,
            io_handlers,
            name: server_name,
            version: None,
            process_name: binary
                .path
                .file_name()
                .map(|name| Arc::from(name.to_string_lossy()))
                .unwrap_or_default(),
            binary,
            capabilities: Default::default(),
            configuration,
            code_action_kinds,
            next_id: Default::default(),
            outbound_tx,
            executor: cx.background_executor().clone(),
            io_tasks: Mutex::new(Some((input_task, output_task))),
            output_done_rx: Mutex::new(Some(output_done_rx)),
            server: Arc::new(Mutex::new(server)),
            workspace_folders,
            root_uri,
        }
    }

    /// List of code action kinds this language server reports being able to emit.
    pub fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kinds.clone()
    }

    async fn handle_incoming_messages<Stdout>(
        stdout: Stdout,
        on_unhandled_notification: impl AsyncFn(NotificationOrRequest) + 'static + Send,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        pending_respond_tasks: PendingRespondTasks,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()>
    where
        Stdout: AsyncRead + Unpin + Send + 'static,
    {
        use smol::stream::StreamExt;
        let stdout = BufReader::new(stdout);
        let _clear_response_handlers = util::defer({
            let response_handlers = response_handlers.clone();
            move || {
                response_handlers.lock().take();
            }
        });
        let mut input_handler = input_handler::LspStdoutHandler::new(
            stdout,
            response_handlers,
            io_handlers,
            cx.background_executor().clone(),
        );

        while let Some(msg) = input_handler.incoming_messages.next().await {
            if msg.method == <notification::Cancel as notification::Notification>::METHOD {
                if let Some(params) = msg.params {
                    if let Ok(cancel_params) = serde_json::from_value::<CancelParams>(params) {
                        let id = match cancel_params.id {
                            NumberOrString::Number(id) => RequestId::Int(id),
                            NumberOrString::String(id) => RequestId::Str(id),
                        };
                        pending_respond_tasks.lock().remove(&id);
                    }
                }
                continue;
            }

            let unhandled_message = {
                let mut notification_handlers = notification_handlers.lock();
                if let Some(handler) = notification_handlers.get_mut(msg.method.as_str()) {
                    handler(msg.id, msg.params.unwrap_or(Value::Null), cx);
                    None
                } else {
                    Some(msg)
                }
            };

            if let Some(msg) = unhandled_message {
                on_unhandled_notification(msg).await;
            }

            // Don't starve the main thread when receiving lots of notifications at once.
            smol::future::yield_now().await;
        }
        input_handler.loop_handle.await
    }

    async fn handle_stderr<Stderr>(
        stderr: Stderr,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        stderr_capture: Arc<Mutex<Option<String>>>,
    ) -> anyhow::Result<()>
    where
        Stderr: AsyncRead + Unpin + Send + 'static,
    {
        let mut stderr = BufReader::new(stderr);
        let mut buffer = Vec::new();

        loop {
            buffer.clear();

            let bytes_read = stderr.read_until(b'\n', &mut buffer).await?;
            if bytes_read == 0 {
                return Ok(());
            }

            if let Ok(message) = std::str::from_utf8(&buffer) {
                log::trace!("incoming stderr message:{message}");
                for handler in io_handlers.lock().values_mut() {
                    handler(IoKind::StdErr, message);
                }

                if let Some(stderr) = stderr_capture.lock().as_mut() {
                    stderr.push_str(message);
                }
            }

            // Don't starve the main thread when receiving lots of messages at once.
            smol::future::yield_now().await;
        }
    }

    async fn handle_outgoing_messages<Stdin>(
        stdin: Stdin,
        outbound_rx: channel::Receiver<String>,
        output_done_tx: barrier::Sender,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    ) -> anyhow::Result<()>
    where
        Stdin: AsyncWrite + Unpin + Send + 'static,
    {
        let mut stdin = BufWriter::new(stdin);
        let _clear_response_handlers = util::defer({
            let response_handlers = response_handlers.clone();
            move || {
                response_handlers.lock().take();
            }
        });
        let mut content_len_buffer = Vec::new();
        while let Ok(message) = outbound_rx.recv().await {
            log::trace!("outgoing message:{}", message);
            for handler in io_handlers.lock().values_mut() {
                handler(IoKind::StdIn, &message);
            }

            content_len_buffer.clear();
            write!(content_len_buffer, "{}", message.len()).unwrap();
            stdin.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
            stdin.write_all(&content_len_buffer).await?;
            stdin.write_all("\r\n\r\n".as_bytes()).await?;
            stdin.write_all(message.as_bytes()).await?;
            stdin.flush().await?;
        }
        drop(output_done_tx);
        Ok(())
    }
}
