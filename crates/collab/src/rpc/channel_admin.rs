use super::*;

pub(super) async fn delete_channel(
    request: proto::DeleteChannel,
    response: Response<proto::DeleteChannel>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;

    let channel_id = request.channel_id;
    let (root_channel, removed_channels) = db
        .delete_channel(ChannelId::from_proto(channel_id), session.user_id())
        .await?;
    response.send(proto::Ack {})?;

    // Notify members of removed channels
    let mut update = proto::UpdateChannels::default();
    update
        .delete_channels
        .extend(removed_channels.into_iter().map(|id| id.to_proto()));

    let connection_pool = session.connection_pool().await;
    for (connection_id, _) in connection_pool.channel_connection_ids(root_channel) {
        session.peer.send(connection_id, update.clone())?;
    }

    Ok(())
}

/// Invite someone to join a channel.
pub(super) async fn invite_channel_member(
    request: proto::InviteChannelMember,
    response: Response<proto::InviteChannelMember>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let invitee_id = UserId::from_proto(request.user_id);
    let InviteMemberResult {
        channel,
        notifications,
    } = db
        .invite_channel_member(
            channel_id,
            invitee_id,
            session.user_id(),
            request.role().into(),
        )
        .await?;

    let update = proto::UpdateChannels {
        channel_invitations: vec![channel.to_proto()],
        ..Default::default()
    };

    let connection_pool = session.connection_pool().await;
    for connection_id in connection_pool.user_connection_ids(invitee_id) {
        session.peer.send(connection_id, update.clone())?;
    }

    send_notifications(&connection_pool, &session.peer, notifications);

    response.send(proto::Ack {})?;
    Ok(())
}

/// remove someone from a channel
pub(super) async fn remove_channel_member(
    request: proto::RemoveChannelMember,
    response: Response<proto::RemoveChannelMember>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let member_id = UserId::from_proto(request.user_id);

    let RemoveChannelMemberResult {
        membership_update,
        notification_id,
    } = db
        .remove_channel_member(channel_id, member_id, session.user_id())
        .await?;

    let mut connection_pool = session.connection_pool().await;
    notify_membership_updated(
        &mut connection_pool,
        membership_update,
        member_id,
        &session.peer,
    );
    for connection_id in connection_pool.user_connection_ids(member_id) {
        if let Some(notification_id) = notification_id {
            session
                .peer
                .send(
                    connection_id,
                    proto::DeleteNotification {
                        notification_id: notification_id.to_proto(),
                    },
                )
                .trace_err();
        }
    }

    response.send(proto::Ack {})?;
    Ok(())
}

/// Toggle the channel between public and private.
/// Care is taken to maintain the invariant that public channels only descend from public channels,
/// (though members-only channels can appear at any point in the hierarchy).
pub(super) async fn set_channel_visibility(
    request: proto::SetChannelVisibility,
    response: Response<proto::SetChannelVisibility>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let visibility = request.visibility().into();

    let channel_model = db
        .set_channel_visibility(channel_id, visibility, session.user_id())
        .await?;
    let root_id = channel_model.root_id();
    let channel = Channel::from_model(channel_model);

    let mut connection_pool = session.connection_pool().await;
    for (user_id, role) in connection_pool
        .channel_user_ids(root_id)
        .collect::<Vec<_>>()
        .into_iter()
    {
        let update = if role.can_see_channel(channel.visibility) {
            connection_pool.subscribe_to_channel(user_id, channel_id, role);
            proto::UpdateChannels {
                channels: vec![channel.to_proto()],
                ..Default::default()
            }
        } else {
            connection_pool.unsubscribe_from_channel(&user_id, &channel_id);
            proto::UpdateChannels {
                delete_channels: vec![channel.id.to_proto()],
                ..Default::default()
            }
        };

        for connection_id in connection_pool.user_connection_ids(user_id) {
            session.peer.send(connection_id, update.clone())?;
        }
    }

    response.send(proto::Ack {})?;
    Ok(())
}

/// Alter the role for a user in the channel.
pub(super) async fn set_channel_member_role(
    request: proto::SetChannelMemberRole,
    response: Response<proto::SetChannelMemberRole>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let member_id = UserId::from_proto(request.user_id);
    let result = db
        .set_channel_member_role(
            channel_id,
            session.user_id(),
            member_id,
            request.role().into(),
        )
        .await?;

    match result {
        db::SetMemberRoleResult::MembershipUpdated(membership_update) => {
            let mut connection_pool = session.connection_pool().await;
            notify_membership_updated(
                &mut connection_pool,
                membership_update,
                member_id,
                &session.peer,
            )
        }
        db::SetMemberRoleResult::InviteUpdated(channel) => {
            let update = proto::UpdateChannels {
                channel_invitations: vec![channel.to_proto()],
                ..Default::default()
            };

            for connection_id in session
                .connection_pool()
                .await
                .user_connection_ids(member_id)
            {
                session.peer.send(connection_id, update.clone())?;
            }
        }
    }

    response.send(proto::Ack {})?;
    Ok(())
}

/// Change the name of a channel
pub(super) async fn rename_channel(
    request: proto::RenameChannel,
    response: Response<proto::RenameChannel>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let channel_model = db
        .rename_channel(channel_id, session.user_id(), &request.name)
        .await?;
    let root_id = channel_model.root_id();
    let channel = Channel::from_model(channel_model);

    response.send(proto::RenameChannelResponse {
        channel: Some(channel.to_proto()),
    })?;

    let connection_pool = session.connection_pool().await;
    let update = proto::UpdateChannels {
        channels: vec![channel.to_proto()],
        ..Default::default()
    };
    for (connection_id, role) in connection_pool.channel_connection_ids(root_id) {
        if role.can_see_channel(channel.visibility) {
            session.peer.send(connection_id, update.clone())?;
        }
    }

    Ok(())
}

/// Move a channel to a new parent.
pub(super) async fn move_channel(
    request: proto::MoveChannel,
    response: Response<proto::MoveChannel>,
    session: MessageContext,
) -> Result<()> {
    let channel_id = ChannelId::from_proto(request.channel_id);
    let to = ChannelId::from_proto(request.to);

    let (root_id, channels) = session
        .db()
        .await
        .move_channel(channel_id, to, session.user_id())
        .await?;

    let connection_pool = session.connection_pool().await;
    for (connection_id, role) in connection_pool.channel_connection_ids(root_id) {
        let channels = channels
            .iter()
            .filter_map(|channel| {
                if role.can_see_channel(channel.visibility) {
                    Some(channel.to_proto())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if channels.is_empty() {
            continue;
        }

        let update = proto::UpdateChannels {
            channels,
            ..Default::default()
        };

        session.peer.send(connection_id, update.clone())?;
    }

    response.send(Ack {})?;
    Ok(())
}

pub(super) async fn reorder_channel(
    request: proto::ReorderChannel,
    response: Response<proto::ReorderChannel>,
    session: MessageContext,
) -> Result<()> {
    let channel_id = ChannelId::from_proto(request.channel_id);
    let direction = request.direction();

    let updated_channels = session
        .db()
        .await
        .reorder_channel(channel_id, direction, session.user_id())
        .await?;

    if let Some(root_id) = updated_channels.first().map(|channel| channel.root_id()) {
        let connection_pool = session.connection_pool().await;
        for (connection_id, role) in connection_pool.channel_connection_ids(root_id) {
            let channels = updated_channels
                .iter()
                .filter_map(|channel| {
                    if role.can_see_channel(channel.visibility) {
                        Some(channel.to_proto())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if channels.is_empty() {
                continue;
            }

            let update = proto::UpdateChannels {
                channels,
                ..Default::default()
            };

            session.peer.send(connection_id, update.clone())?;
        }
    }

    response.send(Ack {})?;
    Ok(())
}

/// Get the list of channel members
pub(super) async fn get_channel_members(
    request: proto::GetChannelMembers,
    response: Response<proto::GetChannelMembers>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let limit = if request.limit == 0 {
        u16::MAX as u64
    } else {
        request.limit
    };

    let channel = db.get_channel(channel_id, session.user_id()).await?;

    let (members, users) = session
        .app_state
        .user_service
        .search_channel_members(&channel, &request.query, limit as u32)
        .await?;
    let users = users.into_iter().map(proto::User::from).collect();

    response.send(proto::GetChannelMembersResponse { members, users })?;
    Ok(())
}

/// Accept or decline a channel invitation.
pub(super) async fn respond_to_channel_invite(
    request: proto::RespondToChannelInvite,
    response: Response<proto::RespondToChannelInvite>,
    session: MessageContext,
) -> Result<()> {
    let db = session.db().await;
    let channel_id = ChannelId::from_proto(request.channel_id);
    let RespondToChannelInvite {
        membership_update,
        notifications,
    } = db
        .respond_to_channel_invite(channel_id, session.user_id(), request.accept)
        .await?;

    let mut connection_pool = session.connection_pool().await;
    if let Some(membership_update) = membership_update {
        notify_membership_updated(
            &mut connection_pool,
            membership_update,
            session.user_id(),
            &session.peer,
        );
    } else {
        let update = proto::UpdateChannels {
            remove_channel_invitations: vec![channel_id.to_proto()],
            ..Default::default()
        };

        for connection_id in connection_pool.user_connection_ids(session.user_id()) {
            session.peer.send(connection_id, update.clone())?;
        }
    };

    send_notifications(&connection_pool, &session.peer, notifications);

    response.send(proto::Ack {})?;

    Ok(())
}
