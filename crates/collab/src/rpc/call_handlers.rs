use super::*;

pub(super) async fn call(
    request: proto::Call,
    response: Response<proto::Call>,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(request.room_id);
    let calling_user_id = session.user_id();
    let calling_connection_id = session.connection_id;
    let called_user_id = UserId::from_proto(request.called_user_id);
    let initial_project_id = request.initial_project_id.map(ProjectId::from_proto);
    if !session
        .db()
        .await
        .has_contact(calling_user_id, called_user_id)
        .await?
    {
        return Err(anyhow!("cannot call a user who isn't a contact"))?;
    }

    let incoming_call = {
        let (room, incoming_call) = &mut *session
            .db()
            .await
            .call(
                room_id,
                calling_user_id,
                calling_connection_id,
                called_user_id,
                initial_project_id,
            )
            .await?;
        room_updated(room, &session.peer);
        mem::take(incoming_call)
    };
    update_user_contacts(called_user_id, &session).await?;

    let mut calls = session
        .connection_pool()
        .await
        .user_connection_ids(called_user_id)
        .map(|connection_id| session.peer.request(connection_id, incoming_call.clone()))
        .collect::<FuturesUnordered<_>>();

    while let Some(call_response) = calls.next().await {
        match call_response.as_ref() {
            Ok(_) => {
                response.send(proto::Ack {})?;
                return Ok(());
            }
            Err(_) => {
                call_response.trace_err();
            }
        }
    }

    {
        let room = session
            .db()
            .await
            .call_failed(room_id, called_user_id)
            .await?;
        room_updated(&room, &session.peer);
    }
    update_user_contacts(called_user_id, &session).await?;

    Err(anyhow!("failed to ring user"))?
}

/// Cancel an outgoing call.
pub(super) async fn cancel_call(
    request: proto::CancelCall,
    response: Response<proto::CancelCall>,
    session: MessageContext,
) -> Result<()> {
    let called_user_id = UserId::from_proto(request.called_user_id);
    let room_id = RoomId::from_proto(request.room_id);
    {
        let room = session
            .db()
            .await
            .cancel_call(room_id, session.connection_id, called_user_id)
            .await?;
        room_updated(&room, &session.peer);
    }

    for connection_id in session
        .connection_pool()
        .await
        .user_connection_ids(called_user_id)
    {
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
    response.send(proto::Ack {})?;

    update_user_contacts(called_user_id, &session).await?;
    Ok(())
}

/// Decline an incoming call.
pub(super) async fn decline_call(
    message: proto::DeclineCall,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(message.room_id);
    {
        let room = session
            .db()
            .await
            .decline_call(Some(room_id), session.user_id())
            .await?
            .context("declining call")?;
        room_updated(&room, &session.peer);
    }

    for connection_id in session
        .connection_pool()
        .await
        .user_connection_ids(session.user_id())
    {
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
    update_user_contacts(session.user_id(), &session).await?;
    Ok(())
}
