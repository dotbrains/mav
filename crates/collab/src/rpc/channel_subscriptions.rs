use super::*;

pub(super) async fn subscribe_to_channels(
    _: proto::SubscribeToChannels,
    session: MessageContext,
) -> Result<()> {
    subscribe_user_to_channels(session.user_id(), &session).await?;
    Ok(())
}

pub(super) async fn subscribe_user_to_channels(
    user_id: UserId,
    session: &Session,
) -> Result<(), Error> {
    let channels_for_user = session.db().await.get_channels_for_user(user_id).await?;
    let mut pool = session.connection_pool().await;
    for membership in &channels_for_user.channel_memberships {
        pool.subscribe_to_channel(user_id, membership.channel_id, membership.role)
    }
    session.peer.send(
        session.connection_id,
        build_update_user_channels(&channels_for_user),
    )?;
    session.peer.send(
        session.connection_id,
        build_channels_update(channels_for_user),
    )?;
    Ok(())
}

/// Creates a new channel.
pub(super) async fn create_channel(
    request: proto::CreateChannel,
    response: Response<proto::CreateChannel>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;

    let parent_id = request.parent_id.map(ChannelId::from_proto);
    let (channel, membership) = db
        .create_channel(&request.name, parent_id, session.user_id())
        .await?;

    let root_id = channel.root_id();
    let channel = Channel::from_model(channel);

    response.send(proto::CreateChannelResponse {
        channel: Some(channel.to_proto()),
        parent_id: request.parent_id,
    })?;

    let mut connection_pool = session.connection_pool().await;
    if let Some(membership) = membership {
        connection_pool.subscribe_to_channel(
            membership.user_id,
            membership.channel_id,
            membership.role,
        );
        let update = proto::UpdateUserChannels {
            channel_memberships: vec![proto::ChannelMembership {
                channel_id: membership.channel_id.to_proto(),
                role: membership.role.into(),
            }],
            ..Default::default()
        };
        for connection_id in connection_pool.user_connection_ids(membership.user_id) {
            session.peer.send(connection_id, update.clone())?;
        }
    }

    for (connection_id, role) in connection_pool.channel_connection_ids(root_id) {
        if !role.can_see_channel(channel.visibility) {
            continue;
        }

        let update = proto::UpdateChannels {
            channels: vec![channel.to_proto()],
            ..Default::default()
        };
        session.peer.send(connection_id, update.clone())?;
    }

    Ok(())
}
