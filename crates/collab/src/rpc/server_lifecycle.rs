use super::*;

impl Server {
    pub async fn start(&self) -> Result<()> {
        let server_id = *self.id.lock();
        let app_state = self.app_state.clone();
        let peer = self.peer.clone();
        let timeout = self.app_state.executor.sleep(CLEANUP_TIMEOUT);
        let pool = self.connection_pool.clone();
        let livekit_client = self.app_state.livekit_client.clone();

        let span = info_span!("start server");
        self.app_state.executor.spawn_detached(
            async move {
                tracing::info!("waiting for cleanup timeout");
                timeout.await;
                tracing::info!("cleanup timeout expired, retrieving stale rooms");

                app_state
                    .db
                    .delete_stale_channel_chat_participants(
                        &app_state.config.mav_environment,
                        server_id,
                    )
                    .await
                    .trace_err();

                if let Some((room_ids, channel_ids)) = app_state
                    .db
                    .stale_server_resource_ids(&app_state.config.mav_environment, server_id)
                    .await
                    .trace_err()
                {
                    tracing::info!(stale_room_count = room_ids.len(), "retrieved stale rooms");
                    tracing::info!(
                        stale_channel_buffer_count = channel_ids.len(),
                        "retrieved stale channel buffers"
                    );

                    for channel_id in channel_ids {
                        if let Some(refreshed_channel_buffer) = app_state
                            .db
                            .clear_stale_channel_buffer_collaborators(channel_id, server_id)
                            .await
                            .trace_err()
                        {
                            for connection_id in refreshed_channel_buffer.connection_ids {
                                peer.send(
                                    connection_id,
                                    proto::UpdateChannelBufferCollaborators {
                                        channel_id: channel_id.to_proto(),
                                        collaborators: refreshed_channel_buffer
                                            .collaborators
                                            .clone(),
                                    },
                                )
                                .trace_err();
                            }
                        }
                    }

                    for room_id in room_ids {
                        let mut contacts_to_update = HashSet::default();
                        let mut canceled_calls_to_user_ids = Vec::new();
                        let mut livekit_room = String::new();
                        let mut delete_livekit_room = false;

                        if let Some(mut refreshed_room) = app_state
                            .db
                            .clear_stale_room_participants(room_id, server_id)
                            .await
                            .trace_err()
                        {
                            tracing::info!(
                                room_id = room_id.0,
                                new_participant_count = refreshed_room.room.participants.len(),
                                "refreshed room"
                            );
                            room_updated(&refreshed_room.room, &peer);
                            if let Some(channel) = refreshed_room.channel.as_ref() {
                                channel_updated(channel, &refreshed_room.room, &peer, &pool.lock());
                            }
                            contacts_to_update
                                .extend(refreshed_room.stale_participant_user_ids.iter().copied());
                            contacts_to_update
                                .extend(refreshed_room.canceled_calls_to_user_ids.iter().copied());
                            canceled_calls_to_user_ids =
                                mem::take(&mut refreshed_room.canceled_calls_to_user_ids);
                            livekit_room = mem::take(&mut refreshed_room.room.livekit_room);
                            delete_livekit_room = refreshed_room.room.participants.is_empty();
                        }

                        {
                            let pool = pool.lock();
                            for canceled_user_id in canceled_calls_to_user_ids {
                                for connection_id in pool.user_connection_ids(canceled_user_id) {
                                    peer.send(
                                        connection_id,
                                        proto::CallCanceled {
                                            room_id: room_id.to_proto(),
                                        },
                                    )
                                    .trace_err();
                                }
                            }
                        }

                        for user_id in contacts_to_update {
                            let busy = app_state.db.is_user_busy(user_id).await.trace_err();
                            let contacts = app_state.db.get_contacts(user_id).await.trace_err();
                            if let Some((busy, contacts)) = busy.zip(contacts) {
                                let pool = pool.lock();
                                let updated_contact = contact_for_user(user_id, busy, &pool);
                                for contact in contacts {
                                    if let db::Contact::Accepted {
                                        user_id: contact_user_id,
                                        ..
                                    } = contact
                                    {
                                        for contact_conn_id in
                                            pool.user_connection_ids(contact_user_id)
                                        {
                                            peer.send(
                                                contact_conn_id,
                                                proto::UpdateContacts {
                                                    contacts: vec![updated_contact.clone()],
                                                    remove_contacts: Default::default(),
                                                    incoming_requests: Default::default(),
                                                    remove_incoming_requests: Default::default(),
                                                    outgoing_requests: Default::default(),
                                                    remove_outgoing_requests: Default::default(),
                                                },
                                            )
                                            .trace_err();
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(live_kit) = livekit_client.as_ref()
                            && delete_livekit_room
                        {
                            live_kit.delete_room(livekit_room).await.trace_err();
                        }
                    }
                }

                app_state
                    .db
                    .delete_stale_channel_chat_participants(
                        &app_state.config.mav_environment,
                        server_id,
                    )
                    .await
                    .trace_err();

                app_state
                    .db
                    .clear_old_worktree_entries(server_id)
                    .await
                    .trace_err();

                app_state
                    .db
                    .delete_stale_servers(&app_state.config.mav_environment, server_id)
                    .await
                    .trace_err();
            }
            .instrument(span),
        );
        Ok(())
    }

    pub fn teardown(&self) {
        self.peer.teardown();
        self.connection_pool.lock().reset();
        let _ = self.teardown.send(true);
    }

    #[cfg(feature = "test-support")]
    pub fn reset(&self, id: ServerId) {
        self.teardown();
        *self.id.lock() = id;
        self.peer.reset(id.0 as u32);
        let _ = self.teardown.send(false);
    }

    #[cfg(feature = "test-support")]
    pub fn id(&self) -> ServerId {
        *self.id.lock()
    }

    pub(super) fn add_handler<F, Fut, M>(&mut self, handler: F) -> &mut Self
    where
        F: 'static + Send + Sync + Fn(TypedEnvelope<M>, MessageContext) -> Fut,
        Fut: 'static + Send + Future<Output = Result<()>>,
        M: EnvelopedMessage,
    {
        let prev_handler = self.handlers.insert(
            TypeId::of::<M>(),
            Box::new(move |envelope, session, span| {
                let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                let received_at = envelope.received_at;
                tracing::info!("message received");
                let start_time = Instant::now();
                let future = (handler)(
                    *envelope,
                    MessageContext {
                        session,
                        span: span.clone(),
                    },
                );
                async move {
                    let result = future.await;
                    let total_duration_ms = received_at.elapsed().as_micros() as f64 / 1000.0;
                    let processing_duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
                    let queue_duration_ms = total_duration_ms - processing_duration_ms;
                    span.record(TOTAL_DURATION_MS, total_duration_ms);
                    span.record(PROCESSING_DURATION_MS, processing_duration_ms);
                    span.record(QUEUE_DURATION_MS, queue_duration_ms);
                    match result {
                        Err(error) => {
                            tracing::error!(?error, "error handling message")
                        }
                        Ok(()) => tracing::info!("finished handling message"),
                    }
                }
                .boxed()
            }),
        );
        if prev_handler.is_some() {
            panic!("registered a handler for the same message twice");
        }
        self
    }

    pub(super) fn add_message_handler<F, Fut, M>(&mut self, handler: F) -> &mut Self
    where
        F: 'static + Send + Sync + Fn(M, MessageContext) -> Fut,
        Fut: 'static + Send + Future<Output = Result<()>>,
        M: EnvelopedMessage,
    {
        self.add_handler(move |envelope, session| handler(envelope.payload, session));
        self
    }

    pub(super) fn add_request_handler<F, Fut, M>(&mut self, handler: F) -> &mut Self
    where
        F: 'static + Send + Sync + Fn(M, Response<M>, MessageContext) -> Fut,
        Fut: Send + Future<Output = Result<()>>,
        M: RequestMessage,
    {
        let handler = Arc::new(handler);
        self.add_handler(move |envelope, session| {
            let receipt = envelope.receipt();
            let handler = handler.clone();
            async move {
                let peer = session.peer.clone();
                let responded = Arc::new(AtomicBool::default());
                let response = Response {
                    peer: peer.clone(),
                    responded: responded.clone(),
                    receipt,
                };
                match (handler)(envelope.payload, response, session).await {
                    Ok(()) => {
                        if responded.load(std::sync::atomic::Ordering::SeqCst) {
                            Ok(())
                        } else {
                            let error = anyhow!("handler did not send a response");
                            let proto_err =
                                ErrorCode::Internal.message(format!("{error}")).to_proto();
                            peer.respond_with_error(receipt, proto_err)?;
                            Err(error)?
                        }
                    }
                    Err(error) => {
                        let proto_err = match &error {
                            Error::Internal(err) => err.to_proto(),
                            _ => ErrorCode::Internal.message(format!("{error}")).to_proto(),
                        };
                        peer.respond_with_error(receipt, proto_err)?;
                        Err(error)
                    }
                }
            }
        })
    }

    pub(super) fn add_request_stream_handler<F, Fut, M>(&mut self, handler: F) -> &mut Self
    where
        F: 'static + Send + Sync + Fn(M, StreamResponse<M>, MessageContext) -> Fut,
        Fut: Send + Future<Output = Result<()>>,
        M: RequestMessage,
    {
        let handler = Arc::new(handler);
        self.add_handler(move |envelope, session| {
            let receipt = envelope.receipt();
            let handler = handler.clone();
            async move {
                let peer = session.peer.clone();
                let ended = Arc::new(AtomicBool::default());
                let response = StreamResponse {
                    peer: peer.clone(),
                    ended: ended.clone(),
                    receipt,
                };
                match (handler)(envelope.payload, response, session).await {
                    Ok(()) => {
                        if ended.load(std::sync::atomic::Ordering::SeqCst) {
                            Ok(())
                        } else {
                            let error = anyhow!("handler did not end a response stream");
                            let proto_err =
                                ErrorCode::Internal.message(format!("{error}")).to_proto();
                            peer.respond_with_error(receipt, proto_err)?;
                            Err(error)?
                        }
                    }
                    Err(error) => {
                        let proto_err = match &error {
                            Error::Internal(err) => err.to_proto(),
                            _ => ErrorCode::Internal.message(format!("{error}")).to_proto(),
                        };
                        peer.respond_with_error(receipt, proto_err)?;
                        Err(error)
                    }
                }
            }
        })
    }
}
