use super::*;
use crate::debugger::dap_store::DapStoreEvent;

fn client_source(abs_path: &Path) -> dap::Source {
    dap::Source {
        name: abs_path
            .file_name()
            .map(|filename| filename.to_string_lossy().into_owned()),
        path: Some(abs_path.to_string_lossy().into_owned()),
        source_reference: None,
        presentation_hint: None,
        origin: None,
        sources: None,
        adapter_data: None,
        checksums: None,
    }
}

#[derive(Clone)]
pub struct RunningMode {
    pub(super) client: Arc<DebugAdapterClient>,
    pub(super) binary: DebugAdapterBinary,
    pub(super) tmp_breakpoint: Option<SourceBreakpoint>,
    pub(super) worktree: WeakEntity<Worktree>,
    pub(super) executor: BackgroundExecutor,
    pub(super) is_started: bool,
    pub(super) has_ever_stopped: bool,
    messages_tx: UnboundedSender<Message>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SessionQuirks {
    pub compact: bool,
    pub prefer_thread_name: bool,
}

impl RunningMode {
    pub(super) async fn new(
        session_id: SessionId,
        parent_session: Option<Entity<Session>>,
        worktree: WeakEntity<Worktree>,
        binary: DebugAdapterBinary,
        messages_tx: futures::channel::mpsc::UnboundedSender<Message>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        let message_handler = Box::new({
            let messages_tx = messages_tx.clone();
            move |message| {
                messages_tx.unbounded_send(message).ok();
            }
        });

        let client = if let Some(client) =
            parent_session.and_then(|session| cx.update(|cx| session.read(cx).adapter_client()))
        {
            client
                .create_child_connection(session_id, binary.clone(), message_handler, cx)
                .await?
        } else {
            DebugAdapterClient::start(session_id, binary.clone(), message_handler, cx).await?
        };

        Ok(Self {
            client: Arc::new(client),
            worktree,
            tmp_breakpoint: None,
            binary,
            executor: cx.background_executor().clone(),
            is_started: false,
            has_ever_stopped: false,
            messages_tx,
        })
    }

    pub(crate) fn worktree(&self) -> &WeakEntity<Worktree> {
        &self.worktree
    }

    pub(super) fn unset_breakpoints_from_paths(
        &self,
        paths: &Vec<Arc<Path>>,
        cx: &mut App,
    ) -> Task<()> {
        let tasks: Vec<_> = paths
            .iter()
            .map(|path| {
                self.request(dap_command::SetBreakpoints {
                    source: client_source(path),
                    source_modified: None,
                    breakpoints: vec![],
                })
            })
            .collect();

        cx.background_spawn(async move {
            futures::future::join_all(tasks)
                .await
                .iter()
                .for_each(|res| match res {
                    Ok(_) => {}
                    Err(err) => {
                        log::warn!("Set breakpoints request failed: {}", err);
                    }
                });
        })
    }

    pub(super) fn send_breakpoints_from_path(
        &self,
        abs_path: Arc<Path>,
        reason: BreakpointUpdatedReason,
        breakpoint_store: &Entity<BreakpointStore>,
        cx: &mut App,
    ) -> Task<()> {
        let breakpoints =
            breakpoint_store
                .read(cx)
                .source_breakpoints_from_path(&abs_path, cx)
                .into_iter()
                .filter(|bp| bp.state.is_enabled())
                .chain(self.tmp_breakpoint.iter().filter_map(|breakpoint| {
                    breakpoint.path.eq(&abs_path).then(|| breakpoint.clone())
                }))
                .map(Into::into)
                .collect();

        let raw_breakpoints = breakpoint_store
            .read(cx)
            .breakpoints_from_path(&abs_path)
            .into_iter()
            .filter(|bp| bp.bp.state.is_enabled())
            .collect::<Vec<_>>();

        let task = self.request(dap_command::SetBreakpoints {
            source: client_source(&abs_path),
            source_modified: Some(matches!(reason, BreakpointUpdatedReason::FileSaved)),
            breakpoints,
        });
        let session_id = self.client.id();
        let breakpoint_store = breakpoint_store.downgrade();
        cx.spawn(async move |cx| match cx.background_spawn(task).await {
            Ok(breakpoints) => {
                let breakpoints =
                    breakpoints
                        .into_iter()
                        .zip(raw_breakpoints)
                        .filter_map(|(dap_bp, mav_bp)| {
                            Some((
                                mav_bp,
                                BreakpointSessionState {
                                    id: dap_bp.id?,
                                    verified: dap_bp.verified,
                                },
                            ))
                        });
                breakpoint_store
                    .update(cx, |this, _| {
                        this.mark_breakpoints_verified(session_id, &abs_path, breakpoints);
                    })
                    .ok();
            }
            Err(err) => log::warn!("Set breakpoints request failed for path: {}", err),
        })
    }

    pub(super) fn send_exception_breakpoints(
        &self,
        filters: Vec<ExceptionBreakpointsFilter>,
        supports_filter_options: bool,
    ) -> Task<Result<Vec<dap::Breakpoint>>> {
        let arg = if supports_filter_options {
            SetExceptionBreakpoints::WithOptions {
                filters: filters
                    .into_iter()
                    .map(|filter| ExceptionFilterOptions {
                        filter_id: filter.filter,
                        condition: None,
                        mode: None,
                    })
                    .collect(),
            }
        } else {
            SetExceptionBreakpoints::Plain {
                filters: filters.into_iter().map(|filter| filter.filter).collect(),
            }
        };
        self.request(arg)
    }

    pub(super) fn send_source_breakpoints(
        &self,
        ignore_breakpoints: bool,
        breakpoint_store: &Entity<BreakpointStore>,
        cx: &App,
    ) -> Task<HashMap<Arc<Path>, anyhow::Error>> {
        let mut breakpoint_tasks = Vec::new();
        let breakpoints = breakpoint_store.read(cx).all_source_breakpoints(cx);
        let mut raw_breakpoints = breakpoint_store.read_with(cx, |this, _| this.all_breakpoints());
        debug_assert_eq!(raw_breakpoints.len(), breakpoints.len());
        let session_id = self.client.id();
        for (path, breakpoints) in breakpoints {
            let breakpoints = if ignore_breakpoints {
                vec![]
            } else {
                breakpoints
                    .into_iter()
                    .filter(|bp| bp.state.is_enabled())
                    .map(Into::into)
                    .collect()
            };

            let raw_breakpoints = raw_breakpoints
                .remove(&path)
                .unwrap_or_default()
                .into_iter()
                .filter(|bp| bp.bp.state.is_enabled());
            let error_path = path.clone();
            let send_request = self
                .request(dap_command::SetBreakpoints {
                    source: client_source(&path),
                    source_modified: Some(false),
                    breakpoints,
                })
                .map(|result| result.map_err(move |e| (error_path, e)));

            let task = cx.spawn({
                let breakpoint_store = breakpoint_store.downgrade();
                async move |cx| {
                    let breakpoints = cx.background_spawn(send_request).await?;

                    let breakpoints = breakpoints.into_iter().zip(raw_breakpoints).filter_map(
                        |(dap_bp, mav_bp)| {
                            Some((
                                mav_bp,
                                BreakpointSessionState {
                                    id: dap_bp.id?,
                                    verified: dap_bp.verified,
                                },
                            ))
                        },
                    );
                    breakpoint_store
                        .update(cx, |this, _| {
                            this.mark_breakpoints_verified(session_id, &path, breakpoints);
                        })
                        .ok();

                    Ok(())
                }
            });
            breakpoint_tasks.push(task);
        }

        cx.background_spawn(async move {
            futures::future::join_all(breakpoint_tasks)
                .await
                .into_iter()
                .filter_map(Result::err)
                .collect::<HashMap<_, _>>()
        })
    }

    pub(super) fn initialize_sequence(
        &self,
        capabilities: &Capabilities,
        initialized_rx: oneshot::Receiver<()>,
        dap_store: WeakEntity<DapStore>,
        cx: &mut Context<Session>,
    ) -> Task<Result<()>> {
        let raw = self.binary.request_args.clone();

        // Of relevance: https://github.com/microsoft/vscode/issues/4902#issuecomment-368583522
        let launch = match raw.request {
            dap::StartDebuggingRequestArgumentsRequest::Launch => self.request(Launch {
                raw: raw.configuration,
            }),
            dap::StartDebuggingRequestArgumentsRequest::Attach => self.request(Attach {
                raw: raw.configuration,
            }),
        };

        let configuration_done_supported = ConfigurationDone::is_supported(capabilities);
        // From spec (on initialization sequence):
        // client sends a setExceptionBreakpoints request if one or more exceptionBreakpointFilters have been defined (or if supportsConfigurationDoneRequest is not true)
        //
        // Thus we should send setExceptionBreakpoints even if `exceptionFilters` variable is empty (as long as there were some options in the first place).
        let should_send_exception_breakpoints = capabilities
            .exception_breakpoint_filters
            .as_ref()
            .is_some_and(|filters| !filters.is_empty())
            || !configuration_done_supported;
        let supports_exception_filters = capabilities
            .supports_exception_filter_options
            .unwrap_or_default();
        let this = self.clone();
        let worktree = self.worktree().clone();
        let mut filters = capabilities
            .exception_breakpoint_filters
            .clone()
            .unwrap_or_default();
        let configuration_sequence = cx.spawn({
            async move |session, cx| {
                let adapter_name = session.read_with(cx, |this, _| this.adapter())?;
                let (breakpoint_store, adapter_defaults) =
                    dap_store.read_with(cx, |dap_store, _| {
                        (
                            dap_store.breakpoint_store().clone(),
                            dap_store.adapter_options(&adapter_name),
                        )
                    })?;
                initialized_rx.await?;
                let errors_by_path = cx
                    .update(|cx| this.send_source_breakpoints(false, &breakpoint_store, cx))
                    .await;

                dap_store.update(cx, |_, cx| {
                    let Some(worktree) = worktree.upgrade() else {
                        return;
                    };

                    for (path, error) in &errors_by_path {
                        log::error!("failed to set breakpoints for {path:?}: {error}");
                    }

                    if let Some(failed_path) = errors_by_path.keys().next() {
                        let failed_path = failed_path
                            .strip_prefix(worktree.read(cx).abs_path())
                            .unwrap_or(failed_path)
                            .display();
                        let message = format!(
                            "Failed to set breakpoints for {failed_path}{}",
                            match errors_by_path.len() {
                                0 => unreachable!(),
                                1 => "".into(),
                                2 => " and 1 other path".into(),
                                n => format!(" and {} other paths", n - 1),
                            }
                        );
                        cx.emit(DapStoreEvent::Notification(message));
                    }
                })?;

                if should_send_exception_breakpoints {
                    _ = session.update(cx, |this, _| {
                        filters.retain(|filter| {
                            let is_enabled = if let Some(defaults) = adapter_defaults.as_ref() {
                                defaults
                                    .exception_breakpoints
                                    .get(&filter.filter)
                                    .map(|options| options.enabled)
                                    .unwrap_or_else(|| filter.default.unwrap_or_default())
                            } else {
                                filter.default.unwrap_or_default()
                            };
                            this.exception_breakpoints
                                .entry(filter.filter.clone())
                                .or_insert_with(|| (filter.clone(), is_enabled));
                            is_enabled
                        });
                    });

                    this.send_exception_breakpoints(filters, supports_exception_filters)
                        .await
                        .ok();
                }

                if configuration_done_supported {
                    this.request(ConfigurationDone {})
                } else {
                    Task::ready(Ok(()))
                }
                .await
            }
        });

        let task = cx.background_spawn(futures::future::try_join(launch, configuration_sequence));

        cx.spawn(async move |this, cx| {
            let result = task.await;

            this.update(cx, |this, cx| {
                if let Some(this) = this.as_running_mut() {
                    this.is_started = true;
                    cx.notify();
                }
            })
            .ok();

            result?;
            anyhow::Ok(())
        })
    }

    pub(super) fn reconnect_for_ssh(&self, cx: &mut AsyncApp) -> Option<Task<Result<()>>> {
        let client = self.client.clone();
        let messages_tx = self.messages_tx.clone();
        let message_handler = Box::new(move |message| {
            messages_tx.unbounded_send(message).ok();
        });
        if client.should_reconnect_for_ssh() {
            Some(cx.spawn(async move |cx| {
                client.connect(message_handler, cx).await?;
                anyhow::Ok(())
            }))
        } else {
            None
        }
    }

    pub(super) fn request<R: LocalDapCommand>(&self, request: R) -> Task<Result<R::Response>>
    where
        <R::DapRequest as dap::requests::Request>::Response: 'static,
        <R::DapRequest as dap::requests::Request>::Arguments: 'static + Send,
    {
        let request = Arc::new(request);

        let request_clone = request.clone();
        let connection = self.client.clone();
        self.executor.spawn(async move {
            let args = request_clone.to_dap();
            let response = connection.request::<R::DapRequest>(args).await?;
            request.response_from_dap(response)
        })
    }
}
