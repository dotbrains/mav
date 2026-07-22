use super::*;

impl Client {
    pub async fn has_credentials(&self, cx: &AsyncApp) -> bool {
        self.credentials_provider
            .read_credentials(cx)
            .await
            .is_some()
    }

    pub async fn sign_in(
        self: &Arc<Self>,
        try_provider: bool,
        cx: &AsyncApp,
    ) -> Result<Credentials> {
        let is_reauthenticating = if self.status().borrow().is_signed_out() {
            self.set_status(Status::Authenticating, cx);
            false
        } else {
            self.set_status(Status::Reauthenticating, cx);
            true
        };

        let mut credentials = None;

        let old_credentials = self.state.read().credentials.clone();
        if let Some(old_credentials) = old_credentials
            && self.validate_credentials(&old_credentials, cx).await?
        {
            credentials = Some(old_credentials);
        }

        if credentials.is_none()
            && try_provider
            && let Some(stored_credentials) = self.credentials_provider.read_credentials(cx).await
        {
            if self.validate_credentials(&stored_credentials, cx).await? {
                credentials = Some(stored_credentials);
            } else {
                self.credentials_provider
                    .delete_credentials(cx)
                    .await
                    .log_err();
            }
        }

        if credentials.is_none() {
            let mut status_rx = self.status();
            let _ = status_rx.next().await;
            futures::select_biased! {
                authenticate = self.authenticate(cx).fuse() => {
                    match authenticate {
                        Ok(creds) => {
                            if IMPERSONATE_LOGIN.is_none() {
                                self.credentials_provider
                                    .write_credentials(creds.user_id, creds.access_token.clone(), cx)
                                    .await
                                    .log_err();
                            }

                            credentials = Some(creds);
                        },
                        Err(err) => {
                            self.set_status(Status::AuthenticationError, cx);
                            return Err(err);
                        }
                    }
                }
                _ = status_rx.next().fuse() => {
                    return Err(anyhow!("authentication canceled"));
                }
            }
        }

        let credentials = credentials.unwrap();
        self.set_id(credentials.user_id);
        self.cloud_client
            .set_credentials(credentials.user_id as u32, credentials.access_token.clone());
        self.state.write().credentials = Some(credentials.clone());
        self.set_status(
            if is_reauthenticating {
                Status::Reauthenticated
            } else {
                Status::Authenticated
            },
            cx,
        );

        Ok(credentials)
    }

    async fn validate_credentials(
        self: &Arc<Self>,
        credentials: &Credentials,
        cx: &AsyncApp,
    ) -> Result<bool> {
        match self
            .cloud_client
            .validate_credentials(credentials.user_id as u32, &credentials.access_token)
            .await
        {
            Ok(valid) => Ok(valid),
            Err(err) => {
                self.set_status(Status::AuthenticationError, cx);
                Err(err.context("failed to validate credentials"))
            }
        }
    }

    /// Maintains a WebSocket connection with Cloud for receiving updates from the server.
    ///
    /// The connection is re-established with exponential backoff if it drops or fails to
    /// establish.
    fn connect_to_cloud(self: &Arc<Self>, cx: &AsyncApp) {
        let this = self.clone();
        let task = cx.spawn(async move |cx| {
            #[cfg(any(test, feature = "test-support"))]
            let mut rng = StdRng::seed_from_u64(0);
            #[cfg(not(any(test, feature = "test-support")))]
            let mut rng = StdRng::from_os_rng();

            let mut delay = INITIAL_RECONNECTION_DELAY;
            loop {
                match Self::run_cloud_connection(&this, cx).await {
                    Ok(()) => {
                        log::info!("cloud websocket disconnected, will reconnect");
                        delay = INITIAL_RECONNECTION_DELAY;
                    }
                    Err(err) => {
                        log::warn!(
                            "cloud websocket connect failed: {err:#}; retrying in {delay:?}"
                        );
                    }
                }

                let jitter = Duration::from_millis(rng.random_range(0..delay.as_millis() as u64));
                cx.background_executor().timer(delay + jitter).await;
                delay = cmp::min(delay * 2, MAX_RECONNECTION_DELAY);
            }
        });
        self.state.write()._cloud_connection_task = Some(task);
    }

    /// Runs a single attempt of the cloud websocket connection, returning once the connection
    /// closes (cleanly or otherwise) or fails to establish.
    async fn run_cloud_connection(self: &Arc<Self>, cx: &mut AsyncApp) -> Result<()> {
        let connect_task = cx.update({
            let cloud_client = self.cloud_client.clone();
            move |cx| cloud_client.connect(cx)
        })?;
        let connection = connect_task.await?;

        let (mut messages, _cloud_io_task) = cx.update(|cx| connection.spawn(cx));

        {
            let mut state = self.state.write();
            let mut cloud_connection_id = state.cloud_connection_id.0.borrow_mut();
            *cloud_connection_id = cloud_connection_id.saturating_add(1);
        }

        while let Some(message) = messages.next().await {
            if let Some(message) = message.log_err() {
                self.handle_message_to_client(message, cx);
            }
        }

        Ok(())
    }

    /// Performs a sign-in and also (optionally) connects to Collab.
    ///
    /// Only Mav staff automatically connect to Collab.
    pub async fn sign_in_with_optional_connect(
        self: &Arc<Self>,
        try_provider: bool,
        cx: &AsyncApp,
    ) -> Result<()> {
        // Don't try to sign in again if we're already connected to Collab, as it will temporarily disconnect us.
        if self.status().borrow().is_connected() {
            return Ok(());
        }

        let (is_staff_tx, is_staff_rx) = oneshot::channel::<bool>();
        let mut is_staff_tx = Some(is_staff_tx);
        cx.update(|cx| {
            cx.on_flags_ready(move |state, _cx| {
                if let Some(is_staff_tx) = is_staff_tx.take() {
                    is_staff_tx.send(state.is_staff).log_err();
                }
            })
            .detach();
        });

        let credentials = self.sign_in(try_provider, cx).await?;

        self.connect_to_cloud(cx);

        cx.update(move |cx| {
            cx.spawn({
                let client = self.clone();
                async move |cx| {
                    let is_staff = is_staff_rx.await?;
                    if is_staff {
                        match client.connect_with_credentials(credentials, cx).await {
                            ConnectionResult::Timeout => Err(anyhow!("connection timed out")),
                            ConnectionResult::ConnectionReset => Err(anyhow!("connection reset")),
                            ConnectionResult::Result(result) => {
                                result.context("client auth and connect")
                            }
                        }
                    } else {
                        Ok(())
                    }
                }
            })
            .detach_and_log_err(cx);
        });

        Ok(())
    }

    pub async fn connect(
        self: &Arc<Self>,
        try_provider: bool,
        cx: &AsyncApp,
    ) -> ConnectionResult<()> {
        let was_disconnected = match *self.status().borrow() {
            Status::SignedOut | Status::Authenticated => true,
            Status::ConnectionError
            | Status::ConnectionLost
            | Status::Authenticating
            | Status::AuthenticationError
            | Status::Reauthenticating
            | Status::Reauthenticated
            | Status::ReconnectionError { .. } => false,
            Status::Connected { .. } | Status::Connecting | Status::Reconnecting => {
                return ConnectionResult::Result(Ok(()));
            }
            Status::UpgradeRequired => {
                return ConnectionResult::Result(
                    Err(EstablishConnectionError::UpgradeRequired)
                        .context("client auth and connect"),
                );
            }
        };
        let credentials = match self.sign_in(try_provider, cx).await {
            Ok(credentials) => credentials,
            Err(err) => return ConnectionResult::Result(Err(err)),
        };

        if was_disconnected {
            self.set_status(Status::Connecting, cx);
        } else {
            self.set_status(Status::Reconnecting, cx);
        }

        self.connect_with_credentials(credentials, cx).await
    }

    async fn connect_with_credentials(
        self: &Arc<Self>,
        credentials: Credentials,
        cx: &AsyncApp,
    ) -> ConnectionResult<()> {
        let mut timeout =
            futures::FutureExt::fuse(cx.background_executor().timer(CONNECTION_TIMEOUT));
        futures::select_biased! {
            connection = self.establish_connection(&credentials, cx).fuse() => {
                match connection {
                    Ok(conn) => {
                        futures::select_biased! {
                            result = self.set_connection(conn, cx).fuse() => {
                                match result.context("client auth and connect") {
                                    Ok(()) => ConnectionResult::Result(Ok(())),
                                    Err(err) => {
                                        self.set_status(Status::ConnectionError, cx);
                                        ConnectionResult::Result(Err(err))
                                    },
                                }
                            },
                            _ = timeout => {
                                self.set_status(Status::ConnectionError, cx);
                                ConnectionResult::Timeout
                            }
                        }
                    }
                    Err(EstablishConnectionError::Unauthorized) => {
                        self.set_status(Status::ConnectionError, cx);
                        ConnectionResult::Result(Err(EstablishConnectionError::Unauthorized).context("client auth and connect"))
                    }
                    Err(EstablishConnectionError::UpgradeRequired) => {
                        self.set_status(Status::UpgradeRequired, cx);
                        ConnectionResult::Result(Err(EstablishConnectionError::UpgradeRequired).context("client auth and connect"))
                    }
                    Err(error) => {
                        self.set_status(Status::ConnectionError, cx);
                        ConnectionResult::Result(Err(error).context("client auth and connect"))
                    }
                }
            }
            _ = &mut timeout => {
                self.set_status(Status::ConnectionError, cx);
                ConnectionResult::Timeout
            }
        }
    }
}
