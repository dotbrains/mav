use super::*;

pub(super) async fn leave_project(
    request: proto::LeaveProject,
    session: MessageContext,
) -> Result<()> {
    let sender_id = session.connection_id;
    let project_id = ProjectId::from_proto(request.project_id);
    let db = session.db().await;

    let (room, project) = &*db.leave_project(project_id, sender_id).await?;
    tracing::info!(
        %project_id,
        "leave project"
    );

    project_left(project, &session);
    if let Some(room) = room {
        room_updated(room, &session.peer);
    }

    Ok(())
}

/// Updates other participants with changes to the project
pub(super) async fn update_project(
    request: proto::UpdateProject,
    response: Response<proto::UpdateProject>,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(request.project_id);
    let (room, guest_connection_ids) = &*session
        .db()
        .await
        .update_project(project_id, session.connection_id, &request.worktrees)
        .await?;
    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    if let Some(room) = room {
        room_updated(room, &session.peer);
    }
    response.send(proto::Ack {})?;

    Ok(())
}

/// Updates other participants with changes to the worktree
pub(super) async fn update_worktree(
    request: proto::UpdateWorktree,
    response: Response<proto::UpdateWorktree>,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .update_worktree(&request, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    response.send(proto::Ack {})?;
    Ok(())
}

pub(super) async fn update_repository(
    request: proto::UpdateRepository,
    response: Response<proto::UpdateRepository>,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .update_repository(&request, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    response.send(proto::Ack {})?;
    Ok(())
}

pub(super) async fn remove_repository(
    request: proto::RemoveRepository,
    response: Response<proto::RemoveRepository>,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .remove_repository(&request, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    response.send(proto::Ack {})?;
    Ok(())
}

/// Updates other participants with changes to the diagnostics
pub(super) async fn update_diagnostic_summary(
    message: proto::UpdateDiagnosticSummary,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .update_diagnostic_summary(&message, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, message.clone())
        },
    );

    Ok(())
}

/// Updates other participants with changes to the worktree settings
pub(super) async fn update_worktree_settings(
    message: proto::UpdateWorktreeSettings,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .update_worktree_settings(&message, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, message.clone())
        },
    );

    Ok(())
}

/// Notify other participants that a language server has started.
pub(super) async fn start_language_server(
    request: proto::StartLanguageServer,
    session: MessageContext,
) -> Result<()> {
    let guest_connection_ids = session
        .db()
        .await
        .start_language_server(&request, session.connection_id)
        .await?;

    broadcast(
        Some(session.connection_id),
        guest_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    Ok(())
}

/// Notify other participants that a language server has changed.
pub(super) async fn update_language_server(
    request: proto::UpdateLanguageServer,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(request.project_id);
    let db = session.db().await;

    if let Some(proto::update_language_server::Variant::MetadataUpdated(update)) = &request.variant
        && let Some(capabilities) = update.capabilities.clone()
    {
        db.update_server_capabilities(project_id, request.language_server_id, capabilities)
            .await?;
    }

    let project_connection_ids = db
        .project_connection_ids(project_id, session.connection_id, true)
        .await?;
    broadcast(
        Some(session.connection_id),
        project_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    Ok(())
}

/// forward a project request to the host. These requests should be read only
/// as guests are allowed to send them.
pub(super) async fn forward_read_only_project_request<T>(
    request: T,
    response: Response<T>,
    session: MessageContext,
) -> Result<()>
where
    T: EntityMessage + RequestMessage,
{
    let project_id = ProjectId::from_proto(request.remote_entity_id());
    let host_connection_id = session
        .db()
        .await
        .host_for_read_only_project_request(project_id, session.connection_id)
        .await?;
    let payload = session.forward_request(host_connection_id, request).await?;
    response.send(payload)?;
    Ok(())
}

/// forward a project stream request to the host. These requests should be read only
/// as guests are allowed to send them.
pub(super) async fn forward_read_only_project_stream_request<T>(
    request: T,
    response: StreamResponse<T>,
    session: MessageContext,
) -> Result<()>
where
    T: EntityMessage + RequestMessage,
{
    let project_id = ProjectId::from_proto(request.remote_entity_id());
    let host_connection_id = session
        .db()
        .await
        .host_for_read_only_project_request(project_id, session.connection_id)
        .await?;
    let mut stream = session
        .forward_request_stream(host_connection_id, request)
        .await?;
    while let Some(payload) = stream.next().await {
        response.send(payload?)?;
    }
    response.end()?;
    Ok(())
}

/// forward a project request to the host. These requests are disallowed
/// for guests.
pub(super) async fn forward_mutating_project_request<T>(
    request: T,
    response: Response<T>,
    session: MessageContext,
) -> Result<()>
where
    T: EntityMessage + RequestMessage,
{
    let project_id = ProjectId::from_proto(request.remote_entity_id());

    let host_connection_id = session
        .db()
        .await
        .host_for_mutating_project_request(project_id, session.connection_id)
        .await?;
    let payload = session.forward_request(host_connection_id, request).await?;
    response.send(payload)?;
    Ok(())
}

pub(super) async fn disallow_guest_request<T>(
    _request: T,
    response: Response<T>,
    _session: MessageContext,
) -> Result<()>
where
    T: RequestMessage,
{
    response.peer.respond_with_error(
        response.receipt,
        ErrorCode::Forbidden
            .message("request is not allowed for guests".to_string())
            .to_proto(),
    )?;
    response
        .responded
        .store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

pub(super) async fn lsp_query(
    request: proto::LspQuery,
    response: Response<proto::LspQuery>,
    session: MessageContext,
) -> Result<()> {
    let (name, should_write) = request.query_name_and_write_permissions();
    tracing::Span::current().record("lsp_query_request", name);
    tracing::info!("lsp_query message received");
    if should_write {
        forward_mutating_project_request(request, response, session).await
    } else {
        forward_read_only_project_request(request, response, session).await
    }
}

/// Notify other participants that a new buffer has been created
pub(super) async fn create_buffer_for_peer(
    request: proto::CreateBufferForPeer,
    session: MessageContext,
) -> Result<()> {
    session
        .db()
        .await
        .check_user_is_project_host(
            ProjectId::from_proto(request.project_id),
            session.connection_id,
        )
        .await?;
    let peer_id = request.peer_id.context("invalid peer id")?;
    session
        .peer
        .forward_send(session.connection_id, peer_id.into(), request)?;
    Ok(())
}

/// Notify other participants that a new image has been created
pub(super) async fn create_image_for_peer(
    request: proto::CreateImageForPeer,
    session: MessageContext,
) -> Result<()> {
    session
        .db()
        .await
        .check_user_is_project_host(
            ProjectId::from_proto(request.project_id),
            session.connection_id,
        )
        .await?;
    let peer_id = request.peer_id.context("invalid peer id")?;
    session
        .peer
        .forward_send(session.connection_id, peer_id.into(), request)?;
    Ok(())
}

/// Notify other participants that a buffer has been updated. This is
/// allowed for guests as long as the update is limited to selections.
pub(super) async fn update_buffer(
    request: proto::UpdateBuffer,
    response: Response<proto::UpdateBuffer>,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(request.project_id);
    let mut capability = Capability::ReadOnly;

    for op in request.operations.iter() {
        match op.variant {
            None | Some(proto::operation::Variant::UpdateSelections(_)) => {}
            Some(_) => capability = Capability::ReadWrite,
        }
    }

    let host = {
        let guard = session
            .db()
            .await
            .connections_for_buffer_update(project_id, session.connection_id, capability)
            .await?;

        let (host, guests) = &*guard;

        broadcast(
            Some(session.connection_id),
            guests.clone(),
            |connection_id| {
                session
                    .peer
                    .forward_send(session.connection_id, connection_id, request.clone())
            },
        );

        *host
    };

    if host != session.connection_id {
        session.forward_request(host, request.clone()).await?;
    }

    response.send(proto::Ack {})?;
    Ok(())
}

pub(super) async fn forward_project_search_chunk(
    message: proto::FindSearchCandidatesChunk,
    response: Response<proto::FindSearchCandidatesChunk>,
    session: MessageContext,
) -> Result<()> {
    let peer_id = message.peer_id.context("missing peer_id")?;
    let payload = session
        .peer
        .forward_request(session.connection_id, peer_id.into(), message)
        .await?;
    response.send(payload)?;
    Ok(())
}

/// Notify other participants that a project has been updated.
pub(super) async fn broadcast_project_message_from_host<T: EntityMessage<Entity = ShareProject>>(
    request: T,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(request.remote_entity_id());
    let project_connection_ids = session
        .db()
        .await
        .project_connection_ids(project_id, session.connection_id, false)
        .await?;

    broadcast(
        Some(session.connection_id),
        project_connection_ids.iter().copied(),
        |connection_id| {
            session
                .peer
                .forward_send(session.connection_id, connection_id, request.clone())
        },
    );
    Ok(())
}
