use super::*;

impl Copilot {
    pub fn is_authenticated(&self) -> bool {
        return matches!(
            self.server,
            CopilotServer::Running(RunningCopilotServer {
                sign_in_status: SignInStatus::Authorized,
                ..
            })
        );
    }
    pub fn sign_in(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        if let CopilotServer::Running(server) = &mut self.server {
            let task = match &server.sign_in_status {
                SignInStatus::Authorized => Task::ready(Ok(())).shared(),
                SignInStatus::SigningIn { task, .. } => {
                    cx.notify();
                    task.clone()
                }
                SignInStatus::SignedOut { .. } | SignInStatus::Unauthorized => {
                    let lsp = server.lsp.clone();

                    let request_timeout = ProjectSettings::get_global(cx)
                        .global_lsp_settings
                        .get_request_timeout();

                    let task = cx
                        .spawn(async move |this, cx| {
                            let sign_in = async {
                                let flow = lsp
                                    .request::<request::SignIn>(
                                        request::SignInParams {},
                                        request_timeout,
                                    )
                                    .await
                                    .into_response()
                                    .context("copilot sign-in")?;

                                this.update(cx, |this, cx| {
                                    if let CopilotServer::Running(RunningCopilotServer {
                                        sign_in_status: status,
                                        ..
                                    }) = &mut this.server
                                        && let SignInStatus::SigningIn {
                                            prompt: prompt_flow,
                                            ..
                                        } = status
                                    {
                                        *prompt_flow = Some(flow.clone());
                                        cx.notify();
                                    }
                                })?;

                                anyhow::Ok(())
                            };

                            let sign_in = sign_in.await;
                            this.update(cx, |this, cx| match sign_in {
                                Ok(()) => Ok(()),
                                Err(error) => {
                                    this.update_sign_in_status(
                                        request::SignInStatus::NotSignedIn,
                                        cx,
                                    );
                                    Err(Arc::new(error))
                                }
                            })?
                        })
                        .shared();
                    server.sign_in_status = SignInStatus::SigningIn {
                        prompt: None,
                        task: task.clone(),
                    };
                    cx.notify();
                    task
                }
            };

            cx.background_spawn(task.map_err(|err| anyhow!("{err:?}")))
        } else {
            // If we're downloading, wait until download is finished
            // If we're in a stuck state, display to the user
            Task::ready(Err(anyhow!("copilot hasn't started yet")))
        }
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        self.update_sign_in_status(request::SignInStatus::NotSignedIn, cx);
        match &self.server {
            CopilotServer::Running(RunningCopilotServer { lsp: server, .. }) => {
                let request_timeout = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .get_request_timeout();

                let server = server.clone();
                cx.background_spawn(async move {
                    server
                        .request::<request::SignOut>(request::SignOutParams {}, request_timeout)
                        .await
                        .into_response()
                        .context("copilot: sign in confirm")?;
                    anyhow::Ok(())
                })
            }
            CopilotServer::Disabled => cx.background_spawn(async {
                clear_copilot_config_dir().await;
                anyhow::Ok(())
            }),
            _ => Task::ready(Err(anyhow!("copilot hasn't started yet"))),
        }
    }

    pub fn reinstall(&mut self, cx: &mut Context<Self>) -> Shared<Task<()>> {
        let language_settings = all_language_settings(None, cx);
        let env = self.build_env(&language_settings.edit_predictions.copilot);
        let start_task = cx
            .spawn({
                let fs = self.fs.clone();
                let node_runtime = self.node_runtime.clone();
                let server_id = self.server_id;
                async move |this, cx| {
                    clear_copilot_dir().await;
                    Self::start_language_server(server_id, fs, node_runtime, env, this, false, cx)
                        .await
                }
            })
            .shared();

        self.server = CopilotServer::Starting {
            task: start_task.clone(),
        };

        cx.notify();

        start_task
    }
}
