use super::*;

pub(super) fn to_axum_message(message: TungsteniteMessage) -> anyhow::Result<AxumMessage> {
    let message = match message {
        TungsteniteMessage::Text(payload) => AxumMessage::Text(payload.as_str().to_string()),
        TungsteniteMessage::Binary(payload) => AxumMessage::Binary(payload.into()),
        TungsteniteMessage::Ping(payload) => AxumMessage::Ping(payload.into()),
        TungsteniteMessage::Pong(payload) => AxumMessage::Pong(payload.into()),
        TungsteniteMessage::Close(frame) => AxumMessage::Close(frame.map(|frame| AxumCloseFrame {
            code: frame.code.into(),
            reason: frame.reason.as_str().to_owned().into(),
        })),
        // We should never receive a frame while reading the message, according
        // to the `tungstenite` maintainers:
        //
        // > It cannot occur when you read messages from the WebSocket, but it
        // > can be used when you want to send the raw frames (e.g. you want to
        // > send the frames to the WebSocket without composing the full message first).
        // >
        // > — https://github.com/snapview/tungstenite-rs/issues/268
        TungsteniteMessage::Frame(_) => {
            bail!("received an unexpected frame while reading the message")
        }
    };

    Ok(message)
}

pub(super) fn to_tungstenite_message(message: AxumMessage) -> TungsteniteMessage {
    match message {
        AxumMessage::Text(payload) => TungsteniteMessage::Text(payload.into()),
        AxumMessage::Binary(payload) => TungsteniteMessage::Binary(payload.into()),
        AxumMessage::Ping(payload) => TungsteniteMessage::Ping(payload.into()),
        AxumMessage::Pong(payload) => TungsteniteMessage::Pong(payload.into()),
        AxumMessage::Close(frame) => {
            TungsteniteMessage::Close(frame.map(|frame| TungsteniteCloseFrame {
                code: frame.code.into(),
                reason: frame.reason.as_ref().into(),
            }))
        }
    }
}

pub(super) fn notify_membership_updated(
    connection_pool: &mut ConnectionPool,
    result: MembershipUpdated,
    user_id: UserId,
    peer: &Peer,
) {
    for membership in &result.new_channels.channel_memberships {
        connection_pool.subscribe_to_channel(user_id, membership.channel_id, membership.role)
    }
    for channel_id in &result.removed_channels {
        connection_pool.unsubscribe_from_channel(&user_id, channel_id)
    }

    let user_channels_update = proto::UpdateUserChannels {
        channel_memberships: result
            .new_channels
            .channel_memberships
            .iter()
            .map(|cm| proto::ChannelMembership {
                channel_id: cm.channel_id.to_proto(),
                role: cm.role.into(),
            })
            .collect(),
        ..Default::default()
    };

    let mut update = build_channels_update(result.new_channels);
    update.delete_channels = result
        .removed_channels
        .into_iter()
        .map(|id| id.to_proto())
        .collect();
    update.remove_channel_invitations = vec![result.channel_id.to_proto()];

    for connection_id in connection_pool.user_connection_ids(user_id) {
        peer.send(connection_id, user_channels_update.clone())
            .trace_err();
        peer.send(connection_id, update.clone()).trace_err();
    }
}

pub(super) fn build_update_user_channels(channels: &ChannelsForUser) -> proto::UpdateUserChannels {
    proto::UpdateUserChannels {
        channel_memberships: channels
            .channel_memberships
            .iter()
            .map(|m| proto::ChannelMembership {
                channel_id: m.channel_id.to_proto(),
                role: m.role.into(),
            })
            .collect(),
        observed_channel_buffer_version: channels.observed_buffer_versions.clone(),
    }
}

pub(super) fn build_channels_update(channels: ChannelsForUser) -> proto::UpdateChannels {
    let mut update = proto::UpdateChannels::default();

    for channel in channels.channels {
        update.channels.push(channel.to_proto());
    }

    update.latest_channel_buffer_versions = channels.latest_buffer_versions;

    for (channel_id, participants) in channels.channel_participants {
        update
            .channel_participants
            .push(proto::ChannelParticipants {
                channel_id: channel_id.to_proto(),
                participant_user_ids: participants.into_iter().map(|id| id.to_proto()).collect(),
            });
    }

    for channel in channels.invited_channels {
        update.channel_invitations.push(channel.to_proto());
    }

    update
}

pub(super) fn build_initial_contacts_update(
    contacts: Vec<db::Contact>,
    pool: &ConnectionPool,
) -> proto::UpdateContacts {
    let mut update = proto::UpdateContacts::default();

    for contact in contacts {
        match contact {
            db::Contact::Accepted { user_id, busy } => {
                update.contacts.push(contact_for_user(user_id, busy, pool));
            }
            db::Contact::Outgoing { user_id } => update.outgoing_requests.push(user_id.to_proto()),
            db::Contact::Incoming { user_id } => {
                update
                    .incoming_requests
                    .push(proto::IncomingContactRequest {
                        requester_id: user_id.to_proto(),
                    })
            }
        }
    }

    update
}

pub(super) fn contact_for_user(
    user_id: UserId,
    busy: bool,
    pool: &ConnectionPool,
) -> proto::Contact {
    proto::Contact {
        user_id: user_id.to_proto(),
        online: pool.is_user_online(user_id),
        busy,
    }
}

pub(super) fn room_updated(room: &proto::Room, peer: &Peer) {
    broadcast(
        None,
        room.participants
            .iter()
            .filter_map(|participant| Some(participant.peer_id?.into())),
        |peer_id| {
            peer.send(
                peer_id,
                proto::RoomUpdated {
                    room: Some(room.clone()),
                },
            )
        },
    );
}

pub(super) fn channel_updated(
    channel: &db::channel::Model,
    room: &proto::Room,
    peer: &Peer,
    pool: &ConnectionPool,
) {
    let participants = room
        .participants
        .iter()
        .map(|p| p.user_id)
        .collect::<Vec<_>>();

    broadcast(
        None,
        pool.channel_connection_ids(channel.root_id())
            .filter_map(|(channel_id, role)| {
                role.can_see_channel(channel.visibility)
                    .then_some(channel_id)
            }),
        |peer_id| {
            peer.send(
                peer_id,
                proto::UpdateChannels {
                    channel_participants: vec![proto::ChannelParticipants {
                        channel_id: channel.id.to_proto(),
                        participant_user_ids: participants.clone(),
                    }],
                    ..Default::default()
                },
            )
        },
    );
}

pub(super) async fn update_user_contacts(user_id: UserId, session: &Session) -> Result<()> {
    let db = session.db().await;

    let contacts = db.get_contacts(user_id).await?;
    let busy = db.is_user_busy(user_id).await?;

    let pool = session.connection_pool().await;
    let updated_contact = contact_for_user(user_id, busy, &pool);
    for contact in contacts {
        if let db::Contact::Accepted {
            user_id: contact_user_id,
            ..
        } = contact
        {
            for contact_conn_id in pool.user_connection_ids(contact_user_id) {
                session
                    .peer
                    .send(
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
    Ok(())
}

pub(super) async fn leave_room_for_session(
    session: &Session,
    connection_id: ConnectionId,
) -> Result<()> {
    let mut contacts_to_update = HashSet::default();

    let room_id;
    let canceled_calls_to_user_ids;
    let livekit_room;
    let delete_livekit_room;
    let room;
    let channel;

    if let Some(mut left_room) = session.db().await.leave_room(connection_id).await? {
        contacts_to_update.insert(session.user_id());

        for project in left_room.left_projects.values() {
            project_left(project, session);
        }

        room_id = RoomId::from_proto(left_room.room.id);
        canceled_calls_to_user_ids = mem::take(&mut left_room.canceled_calls_to_user_ids);
        livekit_room = mem::take(&mut left_room.room.livekit_room);
        delete_livekit_room = left_room.deleted;
        room = mem::take(&mut left_room.room);
        channel = mem::take(&mut left_room.channel);

        room_updated(&room, &session.peer);
    } else {
        return Ok(());
    }

    if let Some(channel) = channel {
        channel_updated(
            &channel,
            &room,
            &session.peer,
            &*session.connection_pool().await,
        );
    }

    {
        let pool = session.connection_pool().await;
        for canceled_user_id in canceled_calls_to_user_ids {
            for connection_id in pool.user_connection_ids(canceled_user_id) {
                session
                    .peer
                    .send(
                        connection_id,
                        proto::CallCanceled {
                            room_id: room_id.to_proto(),
                        },
                    )
                    .trace_err();
            }
            contacts_to_update.insert(canceled_user_id);
        }
    }

    for contact_user_id in contacts_to_update {
        update_user_contacts(contact_user_id, session).await?;
    }

    if let Some(live_kit) = session.app_state.livekit_client.as_ref() {
        live_kit
            .remove_participant(livekit_room.clone(), session.user_id().to_string())
            .await
            .trace_err();

        if delete_livekit_room {
            live_kit.delete_room(livekit_room).await.trace_err();
        }
    }

    Ok(())
}

pub(super) async fn leave_channel_buffers_for_session(session: &Session) -> Result<()> {
    let left_channel_buffers = session
        .db()
        .await
        .leave_channel_buffers(session.connection_id)
        .await?;

    for left_buffer in left_channel_buffers {
        channel_buffer_updated(
            session.connection_id,
            left_buffer.connections,
            &proto::UpdateChannelBufferCollaborators {
                channel_id: left_buffer.channel_id.to_proto(),
                collaborators: left_buffer.collaborators,
            },
            &session.peer,
        );
    }

    Ok(())
}

pub(super) fn project_left(project: &db::LeftProject, session: &Session) {
    for connection_id in &project.connection_ids {
        if project.should_unshare {
            session
                .peer
                .send(
                    *connection_id,
                    proto::UnshareProject {
                        project_id: project.id.to_proto(),
                    },
                )
                .trace_err();
        } else {
            session
                .peer
                .send(
                    *connection_id,
                    proto::RemoveProjectCollaborator {
                        project_id: project.id.to_proto(),
                        peer_id: Some(session.connection_id.into()),
                    },
                )
                .trace_err();
        }
    }
}
