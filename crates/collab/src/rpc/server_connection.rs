use super::*;

impl Server {
    pub fn handle_connection(
        self: &Arc<Self>,
        connection: Connection,
        address: String,
        principal: Principal,
        mav_version: MavVersion,
        release_channel: Option<String>,
        user_agent: Option<String>,
        geoip_country_code: Option<String>,
        system_id: Option<String>,
        send_connection_id: Option<oneshot::Sender<ConnectionId>>,
        executor: Executor,
        connection_guard: Option<ConnectionGuard>,
    ) -> impl Future<Output = ()> + use<> {
        let this = self.clone();
        let span = info_span!("handle connection", %address,
            connection_id=field::Empty,
            user_id=field::Empty,
            login=field::Empty,
            user_agent=field::Empty,
            geoip_country_code=field::Empty,
            release_channel=field::Empty,
        );
        principal.update_span(&span);
        if let Some(user_agent) = user_agent {
            span.record("user_agent", user_agent);
        }
        if let Some(release_channel) = release_channel {
            span.record("release_channel", release_channel);
        }

        if let Some(country_code) = geoip_country_code.as_ref() {
            span.record("geoip_country_code", country_code);
        }

        let mut teardown = self.teardown.subscribe();
        async move {
            if *teardown.borrow() {
                tracing::error!("server is tearing down");
                return;
            }

            let (connection_id, handle_io, mut incoming_rx) =
                this.peer.add_connection(connection, {
                    let executor = executor.clone();
                    move |duration| executor.sleep(duration)
                });
            tracing::Span::current().record("connection_id", format!("{}", connection_id));

            tracing::info!("connection opened");

            let session = Session {
                principal: principal.clone(),
                connection_id,
                db: Arc::new(tokio::sync::Mutex::new(DbHandle(this.app_state.db.clone()))),
                peer: this.peer.clone(),
                connection_pool: this.connection_pool.clone(),
                app_state: this.app_state.clone(),
                geoip_country_code,
                system_id,
                _executor: executor.clone(),
            };

            if let Err(error) = this
                .send_initial_client_update(
                    connection_id,
                    mav_version,
                    send_connection_id,
                    &session,
                )
                .await
            {
                tracing::error!(?error, "failed to send initial client update");
                return;
            }
            drop(connection_guard);

            let handle_io = handle_io.fuse();
            futures::pin_mut!(handle_io);

            // Handlers for foreground messages are pushed into the following `FuturesUnordered`.
            // This prevents deadlocks when e.g., client A performs a request to client B and
            // client B performs a request to client A. If both clients stop processing further
            // messages until their respective request completes, they won't have a chance to
            // respond to the other client's request and cause a deadlock.
            //
            // This arrangement ensures we will attempt to process earlier messages first, but fall
            // back to processing messages arrived later in the spirit of making progress.
            const MAX_CONCURRENT_HANDLERS: usize = 256;
            let mut foreground_message_handlers = FuturesUnordered::new();
            let concurrent_handlers = Arc::new(Semaphore::new(MAX_CONCURRENT_HANDLERS));
            let get_concurrent_handlers = {
                let concurrent_handlers = concurrent_handlers.clone();
                move || MAX_CONCURRENT_HANDLERS - concurrent_handlers.available_permits()
            };
            loop {
                let next_message = async {
                    let permit = concurrent_handlers.clone().acquire_owned().await.unwrap();
                    let message = incoming_rx.next().await;
                    // Cache the concurrent_handlers here, so that we know what the
                    // queue looks like as each handler starts
                    (permit, message, get_concurrent_handlers())
                }
                .fuse();
                futures::pin_mut!(next_message);
                futures::select_biased! {
                    _ = teardown.changed().fuse() => return,
                    result = handle_io => {
                        if let Err(error) = result {
                            tracing::error!(?error, "error handling I/O");
                        }
                        break;
                    }
                    _ = foreground_message_handlers.next() => {}
                    next_message = next_message => {
                        let (permit, message, concurrent_handlers) = next_message;
                        if let Some(message) = message {
                            let type_name = message.payload_type_name();
                            // note: we copy all the fields from the parent span so we can query them in the logs.
                            // (https://github.com/tokio-rs/tracing/issues/2670).
                            let span = tracing::info_span!("receive message",
                                %connection_id,
                                %address,
                                type_name,
                                concurrent_handlers,
                                user_id=field::Empty,
                                login=field::Empty,
                                lsp_query_request=field::Empty,
                                release_channel=field::Empty,
                                { TOTAL_DURATION_MS }=field::Empty,
                                { PROCESSING_DURATION_MS }=field::Empty,
                                { QUEUE_DURATION_MS }=field::Empty,
                                { HOST_WAITING_MS }=field::Empty
                            );
                            principal.update_span(&span);
                            let span_enter = span.enter();
                            if let Some(handler) = this.handlers.get(&message.payload_type_id()) {
                                let is_background = message.is_background();
                                let handle_message = (handler)(message, session.clone(), span.clone());
                                drop(span_enter);

                                let handle_message = async move {
                                    handle_message.await;
                                    drop(permit);
                                }.instrument(span);
                                if is_background {
                                    executor.spawn_detached(handle_message);
                                } else {
                                    foreground_message_handlers.push(handle_message);
                                }
                            } else {
                                tracing::error!("no message handler");
                            }
                        } else {
                            tracing::info!("connection closed");
                            break;
                        }
                    }
                }
            }

            drop(foreground_message_handlers);
            let concurrent_handlers = get_concurrent_handlers();
            tracing::info!(concurrent_handlers, "signing out");
            if let Err(error) = connection_lost(session, teardown, executor).await {
                tracing::error!(?error, "error signing out");
            }
        }
        .instrument(span)
    }

    pub(super) async fn send_initial_client_update(
        &self,
        connection_id: ConnectionId,
        mav_version: MavVersion,
        mut send_connection_id: Option<oneshot::Sender<ConnectionId>>,
        session: &Session,
    ) -> Result<()> {
        self.peer.send(
            connection_id,
            proto::Hello {
                peer_id: Some(connection_id.into()),
            },
        )?;
        tracing::info!("sent hello message");
        if let Some(send_connection_id) = send_connection_id.take() {
            let _ = send_connection_id.send(connection_id);
        }

        match &session.principal {
            Principal::User(user) => {
                if !user.connected_once {
                    self.peer.send(connection_id, proto::ShowContacts {})?;
                    self.app_state
                        .db
                        .set_user_connected_once(user.id, true)
                        .await?;
                }

                let contacts = self.app_state.db.get_contacts(user.id).await?;

                {
                    let mut pool = self.connection_pool.lock();
                    pool.add_connection(connection_id, user.id, user.admin, mav_version.clone());
                    self.peer.send(
                        connection_id,
                        build_initial_contacts_update(contacts, &pool),
                    )?;
                }

                if let Some(incoming_call) =
                    self.app_state.db.incoming_call_for_user(user.id).await?
                {
                    self.peer.send(connection_id, incoming_call)?;
                }

                update_user_contacts(user.id, session).await?;
            }
        }

        Ok(())
    }
}
