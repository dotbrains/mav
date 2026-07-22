use super::*;

pub(super) async fn follow(
    request: proto::Follow,
    response: Response<proto::Follow>,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(request.room_id);
    let project_id = request.project_id.map(ProjectId::from_proto);
    let leader_id = request.leader_id.context("invalid leader id")?.into();
    let follower_id = session.connection_id;

    session
        .db()
        .await
        .check_room_participants(room_id, leader_id, session.connection_id)
        .await?;

    let response_payload = session.forward_request(leader_id, request).await?;
    response.send(response_payload)?;

    if let Some(project_id) = project_id {
        let room = session
            .db()
            .await
            .follow(room_id, project_id, leader_id, follower_id)
            .await?;
        room_updated(&room, &session.peer);
    }

    Ok(())
}

/// Stop following another user in a call.
pub(super) async fn unfollow(request: proto::Unfollow, session: MessageContext) -> Result<()> {
    let room_id = RoomId::from_proto(request.room_id);
    let project_id = request.project_id.map(ProjectId::from_proto);
    let leader_id = request.leader_id.context("invalid leader id")?.into();
    let follower_id = session.connection_id;

    session
        .db()
        .await
        .check_room_participants(room_id, leader_id, session.connection_id)
        .await?;

    session
        .peer
        .forward_send(session.connection_id, leader_id, request)?;

    if let Some(project_id) = project_id {
        let room = session
            .db()
            .await
            .unfollow(room_id, project_id, leader_id, follower_id)
            .await?;
        room_updated(&room, &session.peer);
    }

    Ok(())
}

/// Notify everyone following you of your current location.
pub(super) async fn update_followers(
    request: proto::UpdateFollowers,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(request.room_id);
    let database = session.db.lock().await;

    let connection_ids = if let Some(project_id) = request.project_id {
        let project_id = ProjectId::from_proto(project_id);
        database
            .project_connection_ids(project_id, session.connection_id, true)
            .await?
    } else {
        database
            .room_connection_ids(room_id, session.connection_id)
            .await?
    };

    // For now, don't send view update messages back to that view's current leader.
    let peer_id_to_omit = request.variant.as_ref().and_then(|variant| match variant {
        proto::update_followers::Variant::UpdateView(payload) => payload.leader_id,
        _ => None,
    });

    for connection_id in connection_ids.iter().cloned() {
        if Some(connection_id.into()) != peer_id_to_omit && connection_id != session.connection_id {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())?;
        }
    }
    Ok(())
}

/// Get public data about users.
pub(super) async fn get_users(
    request: proto::GetUsers,
    response: Response<proto::GetUsers>,
    session: MessageContext,
) -> Result<()> {
    let user_ids = request
        .user_ids
        .into_iter()
        .map(UserId::from_proto)
        .collect();
    let users = session
        .app_state
        .user_service
        .get_users_by_ids(user_ids)
        .await?;
    let users = users
        .into_iter()
        .map(|user| proto::User {
            id: user.id.to_proto(),
            username: user.username,
            avatar_url: user.avatar_url,
            github_login: user.github_login,
            name: user.name,
        })
        .collect();
    response.send(proto::UsersResponse { users })?;
    Ok(())
}

/// Search for users (to invite) buy Github login
pub(super) async fn fuzzy_search_users(
    request: proto::FuzzySearchUsers,
    response: Response<proto::FuzzySearchUsers>,
    session: MessageContext,
) -> Result<()> {
    let query = request.query;
    let users = match query.len() {
        0 => vec![],
        1 | 2 => session
            .app_state
            .user_service
            .get_user_by_github_login(&query)
            .await?
            .into_iter()
            .collect(),
        _ => {
            session
                .app_state
                .user_service
                .fuzzy_search_users(&query, 10)
                .await?
        }
    };
    let users = users
        .into_iter()
        .filter(|user| user.id != session.user_id())
        .map(|user| proto::User {
            id: user.id.to_proto(),
            username: user.username,
            avatar_url: user.avatar_url,
            github_login: user.github_login,
            name: user.name,
        })
        .collect();
    response.send(proto::UsersResponse { users })?;
    Ok(())
}

/// Send a contact request to another user.
pub(super) async fn request_contact(
    request: proto::RequestContact,
    response: Response<proto::RequestContact>,
    session: MessageContext,
) -> Result<()> {
    let requester_id = session.user_id();
    let responder_id = UserId::from_proto(request.responder_id);
    if requester_id == responder_id {
        return Err(anyhow!("cannot add yourself as a contact"))?;
    }

    let notifications = session
        .db()
        .await
        .send_contact_request(requester_id, responder_id)
        .await?;

    // Update outgoing contact requests of requester
    let mut update = proto::UpdateContacts::default();
    update.outgoing_requests.push(responder_id.to_proto());
    for connection_id in session
        .connection_pool()
        .await
        .user_connection_ids(requester_id)
    {
        session.peer.send(connection_id, update.clone())?;
    }

    // Update incoming contact requests of responder
    let mut update = proto::UpdateContacts::default();
    update
        .incoming_requests
        .push(proto::IncomingContactRequest {
            requester_id: requester_id.to_proto(),
        });
    let connection_pool = session.connection_pool().await;
    for connection_id in connection_pool.user_connection_ids(responder_id) {
        session.peer.send(connection_id, update.clone())?;
    }

    send_notifications(&connection_pool, &session.peer, notifications);

    response.send(proto::Ack {})?;
    Ok(())
}

/// Accept or decline a contact request
pub(super) async fn respond_to_contact_request(
    request: proto::RespondToContactRequest,
    response: Response<proto::RespondToContactRequest>,
    session: MessageContext,
) -> Result<()> {
    let responder_id = session.user_id();
    let requester_id = UserId::from_proto(request.requester_id);
    let db = session.db().await;
    if request.response == proto::ContactRequestResponse::Dismiss as i32 {
        db.dismiss_contact_notification(responder_id, requester_id)
            .await?;
    } else {
        let accept = request.response == proto::ContactRequestResponse::Accept as i32;

        let notifications = db
            .respond_to_contact_request(responder_id, requester_id, accept)
            .await?;
        let requester_busy = db.is_user_busy(requester_id).await?;
        let responder_busy = db.is_user_busy(responder_id).await?;

        let pool = session.connection_pool().await;
        // Update responder with new contact
        let mut update = proto::UpdateContacts::default();
        if accept {
            update
                .contacts
                .push(contact_for_user(requester_id, requester_busy, &pool));
        }
        update
            .remove_incoming_requests
            .push(requester_id.to_proto());
        for connection_id in pool.user_connection_ids(responder_id) {
            session.peer.send(connection_id, update.clone())?;
        }

        // Update requester with new contact
        let mut update = proto::UpdateContacts::default();
        if accept {
            update
                .contacts
                .push(contact_for_user(responder_id, responder_busy, &pool));
        }
        update
            .remove_outgoing_requests
            .push(responder_id.to_proto());

        for connection_id in pool.user_connection_ids(requester_id) {
            session.peer.send(connection_id, update.clone())?;
        }

        send_notifications(&pool, &session.peer, notifications);
    }

    response.send(proto::Ack {})?;
    Ok(())
}

/// Remove a contact.
pub(super) async fn remove_contact(
    request: proto::RemoveContact,
    response: Response<proto::RemoveContact>,
    session: MessageContext,
) -> Result<()> {
    let requester_id = session.user_id();
    let responder_id = UserId::from_proto(request.user_id);
    let db = session.db().await;
    let (contact_accepted, deleted_notification_id) =
        db.remove_contact(requester_id, responder_id).await?;

    let pool = session.connection_pool().await;
    // Update outgoing contact requests of requester
    let mut update = proto::UpdateContacts::default();
    if contact_accepted {
        update.remove_contacts.push(responder_id.to_proto());
    } else {
        update
            .remove_outgoing_requests
            .push(responder_id.to_proto());
    }
    for connection_id in pool.user_connection_ids(requester_id) {
        session.peer.send(connection_id, update.clone())?;
    }

    // Update incoming contact requests of responder
    let mut update = proto::UpdateContacts::default();
    if contact_accepted {
        update.remove_contacts.push(requester_id.to_proto());
    } else {
        update
            .remove_incoming_requests
            .push(requester_id.to_proto());
    }
    for connection_id in pool.user_connection_ids(responder_id) {
        session.peer.send(connection_id, update.clone())?;
        if let Some(notification_id) = deleted_notification_id {
            session.peer.send(
                connection_id,
                proto::DeleteNotification {
                    notification_id: notification_id.to_proto(),
                },
            )?;
        }
    }

    response.send(proto::Ack {})?;
    Ok(())
}
