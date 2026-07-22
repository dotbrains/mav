use super::*;

impl Client {
    pub fn new(
        clock: Arc<dyn SystemClock>,
        http: Arc<HttpClientWithUrl>,
        cx: &mut App,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: AtomicU64::new(0),
            peer: Peer::new(0),
            telemetry: Telemetry::new(clock, http.clone(), cx),
            cloud_client: Arc::new(CloudApiClient::new(http.clone())),
            http,
            credentials_provider: ClientCredentialsProvider::new(cx),
            state: Default::default(),
            handler_set: Default::default(),
            message_to_client_handlers: Mutex::new(Vec::new()),
            sign_out_tx: Mutex::new(None),

            #[cfg(any(test, feature = "test-support"))]
            authenticate: Default::default(),
            #[cfg(any(test, feature = "test-support"))]
            establish_connection: Default::default(),
            #[cfg(any(test, feature = "test-support"))]
            rpc_url: RwLock::default(),
        })
    }

    pub fn production(cx: &mut App) -> Arc<Self> {
        let clock = Arc::new(clock::RealSystemClock);
        let http = Arc::new(HttpClientWithUrl::new_url(
            cx.http_client(),
            &ClientSettings::get_global(cx).server_url,
            cx.http_client().proxy().cloned(),
        ));
        Self::new(clock, http, cx)
    }

    pub fn id(&self) -> u64 {
        self.id.load(Ordering::SeqCst)
    }

    pub fn http_client(&self) -> Arc<HttpClientWithUrl> {
        self.http.clone()
    }

    pub fn credentials_provider(&self) -> Arc<dyn CredentialsProvider> {
        self.credentials_provider.provider.clone()
    }

    pub fn cloud_client(&self) -> Arc<CloudApiClient> {
        self.cloud_client.clone()
    }

    pub fn set_id(&self, id: u64) -> &Self {
        self.id.store(id, Ordering::SeqCst);
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn teardown(&self) {
        let mut state = self.state.write();
        state._reconnect_task.take();
        state._cloud_connection_task.take();
        self.handler_set.lock().clear();
        self.peer.teardown();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_authenticate<F>(&self, authenticate: F) -> &Self
    where
        F: 'static + Send + Sync + Fn(&AsyncApp) -> Task<Result<Credentials>>,
    {
        *self.authenticate.write() = Some(Box::new(authenticate));
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_establish_connection<F>(&self, connect: F) -> &Self
    where
        F: 'static
            + Send
            + Sync
            + Fn(&Credentials, &AsyncApp) -> Task<Result<Connection, EstablishConnectionError>>,
    {
        *self.establish_connection.write() = Some(Box::new(connect));
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_rpc_url(&self, url: Url) -> &Self {
        *self.rpc_url.write() = Some(url);
        self
    }

    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalClient>().0.clone()
    }
    pub fn set_global(client: Arc<Client>, cx: &mut App) {
        cx.set_global(GlobalClient(client))
    }

    pub fn user_id(&self) -> Option<u64> {
        self.state
            .read()
            .credentials
            .as_ref()
            .map(|credentials| credentials.user_id)
    }

    pub fn peer_id(&self) -> Option<PeerId> {
        if let Status::Connected { peer_id, .. } = &*self.status().borrow() {
            Some(*peer_id)
        } else {
            None
        }
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.state.read().status.1.clone()
    }

    /// Watches successful cloud websocket reconnections.
    ///
    /// The value is bumped each time the websocket handshake completes. The
    /// initial `0` means no reconnection yet.
    pub fn cloud_connection_id(&self) -> watch::Receiver<u64> {
        self.state.read().cloud_connection_id.1.clone()
    }

    pub(crate) fn set_status(self: &Arc<Self>, status: Status, cx: &AsyncApp) {
        log::info!("set status on client {}: {:?}", self.id(), status);
        let mut state = self.state.write();
        *state.status.0.borrow_mut() = status;

        match status {
            Status::Connected { .. } => {
                state._reconnect_task = None;
            }
            Status::ConnectionLost => {
                let client = self.clone();
                state._reconnect_task = Some(cx.spawn(async move |cx| {
                    #[cfg(any(test, feature = "test-support"))]
                    let mut rng = StdRng::seed_from_u64(0);
                    #[cfg(not(any(test, feature = "test-support")))]
                    let mut rng = StdRng::from_os_rng();

                    let mut delay = INITIAL_RECONNECTION_DELAY;
                    loop {
                        match client.connect(true, cx).await {
                            ConnectionResult::Timeout => {
                                log::error!("client connect attempt timed out")
                            }
                            ConnectionResult::ConnectionReset => {
                                log::error!("client connect attempt reset")
                            }
                            ConnectionResult::Result(r) => {
                                if let Err(error) = r {
                                    log::error!("failed to connect: {error}");
                                } else {
                                    break;
                                }
                            }
                        }

                        if matches!(
                            *client.status().borrow(),
                            Status::AuthenticationError | Status::ConnectionError
                        ) {
                            client.set_status(
                                Status::ReconnectionError {
                                    next_reconnection: Instant::now() + delay,
                                },
                                cx,
                            );
                            let jitter = Duration::from_millis(
                                rng.random_range(0..delay.as_millis() as u64),
                            );
                            cx.background_executor().timer(delay + jitter).await;
                            delay = cmp::min(delay * 2, MAX_RECONNECTION_DELAY);
                        } else {
                            break;
                        }
                    }
                }));
            }
            Status::SignedOut | Status::UpgradeRequired => {
                self.telemetry.set_authenticated_user_info(None, false);
                state._reconnect_task.take();
                state._cloud_connection_task.take();
            }
            _ => {}
        }
    }
}
