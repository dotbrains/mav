use super::*;

impl Session {
    pub(super) fn handle_start_debugging_request(
        &mut self,
        request: dap::messages::Request,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let request_seq = request.seq;

        let launch_request: Option<Result<StartDebuggingRequestArguments, _>> = request
            .arguments
            .as_ref()
            .map(|value| serde_json::from_value(value.clone()));

        let mut success = true;
        if let Some(Ok(request)) = launch_request {
            cx.emit(SessionStateEvent::SpawnChildSession { request });
        } else {
            log::error!(
                "Failed to parse launch request arguments: {:?}",
                request.arguments
            );
            success = false;
        }

        cx.spawn(async move |this, cx| {
            this.update(cx, |this, cx| {
                this.respond_to_client(
                    request_seq,
                    success,
                    StartDebugging::COMMAND.to_string(),
                    None,
                    cx,
                )
            })?
            .await
        })
    }

    pub(super) fn handle_run_in_terminal_request(
        &mut self,
        request: dap::messages::Request,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let request_args = match serde_json::from_value::<RunInTerminalRequestArguments>(
            request.arguments.unwrap_or_default(),
        ) {
            Ok(args) => args,
            Err(error) => {
                return cx.spawn(async move |session, cx| {
                    let error = serde_json::to_value(dap::ErrorResponse {
                        error: Some(dap::Message {
                            id: request.seq,
                            format: error.to_string(),
                            variables: None,
                            send_telemetry: None,
                            show_user: None,
                            url: None,
                            url_label: None,
                        }),
                    })
                    .ok();

                    session
                        .update(cx, |this, cx| {
                            this.respond_to_client(
                                request.seq,
                                false,
                                StartDebugging::COMMAND.to_string(),
                                error,
                                cx,
                            )
                        })?
                        .await?;

                    Err(anyhow!("Failed to parse RunInTerminalRequestArguments"))
                });
            }
        };

        let seq = request.seq;

        let (tx, mut rx) = mpsc::channel::<Result<u32>>(1);
        cx.emit(SessionEvent::RunInTerminal {
            request: request_args,
            sender: tx,
        });
        cx.notify();

        cx.spawn(async move |session, cx| {
            let result = util::maybe!(async move {
                rx.next().await.ok_or_else(|| {
                    anyhow!("failed to receive response from spawn terminal".to_string())
                })?
            })
            .await;
            let (success, body) = match result {
                Ok(pid) => (
                    true,
                    serde_json::to_value(dap::RunInTerminalResponse {
                        process_id: None,
                        shell_process_id: Some(pid as u64),
                    })
                    .ok(),
                ),
                Err(error) => (
                    false,
                    serde_json::to_value(dap::ErrorResponse {
                        error: Some(dap::Message {
                            id: seq,
                            format: error.to_string(),
                            variables: None,
                            send_telemetry: None,
                            show_user: None,
                            url: None,
                            url_label: None,
                        }),
                    })
                    .ok(),
                ),
            };

            session
                .update(cx, |session, cx| {
                    session.respond_to_client(
                        seq,
                        success,
                        RunInTerminal::COMMAND.to_string(),
                        body,
                        cx,
                    )
                })?
                .await
        })
    }

    pub(super) fn request_initialize(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let adapter_id = self.adapter().to_string();
        let request = Initialize { adapter_id };

        let SessionState::Running(running) = &self.state else {
            return Task::ready(Err(anyhow!(
                "Cannot send initialize request, task still building"
            )));
        };
        let mut response = running.request(request.clone());

        cx.spawn(async move |this, cx| {
            loop {
                let capabilities = response.await;
                match capabilities {
                    Err(e) => {
                        let Ok(Some(reconnect)) = this.update(cx, |this, cx| {
                            this.as_running()
                                .and_then(|running| running.reconnect_for_ssh(&mut cx.to_async()))
                        }) else {
                            return Err(e);
                        };
                        log::info!("Failed to connect to debug adapter: {}, retrying...", e);
                        reconnect.await?;

                        let Ok(Some(r)) = this.update(cx, |this, _| {
                            this.as_running()
                                .map(|running| running.request(request.clone()))
                        }) else {
                            return Err(e);
                        };
                        response = r
                    }
                    Ok(capabilities) => {
                        this.update(cx, |session, cx| {
                            session.capabilities = capabilities;

                            cx.emit(SessionEvent::CapabilitiesLoaded);
                        })?;
                        return Ok(());
                    }
                }
            }
        })
    }

    pub(super) fn initialize_sequence(
        &mut self,
        initialize_rx: oneshot::Receiver<()>,
        dap_store: WeakEntity<DapStore>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        match &self.state {
            SessionState::Running(local_mode) => {
                local_mode.initialize_sequence(&self.capabilities, initialize_rx, dap_store, cx)
            }
            SessionState::Booting(_) => {
                Task::ready(Err(anyhow!("cannot initialize, still building")))
            }
        }
    }
}
