use super::*;

impl ContextServerStore {
    /// Initiate the OAuth browser flow for a server in the `AuthRequired` state.
    ///
    /// This starts a loopback HTTP callback server on an ephemeral port, builds
    /// the authorization URL, opens the user's browser, waits for the callback,
    /// exchanges the code for tokens, persists them in the keychain, and restarts
    /// the server with the new token provider.
    pub fn authenticate_server(
        &mut self,
        id: &ContextServerId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let state = self.servers.get(id).context("Context server not found")?;

        let (discovery, server, configuration) = match state {
            ContextServerState::AuthRequired {
                discovery,
                server,
                configuration,
            } => (discovery.clone(), server.clone(), configuration.clone()),
            _ => anyhow::bail!("Server is not in AuthRequired state"),
        };

        let needs_keychain_check = match configuration.as_ref() {
            ContextServerConfiguration::Http {
                url,
                oauth: Some(oauth_settings),
                ..
            } if oauth_settings.client_secret.is_none() => Some(url.clone()),
            _ => None,
        };

        let id = id.clone();

        let task = cx.spawn({
            let id = id.clone();
            let server = server.clone();
            let configuration = configuration.clone();
            async move |this, cx| {
                if let Some(server_url) = needs_keychain_check {
                    let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
                    let has_keychain_secret =
                        Self::load_client_secret(&credentials_provider, &server_url, cx)
                            .await
                            .ok()
                            .flatten()
                            .is_some();

                    if !has_keychain_secret {
                        this.update(cx, |this, cx| {
                            this.update_server_state(
                                id.clone(),
                                ContextServerState::ClientSecretRequired {
                                    server,
                                    configuration,
                                    discovery,
                                    error: None,
                                },
                                cx,
                            );
                        })
                        .log_err();
                        return;
                    }
                }

                let result = Self::run_oauth_flow(
                    this.clone(),
                    id.clone(),
                    discovery.clone(),
                    configuration.clone(),
                    cx,
                )
                .await;

                if let Err(err) = &result {
                    log::error!("{} OAuth authentication failed: {:?}", id, err);
                    this.update(cx, |this, cx| {
                        this.update_server_state(
                            id.clone(),
                            ContextServerState::Error {
                                server,
                                configuration,
                                error: format!("{err:#}").into(),
                            },
                            cx,
                        )
                    })
                    .log_err();
                }
            }
        });

        self.update_server_state(
            id,
            ContextServerState::Authenticating {
                server,
                configuration,
                _task: task,
            },
            cx,
        );

        Ok(())
    }

    /// Store the client secret and proceed with authentication.
    pub fn submit_client_secret(
        &mut self,
        id: &ContextServerId,
        secret: String,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let state = self.servers.get(id).context("Context server not found")?;

        let (server, configuration, discovery) = match state {
            ContextServerState::ClientSecretRequired {
                server,
                configuration,
                discovery,
                ..
            } => (server.clone(), configuration.clone(), discovery.clone()),
            _ => anyhow::bail!("Server is not in ClientSecretRequired state"),
        };

        let server_url = match configuration.as_ref() {
            ContextServerConfiguration::Http { url, .. } => url.clone(),
            _ => anyhow::bail!("OAuth only supported for HTTP servers"),
        };

        let id = id.clone();

        let task = cx.spawn({
            let id = id.clone();
            let server = server.clone();
            let configuration = configuration.clone();
            async move |this, cx| {
                // Store the secret if non-empty (empty means public client / skip).
                if !secret.is_empty() {
                    let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
                    if let Err(err) =
                        Self::store_client_secret(&credentials_provider, &server_url, &secret, cx)
                            .await
                    {
                        log::error!(
                            "{} failed to store client secret in keychain: {:?}",
                            id,
                            err
                        );
                    }
                }

                let result = Self::run_oauth_flow(
                    this.clone(),
                    id.clone(),
                    discovery.clone(),
                    configuration.clone(),
                    cx,
                )
                .await;

                if let Err(err) = &result {
                    log::error!("{} OAuth authentication failed: {:?}", id, err);

                    let is_bad_client_credentials = err
                        .downcast_ref::<oauth::OAuthTokenError>()
                        .is_some_and(|e| e.error == "unauthorized_client");

                    if is_bad_client_credentials {
                        // Clear the bad secret from the keychain so the user
                        // gets a fresh prompt.
                        let credentials_provider =
                            cx.update(|cx| mav_credentials_provider::global(cx));
                        Self::clear_client_secret(&credentials_provider, &server_url, cx)
                            .await
                            .log_err();

                        this.update(cx, |this, cx| {
                            this.update_server_state(
                                id.clone(),
                                ContextServerState::ClientSecretRequired {
                                    server,
                                    configuration,
                                    discovery,
                                    error: Some(format!("{err:#}").into()),
                                },
                                cx,
                            );
                        })
                        .log_err();
                    } else {
                        this.update(cx, |this, cx| {
                            this.update_server_state(
                                id.clone(),
                                ContextServerState::Error {
                                    server,
                                    configuration,
                                    error: format!("{err:#}").into(),
                                },
                                cx,
                            )
                        })
                        .log_err();
                    }
                }
            }
        });

        self.update_server_state(
            id,
            ContextServerState::Authenticating {
                server,
                configuration,
                _task: task,
            },
            cx,
        );

        Ok(())
    }

    async fn run_oauth_flow(
        this: WeakEntity<Self>,
        id: ContextServerId,
        discovery: Arc<OAuthDiscovery>,
        configuration: Arc<ContextServerConfiguration>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let resource = oauth::canonical_server_uri(&discovery.resource_metadata.resource);
        let pkce = oauth::generate_pkce_challenge();

        let mut state_bytes = [0u8; 32];
        rand::rng().fill(&mut state_bytes);
        let state_param: String = state_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        // Start a loopback HTTP server on an ephemeral port. The redirect URI
        // includes this port so the browser sends the callback directly to our
        // process.
        let (redirect_uri, callback_rx) =
            oauth::start_callback_server().context("Failed to start OAuth callback server")?;

        let http_client = cx.update(|cx| cx.http_client());
        let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
        let server_url = match configuration.as_ref() {
            ContextServerConfiguration::Http { url, .. } => url.clone(),
            _ => anyhow::bail!("OAuth authentication only supported for HTTP servers"),
        };

        let client_registration = match configuration.as_ref() {
            ContextServerConfiguration::Http {
                url,
                oauth: Some(oauth_settings),
                ..
            } => {
                // Pre-registered client. Resolve the secret from settings, then keychain.
                let client_secret = if oauth_settings.client_secret.is_some() {
                    oauth_settings.client_secret.clone()
                } else {
                    Self::load_client_secret(&credentials_provider, url, cx)
                        .await
                        .ok()
                        .flatten()
                };
                oauth::OAuthClientRegistration {
                    client_id: oauth_settings.client_id.clone(),
                    client_secret,
                }
            }
            _ => oauth::resolve_client_registration(&http_client, &discovery, &redirect_uri)
                .await
                .context("Failed to resolve OAuth client registration")?,
        };

        let auth_url = oauth::build_authorization_url(
            &discovery.auth_server_metadata,
            &client_registration.client_id,
            &redirect_uri,
            &discovery.scopes,
            &resource,
            &pkce,
            &state_param,
        );

        cx.update(|cx| cx.open_url(auth_url.as_str()));

        let callback = callback_rx
            .await
            .context("OAuth callback server received an invalid request")?;

        if callback.state != state_param {
            anyhow::bail!("OAuth state parameter mismatch (possible CSRF)");
        }

        let tokens = oauth::exchange_code(
            &http_client,
            &discovery.auth_server_metadata,
            &callback.code,
            &client_registration.client_id,
            &redirect_uri,
            &pkce.verifier,
            &resource,
            client_registration.client_secret.as_deref(),
        )
        .await
        .context("Failed to exchange authorization code for tokens")?;

        let session = OAuthSession {
            token_endpoint: discovery.auth_server_metadata.token_endpoint.clone(),
            resource: discovery.resource_metadata.resource.clone(),
            client_registration,
            tokens,
        };

        Self::store_session(&credentials_provider, &server_url, &session, cx)
            .await
            .context("Failed to persist OAuth session in keychain")?;

        let token_provider = Self::create_oauth_token_provider(
            &id,
            &server_url,
            session,
            http_client.clone(),
            credentials_provider,
            cx,
        );

        let new_server = this.update(cx, |this, cx| {
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
                        http_client.clone(),
                        url.to_string(),
                        headers.clone(),
                        cx.background_executor().clone(),
                        Some(token_provider.clone()),
                    );
                    Ok(Arc::new(ContextServer::new_with_timeout(
                        id.clone(),
                        Arc::new(transport),
                        Some(Duration::from_secs(
                            timeout.unwrap_or(global_timeout).min(MAX_TIMEOUT_SECS),
                        )),
                    )))
                }
                _ => anyhow::bail!("OAuth authentication only supported for HTTP servers"),
            }
        })??;

        this.update(cx, |this, cx| {
            this.run_server(new_server, configuration, cx);
        })?;

        Ok(())
    }

    /// Store the full OAuth session in the system keychain, keyed by the
    /// server's canonical URI.
    pub(super) async fn store_session(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        session: &OAuthSession,
        cx: &AsyncApp,
    ) -> Result<()> {
        let key = Self::keychain_key(server_url);
        let json = serde_json::to_string(session)?;
        credentials_provider
            .write_credentials(&key, "mcp-oauth", json.as_bytes(), cx)
            .await
    }

    /// Load the full OAuth session from the system keychain for the given
    /// server URL.
    pub(super) async fn load_session(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        cx: &AsyncApp,
    ) -> Result<Option<OAuthSession>> {
        let key = Self::keychain_key(server_url);
        match credentials_provider.read_credentials(&key, cx).await? {
            Some((_username, password_bytes)) => {
                let session: OAuthSession = serde_json::from_slice(&password_bytes)?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Clear the stored OAuth session from the system keychain.
    pub(super) async fn clear_session(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        cx: &AsyncApp,
    ) -> Result<()> {
        let key = Self::keychain_key(server_url);
        credentials_provider.delete_credentials(&key, cx).await
    }

    pub(super) fn keychain_key(server_url: &url::Url) -> String {
        format!("mcp-oauth:{}", oauth::canonical_server_uri(server_url))
    }

    pub(super) fn client_secret_keychain_key(server_url: &url::Url) -> String {
        format!(
            "mcp-oauth-client-secret:{}",
            oauth::canonical_server_uri(server_url)
        )
    }

    pub(super) async fn load_client_secret(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        cx: &AsyncApp,
    ) -> Result<Option<String>> {
        let key = Self::client_secret_keychain_key(server_url);
        match credentials_provider.read_credentials(&key, cx).await? {
            Some((_username, secret_bytes)) => Ok(Some(String::from_utf8(secret_bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn store_client_secret(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        secret: &str,
        cx: &AsyncApp,
    ) -> Result<()> {
        let key = Self::client_secret_keychain_key(server_url);
        credentials_provider
            .write_credentials(&key, "mcp-oauth-client-secret", secret.as_bytes(), cx)
            .await
    }

    pub(super) async fn clear_client_secret(
        credentials_provider: &Arc<dyn CredentialsProvider>,
        server_url: &url::Url,
        cx: &AsyncApp,
    ) -> Result<()> {
        let key = Self::client_secret_keychain_key(server_url);
        credentials_provider.delete_credentials(&key, cx).await
    }

    /// Log out of an OAuth-authenticated MCP server: clear the stored OAuth
    /// session from the keychain and stop the server.
    pub fn logout_server(&mut self, id: &ContextServerId, cx: &mut Context<Self>) -> Result<()> {
        let state = self.servers.get(id).context("Context server not found")?;
        let configuration = state.configuration();

        let server_url = match configuration.as_ref() {
            ContextServerConfiguration::Http { url, .. } => url.clone(),
            _ => anyhow::bail!("logout only applies to HTTP servers with OAuth"),
        };

        let id = id.clone();
        self.stop_server(&id, cx)?;

        cx.spawn(async move |this, cx| {
            let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
            if let Err(err) = Self::clear_session(&credentials_provider, &server_url, &cx).await {
                log::error!("{} failed to clear OAuth session: {}", id, err);
            }
            // Also clear any client secret so the user gets a fresh prompt on
            // the next authentication attempt.
            Self::clear_client_secret(&credentials_provider, &server_url, &cx)
                .await
                .log_err();
            // Trigger server recreation so the next start uses a fresh
            // transport without the old (now-invalidated) token provider.
            this.update(cx, |this, cx| {
                this.available_context_servers_changed(cx);
            })
            .log_err();
        })
        .detach();

        Ok(())
    }
}
