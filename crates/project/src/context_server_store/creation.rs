use super::*;

impl ContextServerStore {
    pub async fn create_context_server(
        this: WeakEntity<Self>,
        id: ContextServerId,
        configuration: Arc<ContextServerConfiguration>,
        cx: &mut AsyncApp,
    ) -> Result<(Arc<ContextServer>, Arc<ContextServerConfiguration>)> {
        let remote = configuration.remote();
        let needs_remote_command = match configuration.as_ref() {
            ContextServerConfiguration::Custom { .. }
            | ContextServerConfiguration::Extension { .. } => remote,
            ContextServerConfiguration::Http { .. } => false,
        };

        let (remote_state, is_remote_project) = this.update(cx, |this, _| {
            let remote_state = match &this.state {
                ContextServerStoreState::Remote {
                    project_id,
                    upstream_client,
                } if needs_remote_command => Some((*project_id, upstream_client.clone())),
                _ => None,
            };
            (remote_state, this.is_remote_project())
        })?;

        let root_path: Option<Arc<Path>> = this.update(cx, |this, cx| {
            this.project
                .as_ref()
                .and_then(|project| {
                    project
                        .read_with(cx, |project, cx| project.active_project_directory(cx))
                        .ok()
                        .flatten()
                })
                .or_else(|| {
                    this.worktree_store.read_with(cx, |store, cx| {
                        store.visible_worktrees(cx).fold(None, |acc, item| {
                            if acc.is_none() {
                                item.read(cx).root_dir()
                            } else {
                                acc
                            }
                        })
                    })
                })
        })?;

        let configuration = if let Some((project_id, upstream_client)) = remote_state {
            let root_dir = root_path.as_ref().map(|p| p.display().to_string());

            let response = upstream_client
                .update(cx, |client, _| {
                    client
                        .proto_client()
                        .request(proto::GetContextServerCommand {
                            project_id,
                            server_id: id.0.to_string(),
                            root_dir: root_dir.clone(),
                        })
                })
                .await?;

            let remote_command = upstream_client.update(cx, |client, _| {
                client.build_command(
                    Some(response.path),
                    &response.args,
                    &response.env.into_iter().collect(),
                    root_dir,
                    None,
                    Interactive::Yes,
                )
            })?;

            let command = ContextServerCommand {
                path: remote_command.program.into(),
                args: remote_command.args,
                env: Some(remote_command.env.into_iter().collect()),
                timeout: None,
            };

            Arc::new(ContextServerConfiguration::Custom { command, remote })
        } else {
            configuration
        };

        if let Some(server) = this.update(cx, |this, _| {
            this.context_server_factory
                .as_ref()
                .map(|factory| factory(id.clone(), configuration.clone()))
        })? {
            return Ok((server, configuration));
        }

        let cached_token_provider: Option<Arc<dyn oauth::OAuthTokenProvider>> =
            if let ContextServerConfiguration::Http { url, .. } = configuration.as_ref() {
                if configuration.has_static_auth_header() {
                    None
                } else {
                    let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
                    let http_client = cx.update(|cx| cx.http_client());

                    match Self::load_session(&credentials_provider, url, &cx).await {
                        Ok(Some(session)) => {
                            log::info!("{} loaded cached OAuth session from keychain", id);
                            Some(Self::create_oauth_token_provider(
                                &id,
                                url,
                                session,
                                http_client,
                                credentials_provider,
                                cx,
                            ))
                        }
                        Ok(None) => None,
                        Err(err) => {
                            log::warn!("{} failed to load cached OAuth session: {}", id, err);
                            None
                        }
                    }
                }
            } else {
                None
            };

        let server: Arc<ContextServer> = this.update(cx, |this, cx| {
            let global_timeout =
                Self::resolve_project_settings(&this.worktree_store, cx).context_server_timeout;

            match configuration.as_ref() {
                ContextServerConfiguration::Http {
                    url,
                    headers,
                    timeout,
                    oauth: _,
                } => {
                    let transport = HttpTransport::new_with_token_provider(
                        cx.http_client(),
                        url.to_string(),
                        headers.clone(),
                        cx.background_executor().clone(),
                        cached_token_provider.clone(),
                    );
                    anyhow::Ok(Arc::new(ContextServer::new_with_timeout(
                        id,
                        Arc::new(transport),
                        Some(Duration::from_secs(
                            timeout.unwrap_or(global_timeout).min(MAX_TIMEOUT_SECS),
                        )),
                    )))
                }
                _ => {
                    let mut command = configuration
                        .command()
                        .context("Missing command configuration for stdio context server")?
                        .clone();
                    command.timeout = Some(
                        command
                            .timeout
                            .unwrap_or(global_timeout)
                            .min(MAX_TIMEOUT_SECS),
                    );

                    // Don't pass remote paths as working directory for locally-spawned processes
                    let working_directory = if is_remote_project { None } else { root_path };
                    anyhow::Ok(Arc::new(ContextServer::stdio(
                        id,
                        command,
                        working_directory,
                    )))
                }
            }
        })??;

        Ok((server, configuration))
    }

    pub(super) async fn handle_get_context_server_command(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetContextServerCommand>,
        mut cx: AsyncApp,
    ) -> Result<proto::ContextServerCommand> {
        let server_id = ContextServerId(envelope.payload.server_id.into());

        let (settings, registry, worktree_store) = this.update(&mut cx, |this, inner_cx| {
            let ContextServerStoreState::Local {
                is_headless: true, ..
            } = &this.state
            else {
                anyhow::bail!("unexpected GetContextServerCommand request in a non-local project");
            };

            let settings = this
                .context_server_settings
                .get(&server_id.0)
                .cloned()
                .or_else(|| {
                    this.registry
                        .read(inner_cx)
                        .context_server_descriptor(&server_id.0)
                        .map(|_| ContextServerSettings::default_extension())
                })
                .with_context(|| format!("context server `{}` not found", server_id))?;

            anyhow::Ok((settings, this.registry.clone(), this.worktree_store.clone()))
        })?;

        let configuration = ContextServerConfiguration::from_settings(
            settings,
            server_id.clone(),
            registry,
            worktree_store,
            &cx,
        )
        .await
        .with_context(|| format!("failed to build configuration for `{}`", server_id))?;

        let command = configuration
            .command()
            .context("context server has no command (HTTP servers don't need RPC)")?;

        Ok(proto::ContextServerCommand {
            path: command.path.display().to_string(),
            args: command.args.clone(),
            env: command
                .env
                .clone()
                .map(|env| env.into_iter().collect())
                .unwrap_or_default(),
        })
    }

    pub(super) fn resolve_project_settings<'a>(
        worktree_store: &'a Entity<WorktreeStore>,
        cx: &'a App,
    ) -> &'a ProjectSettings {
        let location = worktree_store
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .map(|worktree| settings::SettingsLocation {
                worktree_id: worktree.read(cx).id(),
                path: RelPath::empty(),
            });
        ProjectSettings::get(location, cx)
    }

    pub(super) fn create_oauth_token_provider(
        id: &ContextServerId,
        server_url: &url::Url,
        session: OAuthSession,
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut AsyncApp,
    ) -> Arc<dyn oauth::OAuthTokenProvider> {
        let (token_refresh_tx, mut token_refresh_rx) = futures::channel::mpsc::unbounded();
        let id = id.clone();
        let server_url = server_url.clone();

        cx.spawn(async move |cx| {
            while let Some(refreshed_session) = token_refresh_rx.next().await {
                if let Err(err) =
                    Self::store_session(&credentials_provider, &server_url, &refreshed_session, &cx)
                        .await
                {
                    log::warn!("{} failed to persist refreshed OAuth session: {}", id, err);
                }
            }
            log::debug!("{} OAuth session persistence task ended", id);
        })
        .detach();

        Arc::new(McpOAuthTokenProvider::new(
            session,
            http_client,
            Some(token_refresh_tx),
        ))
    }
}
