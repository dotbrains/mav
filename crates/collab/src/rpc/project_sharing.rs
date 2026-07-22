use super::*;

pub(super) async fn update_participant_location(
    request: proto::UpdateParticipantLocation,
    response: Response<proto::UpdateParticipantLocation>,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(request.room_id);
    let location = request.location.context("invalid location")?;

    let db = session.db().await;
    let room = db
        .update_room_participant_location(room_id, session.connection_id, location)
        .await?;

    room_updated(&room, &session.peer);
    response.send(proto::Ack {})?;
    Ok(())
}

/// Share a project into the room.
pub(super) async fn share_project(
    request: proto::ShareProject,
    response: Response<proto::ShareProject>,
    session: MessageContext,
) -> Result<()> {
    let (project_id, room) = &*session
        .db()
        .await
        .share_project(
            RoomId::from_proto(request.room_id),
            session.connection_id,
            &request.worktrees,
            request.is_ssh_project,
            request.windows_paths.unwrap_or(false),
            &request.features,
        )
        .await?;
    response.send(proto::ShareProjectResponse {
        project_id: project_id.to_proto(),
    })?;
    room_updated(room, &session.peer);

    Ok(())
}

/// Unshare a project from the room.
pub(super) async fn unshare_project(
    message: proto::UnshareProject,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(message.project_id);
    unshare_project_internal(project_id, session.connection_id, &session).await
}

pub(super) async fn unshare_project_internal(
    project_id: ProjectId,
    connection_id: ConnectionId,
    session: &Session,
) -> Result<()> {
    let delete = {
        let room_guard = session
            .db()
            .await
            .unshare_project(project_id, connection_id)
            .await?;

        let (delete, room, guest_connection_ids) = &*room_guard;

        let message = proto::UnshareProject {
            project_id: project_id.to_proto(),
        };

        broadcast(
            Some(connection_id),
            guest_connection_ids.iter().copied(),
            |conn_id| session.peer.send(conn_id, message.clone()),
        );
        if let Some(room) = room {
            room_updated(room, &session.peer);
        }

        *delete
    };

    if delete {
        let db = session.db().await;
        db.delete_project(project_id).await?;
    }

    Ok(())
}

/// Join someone elses shared project.
pub(super) async fn join_project(
    request: proto::JoinProject,
    response: Response<proto::JoinProject>,
    session: MessageContext,
) -> Result<()> {
    let project_id = ProjectId::from_proto(request.project_id);

    tracing::info!(%project_id, "join project");

    let db = session.db().await;
    let project_model = db.get_project(project_id).await?;
    let host_features: Vec<String> =
        serde_json::from_str(&project_model.features).unwrap_or_default();
    let guest_features: HashSet<_> = request.features.iter().collect();
    let host_features_set: HashSet<_> = host_features.iter().collect();
    if guest_features != host_features_set {
        let host_connection_id = project_model.host_connection()?;
        let mut pool = session.connection_pool().await;
        let host_version = pool
            .connection(host_connection_id)
            .map(|c| c.mav_version.to_string());
        let guest_version = pool
            .connection(session.connection_id)
            .map(|c| c.mav_version.to_string());
        drop(pool);
        Err(anyhow!(
            "The host (v{}) and guest (v{}) are using incompatible versions of Mav. The peer with the older version must update to collaborate.",
            host_version.as_deref().unwrap_or("unknown"),
            guest_version.as_deref().unwrap_or("unknown"),
        ))?;
    }

    let (project, replica_id) = &mut *db
        .join_project(
            project_id,
            session.connection_id,
            session.user_id(),
            request.committer_name.clone(),
            request.committer_email.clone(),
        )
        .await?;
    drop(db);

    tracing::info!(%project_id, "join remote project");
    let collaborators = project
        .collaborators
        .iter()
        .filter(|collaborator| collaborator.connection_id != session.connection_id)
        .map(|collaborator| collaborator.to_proto())
        .collect::<Vec<_>>();
    let project_id = project.id;
    let guest_user_id = session.user_id();

    let worktrees = project
        .worktrees
        .iter()
        .map(|(id, worktree)| proto::WorktreeMetadata {
            id: *id,
            root_name: worktree.root_name.clone(),
            visible: worktree.visible,
            abs_path: worktree.abs_path.clone(),
            root_repo_common_dir: None,
        })
        .collect::<Vec<_>>();

    let add_project_collaborator = proto::AddProjectCollaborator {
        project_id: project_id.to_proto(),
        collaborator: Some(proto::Collaborator {
            peer_id: Some(session.connection_id.into()),
            replica_id: replica_id.0 as u32,
            user_id: guest_user_id.to_proto(),
            is_host: false,
            committer_name: request.committer_name.clone(),
            committer_email: request.committer_email.clone(),
        }),
    };

    for collaborator in &collaborators {
        session
            .peer
            .send(
                collaborator.peer_id.unwrap().into(),
                add_project_collaborator.clone(),
            )
            .trace_err();
    }

    // First, we send the metadata associated with each worktree.
    let (language_servers, language_server_capabilities) = project
        .language_servers
        .clone()
        .into_iter()
        .map(|server| (server.server, server.capabilities))
        .unzip();
    response.send(proto::JoinProjectResponse {
        project_id: project.id.0 as u64,
        worktrees,
        replica_id: replica_id.0 as u32,
        collaborators,
        language_servers,
        language_server_capabilities,
        role: project.role.into(),
        windows_paths: project.path_style == PathStyle::Windows,
        features: project.features.clone(),
    })?;

    for (worktree_id, worktree) in mem::take(&mut project.worktrees) {
        // Stream this worktree's entries.
        let message = proto::UpdateWorktree {
            project_id: project_id.to_proto(),
            worktree_id,
            abs_path: worktree.abs_path.clone(),
            root_name: worktree.root_name,
            root_repo_common_dir: worktree.root_repo_common_dir,
            updated_entries: worktree.entries,
            removed_entries: Default::default(),
            scan_id: worktree.scan_id,
            is_last_update: worktree.scan_id == worktree.completed_scan_id,
            updated_repositories: worktree.legacy_repository_entries.into_values().collect(),
            removed_repositories: Default::default(),
        };
        for update in proto::split_worktree_update(message) {
            session.peer.send(session.connection_id, update.clone())?;
        }

        // Stream this worktree's diagnostics.
        let mut worktree_diagnostics = worktree.diagnostic_summaries.into_iter();
        if let Some(summary) = worktree_diagnostics.next() {
            let message = proto::UpdateDiagnosticSummary {
                project_id: project.id.to_proto(),
                worktree_id: worktree.id,
                summary: Some(summary),
                more_summaries: worktree_diagnostics.collect(),
            };
            session.peer.send(session.connection_id, message)?;
        }

        for settings_file in worktree.settings_files {
            session.peer.send(
                session.connection_id,
                proto::UpdateWorktreeSettings {
                    project_id: project_id.to_proto(),
                    worktree_id: worktree.id,
                    path: settings_file.path,
                    content: Some(settings_file.content),
                    kind: Some(settings_file.kind.to_proto() as i32),
                    outside_worktree: Some(settings_file.outside_worktree),
                },
            )?;
        }
    }

    for repository in mem::take(&mut project.repositories) {
        for update in split_repository_update(repository) {
            session.peer.send(session.connection_id, update)?;
        }
    }

    for language_server in &project.language_servers {
        session.peer.send(
            session.connection_id,
            proto::UpdateLanguageServer {
                project_id: project_id.to_proto(),
                server_name: Some(language_server.server.name.clone()),
                language_server_id: language_server.server.id,
                variant: Some(
                    proto::update_language_server::Variant::DiskBasedDiagnosticsUpdated(
                        proto::LspDiskBasedDiagnosticsUpdated {},
                    ),
                ),
            },
        )?;
    }

    Ok(())
}
