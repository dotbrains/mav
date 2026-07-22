use super::*;

impl Copilot {
    pub fn new(
        project: Option<Entity<Project>>,
        new_server_id: LanguageServerId,
        fs: Arc<dyn Fs>,
        node_runtime: NodeRuntime,
        cx: &mut Context<Self>,
    ) -> Self {
        let send_focus_notification = project.map(|project| {
            cx.subscribe(&project, |this, project, e: &project::Event, cx| {
                if let project::Event::ActiveEntryChanged(new_entry) = e
                    && let Ok(running) = this.server.as_authenticated()
                {
                    let uri = new_entry
                        .and_then(|id| project.read(cx).path_for_entry(id, cx))
                        .and_then(|entry| project.read(cx).absolute_path(&entry, cx))
                        .and_then(|abs_path| lsp::Uri::from_file_path(abs_path).ok());

                    _ = running.lsp.notify::<DidFocus>(DidFocusParams { uri });
                }
            })
        });
        let global_authentication_events =
            cx.try_global::<GlobalCopilotAuth>().cloned().map(|auth| {
                cx.subscribe(&auth.0, |_, _, _: &Event, cx| {
                    let request_timeout = ProjectSettings::get_global(cx)
                        .global_lsp_settings
                        .get_request_timeout();
                    cx.spawn(async move |this, cx| {
                        let Some(server) = this
                            .update(cx, |this, _| this.language_server().cloned())
                            .ok()
                            .flatten()
                        else {
                            return;
                        };
                        let status = server
                            .request::<request::CheckStatus>(
                                request::CheckStatusParams {
                                    local_checks_only: false,
                                },
                                request_timeout,
                            )
                            .await
                            .into_response()
                            .ok();
                        if let Some(status) = status {
                            this.update(cx, |copilot, cx| {
                                copilot.update_sign_in_status(status, cx);
                            })
                            .ok();
                        }
                    })
                    .detach()
                })
            });
        let _subscriptions = std::iter::once(cx.on_app_quit(Self::shutdown_language_server))
            .chain(send_focus_notification)
            .chain(global_authentication_events)
            .collect();
        let mut this = Self {
            server_id: new_server_id,
            fs,
            node_runtime,
            server: CopilotServer::Disabled,
            buffers: Default::default(),
            _subscriptions,
        };
        this.start_copilot(true, false, cx);
        cx.observe_global::<SettingsStore>(move |this, cx| {
            let ai_disabled = DisableAiSettings::get_global(cx).disable_ai;

            if ai_disabled {
                // Stop the server if AI is disabled
                if !matches!(this.server, CopilotServer::Disabled) {
                    let shutdown = match mem::replace(&mut this.server, CopilotServer::Disabled) {
                        CopilotServer::Running(server) => {
                            let shutdown_future = server.lsp.shutdown();
                            Some(cx.background_spawn(async move {
                                if let Some(fut) = shutdown_future {
                                    fut.await;
                                }
                            }))
                        }
                        _ => None,
                    };
                    if let Some(task) = shutdown {
                        task.detach();
                    }
                    cx.notify();
                }
            } else {
                // Only start if AI is enabled
                this.start_copilot(true, false, cx);
                if let Ok(server) = this.server.as_running() {
                    notify_did_change_config_to_server(&server.lsp, cx)
                        .context("copilot setting change: did change configuration")
                        .log_err();
                }
            }
            this.update_action_visibilities(cx);
        })
        .detach();
        cx.observe_self(|copilot, cx| {
            copilot.update_action_visibilities(cx);
        })
        .detach();
        this
    }

    fn shutdown_language_server(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let shutdown = match mem::replace(&mut self.server, CopilotServer::Disabled) {
            CopilotServer::Running(server) => Some(Box::pin(async move { server.lsp.shutdown() })),
            _ => None,
        };

        async move {
            if let Some(shutdown) = shutdown {
                shutdown.await;
            }
        }
    }

    pub fn start_copilot(
        &mut self,
        check_edit_prediction_provider: bool,
        awaiting_sign_in_after_start: bool,
        cx: &mut Context<Self>,
    ) {
        if DisableAiSettings::get_global(cx).disable_ai {
            return;
        }
        if !matches!(self.server, CopilotServer::Disabled) {
            return;
        }
        let language_settings = all_language_settings(None, cx);
        if check_edit_prediction_provider
            && language_settings.edit_predictions.provider != EditPredictionProvider::Copilot
        {
            return;
        }
        let server_id = self.server_id;
        let fs = self.fs.clone();
        let node_runtime = self.node_runtime.clone();
        let env = self.build_env(&language_settings.edit_predictions.copilot);
        let start_task = cx
            .spawn(async move |this, cx| {
                Self::start_language_server(
                    server_id,
                    fs,
                    node_runtime,
                    env,
                    this,
                    awaiting_sign_in_after_start,
                    cx,
                )
                .await
            })
            .shared();
        self.server = CopilotServer::Starting { task: start_task };
        cx.notify();
    }

    pub(super) fn build_env(
        &self,
        copilot_settings: &CopilotSettings,
    ) -> Option<HashMap<String, String>> {
        let proxy_url = copilot_settings.proxy.clone()?;
        let no_verify = copilot_settings.proxy_no_verify;
        let http_or_https_proxy = if proxy_url.starts_with("http:") {
            Some("HTTP_PROXY")
        } else if proxy_url.starts_with("https:") {
            Some("HTTPS_PROXY")
        } else {
            log::error!(
                "Unsupported protocol scheme for language server proxy (must be http or https)"
            );
            None
        };

        let mut env = HashMap::default();

        if let Some(proxy_type) = http_or_https_proxy {
            env.insert(proxy_type.to_string(), proxy_url);
            if let Some(true) = no_verify {
                env.insert("NODE_TLS_REJECT_UNAUTHORIZED".to_string(), "0".to_string());
            };
        }

        for env_var in [
            copilot_chat::COPILOT_OAUTH_ENV_VAR,
            copilot_chat::GITHUB_COPILOT_OAUTH_ENV_VAR,
        ] {
            if let Ok(oauth_token) = env::var(env_var) {
                env.insert(env_var.to_string(), oauth_token);
                break;
            }
        }

        if env.is_empty() { None } else { Some(env) }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn fake(cx: &mut gpui::TestAppContext) -> (Entity<Self>, lsp::FakeLanguageServer) {
        use fs::FakeFs;
        use gpui::Subscription;
        use lsp::FakeLanguageServer;
        use node_runtime::NodeRuntime;

        let (server, fake_server) = FakeLanguageServer::new(
            LanguageServerId(0),
            LanguageServerBinary {
                path: "path/to/copilot".into(),
                arguments: vec![],
                env: None,
            },
            "copilot".into(),
            Default::default(),
            &mut cx.to_async(),
        );
        let node_runtime = NodeRuntime::unavailable();
        let send_focus_notification = Subscription::new(|| {});
        let this = cx.new(|cx| Self {
            server_id: LanguageServerId(0),
            fs: FakeFs::new(cx.background_executor().clone()),
            node_runtime,
            server: CopilotServer::Running(RunningCopilotServer {
                lsp: Arc::new(server),
                sign_in_status: SignInStatus::Authorized,
                registered_buffers: Default::default(),
            }),
            _subscriptions: vec![
                send_focus_notification,
                cx.on_app_quit(Self::shutdown_language_server),
            ],
            buffers: Default::default(),
        });
        (this, fake_server)
    }

    pub(super) async fn start_language_server(
        new_server_id: LanguageServerId,
        fs: Arc<dyn Fs>,
        node_runtime: NodeRuntime,
        env: Option<HashMap<String, String>>,
        this: WeakEntity<Self>,
        awaiting_sign_in_after_start: bool,
        cx: &mut AsyncApp,
    ) {
        let start_language_server = async {
            let server_path = get_copilot_lsp(fs, node_runtime).await?;

            let arguments: Vec<OsString> = vec!["--stdio".into()];
            let binary = LanguageServerBinary {
                path: server_path,
                arguments,
                env,
            };

            let root_path = if cfg!(target_os = "windows") {
                Path::new("C:/")
            } else {
                Path::new("/")
            };

            let server_name = LanguageServerName("copilot".into());
            let server = LanguageServer::new(
                Arc::new(Mutex::new(None)),
                new_server_id,
                server_name,
                binary,
                root_path,
                None,
                Default::default(),
                cx,
            )?;

            server
                .on_notification::<DidChangeStatus, _>({
                    let this = this.clone();
                    move |params, cx| {
                        if params.kind == request::StatusKind::Normal {
                            let this = this.clone();
                            cx.spawn(async move |cx| {
                                let lsp = this
                                    .read_with(cx, |copilot, _| {
                                        if let CopilotServer::Running(server) = &copilot.server {
                                            Some(server.lsp.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .ok()
                                    .flatten();
                                let Some(lsp) = lsp else { return };
                                let request_timeout = cx.update(|cx| {
                                    ProjectSettings::get_global(cx)
                                        .global_lsp_settings
                                        .get_request_timeout()
                                });
                                let status = lsp
                                    .request::<request::CheckStatus>(
                                        request::CheckStatusParams {
                                            local_checks_only: false,
                                        },
                                        request_timeout,
                                    )
                                    .await
                                    .into_response()
                                    .ok();
                                if let Some(status) = status {
                                    this.update(cx, |copilot, cx| {
                                        copilot.update_sign_in_status(status, cx);
                                    })
                                    .ok();
                                }
                            })
                            .detach();
                        }
                    }
                })
                .detach();

            server
                .on_request::<lsp::request::ShowDocument, _, _>(move |params, cx| {
                    if params.external.unwrap_or(false) {
                        let url = params.uri.to_string();
                        cx.update(|cx| cx.open_url(&url));
                    }
                    async move { Ok(lsp::ShowDocumentResult { success: true }) }
                })
                .detach();

            let configuration = lsp::DidChangeConfigurationParams {
                settings: Default::default(),
            };

            let editor_info = request::SetEditorInfoParams {
                editor_info: request::EditorInfo {
                    name: "mav".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
                editor_plugin_info: request::EditorPluginInfo {
                    name: "mav-copilot".into(),
                    version: "0.0.1".into(),
                },
            };
            let editor_info_json = serde_json::to_value(&editor_info)?;

            let request_timeout = cx.update(|app| {
                ProjectSettings::get_global(app)
                    .global_lsp_settings
                    .get_request_timeout()
            });

            let server = cx
                .update(|cx| {
                    let mut params = server.default_initialize_params(false, false, cx);
                    params.initialization_options = Some(editor_info_json);
                    params
                        .capabilities
                        .window
                        .get_or_insert_with(Default::default)
                        .show_document =
                        Some(lsp::ShowDocumentClientCapabilities { support: true });
                    server.initialize(params, configuration.into(), request_timeout, cx)
                })
                .await?;

            this.update(cx, |_, cx| notify_did_change_config_to_server(&server, cx))?
                .context("copilot: did change configuration")?;

            let status = server
                .request::<request::CheckStatus>(
                    request::CheckStatusParams {
                        local_checks_only: false,
                    },
                    request_timeout,
                )
                .await
                .into_response()
                .context("copilot: check status")?;

            anyhow::Ok((server, status))
        };

        let server = start_language_server.await;
        this.update(cx, |this, cx| {
            cx.notify();

            if env::var("MAV_FORCE_COPILOT_ERROR").is_ok() {
                this.server = CopilotServer::Error(
                    "Forced error for testing (MAV_FORCE_COPILOT_ERROR)".into(),
                );
                return;
            }

            match server {
                Ok((server, status)) => {
                    this.server = CopilotServer::Running(RunningCopilotServer {
                        lsp: server,
                        sign_in_status: SignInStatus::SignedOut {
                            awaiting_signing_in: awaiting_sign_in_after_start,
                        },
                        registered_buffers: Default::default(),
                    });
                    this.update_sign_in_status(status, cx);
                }
                Err(error) => {
                    this.server = CopilotServer::Error(error.to_string().into());
                    cx.notify()
                }
            }
        })
        .ok();
    }
}
