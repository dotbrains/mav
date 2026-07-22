use super::*;

/// Start editing the channel notes
pub(super) async fn join_channel_buffer(
    request: proto::JoinChannelBuffer,
    response: Response<proto::JoinChannelBuffer>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);

    let open_response = db
        .join_channel_buffer(channel_id, session.user_id(), session.connection_id)
        .await?;

    let collaborators = open_response.collaborators.clone();
    response.send(open_response)?;

    let update = UpdateChannelBufferCollaborators {
        channel_id: channel_id.to_proto(),
        collaborators: collaborators.clone(),
    };
    channel_buffer_updated(
        session.connection_id,
        collaborators
            .iter()
            .filter_map(|collaborator| Some(collaborator.peer_id?.into())),
        &update,
        &session.peer,
    );

    Ok(())
}

/// Edit the channel notes
pub(super) async fn update_channel_buffer(
    request: proto::UpdateChannelBuffer,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);

    let (collaborators, epoch, version) = db
        .update_channel_buffer(channel_id, session.user_id(), &request.operations)
        .await?;

    channel_buffer_updated(
        session.connection_id,
        collaborators.clone(),
        &proto::UpdateChannelBuffer {
            channel_id: channel_id.to_proto(),
            operations: request.operations,
        },
        &session.peer,
    );

    let pool = &*session.connection_pool().await;

    let non_collaborators =
        pool.channel_connection_ids(channel_id)
            .filter_map(|(connection_id, _)| {
                if collaborators.contains(&connection_id) {
                    None
                } else {
                    Some(connection_id)
                }
            });

    broadcast(None, non_collaborators, |peer_id| {
        session.peer.send(
            peer_id,
            proto::UpdateChannels {
                latest_channel_buffer_versions: vec![proto::ChannelBufferVersion {
                    channel_id: channel_id.to_proto(),
                    epoch: epoch as u64,
                    version: version.clone(),
                }],
                ..Default::default()
            },
        )
    });

    Ok(())
}

/// Rejoin the channel notes after a connection blip
pub(super) async fn rejoin_channel_buffers(
    request: proto::RejoinChannelBuffers,
    response: Response<proto::RejoinChannelBuffers>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let buffers = db
        .rejoin_channel_buffers(&request.buffers, session.user_id(), session.connection_id)
        .await?;

    for rejoined_buffer in &buffers {
        let collaborators_to_notify = rejoined_buffer
            .buffer
            .collaborators
            .iter()
            .filter_map(|c| Some(c.peer_id?.into()));
        channel_buffer_updated(
            session.connection_id,
            collaborators_to_notify,
            &proto::UpdateChannelBufferCollaborators {
                channel_id: rejoined_buffer.buffer.channel_id,
                collaborators: rejoined_buffer.buffer.collaborators.clone(),
            },
            &session.peer,
        );
    }

    response.send(proto::RejoinChannelBuffersResponse {
        buffers: buffers.into_iter().map(|b| b.buffer).collect(),
    })?;

    Ok(())
}

/// Stop editing the channel notes
pub(super) async fn leave_channel_buffer(
    request: proto::LeaveChannelBuffer,
    response: Response<proto::LeaveChannelBuffer>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);

    let left_buffer = db
        .leave_channel_buffer(channel_id, session.connection_id)
        .await?;

    response.send(Ack {})?;

    channel_buffer_updated(
        session.connection_id,
        left_buffer.connections,
        &proto::UpdateChannelBufferCollaborators {
            channel_id: channel_id.to_proto(),
            collaborators: left_buffer.collaborators,
        },
        &session.peer,
    );

    Ok(())
}

pub(super) fn channel_buffer_updated<T: EnvelopedMessage>(
    sender_id: ConnectionId,
    collaborators: impl IntoIterator<Item = ConnectionId>,
    message: &T,
    peer: &Peer,
) {
    broadcast(Some(sender_id), collaborators, |peer_id| {
        peer.send(peer_id, message.clone())
    });
}

pub(super) fn send_notifications(
    connection_pool: &ConnectionPool,
    peer: &Peer,
    notifications: db::NotificationBatch,
) {
    for (user_id, notification) in notifications {
        for connection_id in connection_pool.user_connection_ids(user_id) {
            if let Err(error) = peer.send(
                connection_id,
                proto::AddNotification {
                    notification: Some(notification.clone()),
                },
            ) {
                tracing::error!(
                    "failed to send notification to {:?} {}",
                    connection_id,
                    error
                );
            }
        }
    }
}
