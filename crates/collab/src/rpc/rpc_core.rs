use super::*;

pub(super) fn broadcast<F>(
    sender_id: Option<ConnectionId>,
    receiver_ids: impl IntoIterator<Item = ConnectionId>,
    mut f: F,
) where
    F: FnMut(ConnectionId) -> anyhow::Result<()>,
{
    for receiver_id in receiver_ids {
        if Some(receiver_id) != sender_id
            && let Err(error) = f(receiver_id)
        {
            tracing::error!("failed to send to {:?} {}", receiver_id, error);
        }
    }
}

#[instrument(err, skip(executor))]
pub(super) async fn connection_lost(
    session: Session,
    mut teardown: watch::Receiver<bool>,
    executor: Executor,
) -> Result<()> {
    session.peer.disconnect(session.connection_id);
    session
        .connection_pool()
        .await
        .remove_connection(session.connection_id)?;

    session
        .db()
        .await
        .connection_lost(session.connection_id)
        .await
        .trace_err();

    futures::select_biased! {
        _ = executor.sleep(RECONNECT_TIMEOUT).fuse() => {

            log::info!("connection lost, removing all resources for user:{}, connection:{:?}", session.user_id(), session.connection_id);
            leave_room_for_session(&session, session.connection_id).await.trace_err();
            leave_channel_buffers_for_session(&session)
                .await
                .trace_err();

            if !session
                .connection_pool()
                .await
                .is_user_online(session.user_id())
            {
                let db = session.db().await;
                if let Some(room) = db.decline_call(None, session.user_id()).await.trace_err().flatten() {
                    room_updated(&room, &session.peer);
                }
            }

            update_user_contacts(session.user_id(), &session).await?;
        },
        _ = teardown.changed().fuse() => {}
    }

    Ok(())
}
