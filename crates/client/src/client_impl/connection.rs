use super::*;

impl Client {
    pub(crate) async fn set_connection(
        self: &Arc<Self>,
        conn: Connection,
        cx: &AsyncApp,
    ) -> Result<()> {
        let executor = cx.background_executor();
        log::debug!("add connection to peer");
        let (connection_id, handle_io, mut incoming) = self.peer.add_connection(conn, {
            let executor = executor.clone();
            move |duration| executor.timer(duration)
        });
        let handle_io = executor.spawn(handle_io);

        let peer_id = async {
            log::debug!("waiting for server hello");
            let message = incoming.next().await.context("no hello message received")?;
            log::debug!("got server hello");
            let hello_message_type_name = message.payload_type_name().to_string();
            let hello = message
                .into_any()
                .downcast::<TypedEnvelope<proto::Hello>>()
                .map_err(|_| {
                    anyhow!(
                        "invalid hello message received: {:?}",
                        hello_message_type_name
                    )
                })?;
            let peer_id = hello.payload.peer_id.context("invalid peer id")?;
            Ok(peer_id)
        };

        let peer_id = match peer_id.await {
            Ok(peer_id) => peer_id,
            Err(error) => {
                self.peer.disconnect(connection_id);
                return Err(error);
            }
        };

        log::debug!(
            "set status to connected (connection id: {:?}, peer id: {:?})",
            connection_id,
            peer_id
        );
        self.set_status(
            Status::Connected {
                peer_id,
                connection_id,
            },
            cx,
        );

        cx.spawn({
            let this = self.clone();
            async move |cx| {
                while let Some(message) = incoming.next().await {
                    this.handle_message(message, cx);
                    // Don't starve the main thread when receiving lots of messages at once.
                    smol::future::yield_now().await;
                }
            }
        })
        .detach();

        cx.spawn({
            let this = self.clone();
            async move |cx| match handle_io.await {
                Ok(()) => {
                    if *this.status().borrow()
                        == (Status::Connected {
                            connection_id,
                            peer_id,
                        })
                    {
                        this.set_status(Status::SignedOut, cx);
                    }
                }
                Err(err) => {
                    log::error!("connection error: {:?}", err);
                    this.set_status(Status::ConnectionLost, cx);
                }
            }
        })
        .detach();

        Ok(())
    }

    pub(crate) fn authenticate(self: &Arc<Self>, cx: &AsyncApp) -> Task<Result<Credentials>> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(callback) = self.authenticate.read().as_ref() {
            return callback(cx);
        }

        self.authenticate_with_browser(cx)
    }

    pub(crate) fn establish_connection(
        self: &Arc<Self>,
        credentials: &Credentials,
        cx: &AsyncApp,
    ) -> Task<Result<Connection, EstablishConnectionError>> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(callback) = self.establish_connection.read().as_ref() {
            return callback(credentials, cx);
        }

        self.establish_websocket_connection(credentials, cx)
    }

    fn rpc_url(
        &self,
        http: Arc<HttpClientWithUrl>,
        release_channel: Option<ReleaseChannel>,
    ) -> impl Future<Output = Result<url::Url>> + use<> {
        #[cfg(any(test, feature = "test-support"))]
        let url_override = self.rpc_url.read().clone();

        async move {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(url) = url_override {
                return Ok(url);
            }

            if let Some(url) = &*MAV_RPC_URL {
                return Url::parse(url).context("invalid rpc url");
            }

            let mut url = http.build_url("/rpc");
            if let Some(preview_param) =
                release_channel.and_then(|channel| channel.release_query_param())
            {
                url += "?";
                url += preview_param;
            }

            let response = http.get(&url, Default::default(), false).await?;
            anyhow::ensure!(
                response.status().is_redirection(),
                "unexpected /rpc response status {}",
                response.status()
            );
            let collab_url = response
                .headers()
                .get("Location")
                .context("missing location header in /rpc response")?
                .to_str()
                .map_err(EstablishConnectionError::other)?
                .to_string();
            Url::parse(&collab_url).with_context(|| format!("parsing collab rpc url {collab_url}"))
        }
    }

    fn establish_websocket_connection(
        self: &Arc<Self>,
        credentials: &Credentials,
        cx: &AsyncApp,
    ) -> Task<Result<Connection, EstablishConnectionError>> {
        let release_channel = cx.update(|cx| ReleaseChannel::try_global(cx));
        let app_version = cx.update(|cx| AppVersion::global(cx).to_string());

        let http = self.http.clone();
        let proxy = http.proxy().cloned();
        let user_agent = http.user_agent().cloned();
        let credentials = credentials.clone();
        let rpc_url = self.rpc_url(http, release_channel);
        let system_id = self.telemetry.system_id();
        let metrics_id = self.telemetry.metrics_id();
        cx.spawn(async move |cx| {
            use HttpOrHttps::*;

            #[derive(Debug)]
            enum HttpOrHttps {
                Http,
                Https,
            }

            let mut rpc_url = rpc_url.await?;
            let url_scheme = match rpc_url.scheme() {
                "https" => Https,
                "http" => Http,
                _ => Err(anyhow!("invalid rpc url: {}", rpc_url))?,
            };

            let stream = gpui_tokio::Tokio::spawn_result(cx, {
                let rpc_url = rpc_url.clone();
                async move {
                    let rpc_host = rpc_url
                        .host_str()
                        .zip(rpc_url.port_or_known_default())
                        .context("missing host in rpc url")?;
                    Ok(match proxy {
                        Some(proxy) => connect_proxy_stream(&proxy, rpc_host).await?,
                        None => Box::new(TcpStream::connect(rpc_host).await?),
                    })
                }
            })
            .await?;

            log::info!("connected to rpc endpoint {}", rpc_url);

            rpc_url
                .set_scheme(match url_scheme {
                    Https => "wss",
                    Http => "ws",
                })
                .unwrap();

            // We call `into_client_request` to let `tungstenite` construct the WebSocket request
            // for us from the RPC URL.
            //
            // Among other things, it will generate and set a `Sec-WebSocket-Key` header for us.
            let mut request = IntoClientRequest::into_client_request(rpc_url.as_str())?;

            // We then modify the request to add our desired headers.
            let request_headers = request.headers_mut();
            request_headers.insert(
                http::header::AUTHORIZATION,
                HeaderValue::from_str(&credentials.authorization_header())?,
            );
            request_headers.insert(
                "x-mav-protocol-version",
                HeaderValue::from_str(&rpc::PROTOCOL_VERSION.to_string())?,
            );
            request_headers.insert("x-mav-app-version", HeaderValue::from_str(&app_version)?);
            request_headers.insert(
                "x-mav-release-channel",
                HeaderValue::from_str(release_channel.map(|r| r.dev_name()).unwrap_or("unknown"))?,
            );
            if let Some(user_agent) = user_agent {
                request_headers.insert(http::header::USER_AGENT, user_agent);
            }
            if let Some(system_id) = system_id {
                request_headers.insert("x-mav-system-id", HeaderValue::from_str(&system_id)?);
            }
            if let Some(metrics_id) = metrics_id {
                request_headers.insert("x-mav-metrics-id", HeaderValue::from_str(&metrics_id)?);
            }

            let (stream, _) = async_tungstenite::tokio::client_async_tls_with_connector_and_config(
                request,
                stream,
                Some(Arc::new(http_client_tls::tls_config()).into()),
                None,
            )
            .await?;

            Ok(Connection::new(
                stream
                    .map_err(|error| anyhow!(error))
                    .sink_map_err(|error| anyhow!(error)),
            ))
        })
    }
}
