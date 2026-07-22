use super::*;

pub(super) async fn join_channel(
    request: proto::JoinChannel,
    response: Response<proto::JoinChannel>,
    session: MessageContext,
) -> Result<()> {
    let channel_id = ChannelId::from_proto(request.channel_id);
    join_channel_internal(channel_id, Box::new(response), session).await
}

pub(super) trait JoinChannelInternalResponse {
    fn send(self, result: proto::JoinRoomResponse) -> Result<()>;
}
impl JoinChannelInternalResponse for Response<proto::JoinChannel> {
    fn send(self, result: proto::JoinRoomResponse) -> Result<()> {
        Response::<proto::JoinChannel>::send(self, result)
    }
}
impl JoinChannelInternalResponse for Response<proto::JoinRoom> {
    fn send(self, result: proto::JoinRoomResponse) -> Result<()> {
        Response::<proto::JoinRoom>::send(self, result)
    }
}

pub(super) async fn join_channel_internal(
    channel_id: ChannelId,
    response: Box<impl JoinChannelInternalResponse>,
    session: MessageContext,
) -> Result<()> {
    let joined_room = {
        let mut db = session.db().await;
        // If mav quits without leaving the room, and the user re-opens mav before the
        // RECONNECT_TIMEOUT, we need to make sure that we kick the user out of the previous
        // room they were in.
        if let Some(connection) = db.stale_room_connection(session.user_id()).await? {
            tracing::info!(
                stale_connection_id = %connection,
                "cleaning up stale connection",
            );
            drop(db);
            leave_room_for_session(&session, connection).await?;
            db = session.db().await;
        }

        let (joined_room, membership_updated, role) = db
            .join_channel(channel_id, session.user_id(), session.connection_id)
            .await?;

        let live_kit_connection_info =
            session
                .app_state
                .livekit_client
                .as_ref()
                .and_then(|live_kit| {
                    let (can_publish, token) = if role == ChannelRole::Guest {
                        (
                            false,
                            live_kit
                                .guest_token(
                                    &joined_room.room.livekit_room,
                                    &session.user_id().to_string(),
                                )
                                .trace_err()?,
                        )
                    } else {
                        (
                            true,
                            live_kit
                                .room_token(
                                    &joined_room.room.livekit_room,
                                    &session.user_id().to_string(),
                                )
                                .trace_err()?,
                        )
                    };

                    Some(LiveKitConnectionInfo {
                        server_url: live_kit.url().into(),
                        token,
                        can_publish,
                    })
                });

        response.send(proto::JoinRoomResponse {
            room: Some(joined_room.room.clone()),
            channel_id: joined_room
                .channel
                .as_ref()
                .map(|channel| channel.id.to_proto()),
            live_kit_connection_info,
        })?;

        let mut connection_pool = session.connection_pool().await;
        if let Some(membership_updated) = membership_updated {
            notify_membership_updated(
                &mut connection_pool,
                membership_updated,
                session.user_id(),
                &session.peer,
            );
        }

        room_updated(&joined_room.room, &session.peer);

        joined_room
    };

    channel_updated(
        &joined_room.channel.context("channel not returned")?,
        &joined_room.room,
        &session.peer,
        &*session.connection_pool().await,
    );

    update_user_contacts(session.user_id(), &session).await?;
    Ok(())
}

/// Send a message to the channel
pub(super) async fn send_channel_message(
    _request: proto::SendChannelMessage,
    _response: Response<proto::SendChannelMessage>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Delete a channel message
pub(super) async fn remove_channel_message(
    _request: proto::RemoveChannelMessage,
    _response: Response<proto::RemoveChannelMessage>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

pub(super) async fn update_channel_message(
    _request: proto::UpdateChannelMessage,
    _response: Response<proto::UpdateChannelMessage>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Mark a channel message as read
pub(super) async fn acknowledge_channel_message(
    _request: proto::AckChannelMessage,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Mark a buffer version as synced
pub(super) async fn acknowledge_buffer_version(
    request: proto::AckBufferOperation,
    session: MessageContext,
) -> Result<()> {
    let buffer_id = BufferId::from_proto(request.buffer_id);
    session
        .db()
        .await
        .observe_buffer_version(
            buffer_id,
            session.user_id(),
            request.epoch as i32,
            &request.version,
        )
        .await?;
    Ok(())
}

/// Start receiving chat updates for a channel
pub(super) async fn join_channel_chat(
    _request: proto::JoinChannelChat,
    _response: Response<proto::JoinChannelChat>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Stop receiving chat updates for a channel
pub(super) async fn leave_channel_chat(
    _request: proto::LeaveChannelChat,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Retrieve the chat history for a channel
pub(super) async fn get_channel_messages(
    _request: proto::GetChannelMessages,
    _response: Response<proto::GetChannelMessages>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Retrieve specific chat messages
pub(super) async fn get_channel_messages_by_id(
    _request: proto::GetChannelMessagesById,
    _response: Response<proto::GetChannelMessagesById>,
    _session: MessageContext,
) -> Result<()> {
    Err(anyhow!("chat has been removed in the latest version of Mav").into())
}

/// Retrieve the current users notifications
pub(super) async fn get_notifications(
    request: proto::GetNotifications,
    response: Response<proto::GetNotifications>,
    session: MessageContext,
) -> Result<()> {
    let notifications = session
        .db()
        .await
        .get_notifications(
            session.user_id(),
            NOTIFICATION_COUNT_PER_PAGE,
            request.before_id.map(db::NotificationId::from_proto),
        )
        .await?;
    response.send(proto::GetNotificationsResponse {
        done: notifications.len() < NOTIFICATION_COUNT_PER_PAGE,
        notifications,
    })?;
    Ok(())
}

/// Mark notifications as read
pub(super) async fn mark_notification_as_read(
    request: proto::MarkNotificationRead,
    response: Response<proto::MarkNotificationRead>,
    session: MessageContext,
) -> Result<()> {
    let database = &session.db().await;
    let notifications = database
        .mark_notification_as_read_by_id(
            session.user_id(),
            NotificationId::from_proto(request.notification_id),
        )
        .await?;
    send_notifications(
        &*session.connection_pool().await,
        &session.peer,
        notifications,
    );
    response.send(proto::Ack {})?;
    Ok(())
}
