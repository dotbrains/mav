use super::*;

pub(super) async fn ping(
    _: proto::Ping,
    response: Response<proto::Ping>,
    _session: MessageContext,
) -> Result<()> {
    response.send(proto::Ack {})?;
    Ok(())
}

/// Creates a new room for calling (outside of channels)
pub(super) async fn create_room(
    _request: proto::CreateRoom,
    response: Response<proto::CreateRoom>,
    session: MessageContext,
) -> Result<()> {
    let livekit_room = nanoid::nanoid!(30);

    let live_kit_connection_info = util::maybe!(async {
        let live_kit = session.app_state.livekit_client.as_ref();
        let live_kit = live_kit?;
        let user_id = session.user_id().to_string();

        let token = live_kit.room_token(&livekit_room, &user_id).trace_err()?;

        Some(proto::LiveKitConnectionInfo {
            server_url: live_kit.url().into(),
            token,
            can_publish: true,
        })
    })
    .await;

    let room = session
        .db()
        .await
        .create_room(session.user_id(), session.connection_id, &livekit_room)
        .await?;

    response.send(proto::CreateRoomResponse {
        room: Some(room.clone()),
        live_kit_connection_info,
    })?;

    update_user_contacts(session.user_id(), &session).await?;
    Ok(())
}

/// Join a room from an invitation. Equivalent to joining a channel if there is one.
pub(super) async fn join_room(
    request: proto::JoinRoom,
    response: Response<proto::JoinRoom>,
    session: MessageContext,
) -> Result<()> {
    let room_id = RoomId::from_proto(request.id);

    let channel_id = session.db().await.channel_id_for_room(room_id).await?;

    if let Some(channel_id) = channel_id {
        return join_channel_internal(channel_id, Box::new(response), session).await;
    }

    let joined_room = {
        let room = session
            .db()
            .await
            .join_room(room_id, session.user_id(), session.connection_id)
            .await?;
        room_updated(&room.room, &session.peer);
        room.into_inner()
    };

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

    let live_kit_connection_info = if let Some(live_kit) = session.app_state.livekit_client.as_ref()
    {
        live_kit
            .room_token(
                &joined_room.room.livekit_room,
                &session.user_id().to_string(),
            )
            .trace_err()
            .map(|token| proto::LiveKitConnectionInfo {
                server_url: live_kit.url().into(),
                token,
                can_publish: true,
            })
    } else {
        None
    };

    response.send(proto::JoinRoomResponse {
        room: Some(joined_room.room),
        channel_id: None,
        live_kit_connection_info,
    })?;

    update_user_contacts(session.user_id(), &session).await?;
    Ok(())
}

/// Rejoin room is used to reconnect to a room after connection errors.
pub(super) async fn rejoin_room(
    request: proto::RejoinRoom,
    response: Response<proto::RejoinRoom>,
    session: MessageContext,
) -> Result<()> {
    let room;
    let channel;
    {
        let mut rejoined_room = session
            .db()
            .await
            .rejoin_room(request, session.user_id(), session.connection_id)
            .await?;

        response.send(proto::RejoinRoomResponse {
            room: Some(rejoined_room.room.clone()),
            reshared_projects: rejoined_room
                .reshared_projects
                .iter()
                .map(|project| proto::ResharedProject {
                    id: project.id.to_proto(),
                    collaborators: project
                        .collaborators
                        .iter()
                        .map(|collaborator| collaborator.to_proto())
                        .collect(),
                })
                .collect(),
            rejoined_projects: rejoined_room
                .rejoined_projects
                .iter()
                .map(|rejoined_project| rejoined_project.to_proto())
                .collect(),
        })?;
        room_updated(&rejoined_room.room, &session.peer);

        for project in &rejoined_room.reshared_projects {
            for collaborator in &project.collaborators {
                session
                    .peer
                    .send(
                        collaborator.connection_id,
                        proto::UpdateProjectCollaborator {
                            project_id: project.id.to_proto(),
                            old_peer_id: Some(project.old_connection_id.into()),
                            new_peer_id: Some(session.connection_id.into()),
                        },
                    )
                    .trace_err();
            }

            broadcast(
                Some(session.connection_id),
                project
                    .collaborators
                    .iter()
                    .map(|collaborator| collaborator.connection_id),
                |connection_id| {
                    session.peer.forward_send(
                        session.connection_id,
                        connection_id,
                        proto::UpdateProject {
                            project_id: project.id.to_proto(),
                            worktrees: project.worktrees.clone(),
                        },
                    )
                },
            );
        }

        notify_rejoined_projects(&mut rejoined_room.rejoined_projects, &session)?;

        let rejoined_room = rejoined_room.into_inner();

        room = rejoined_room.room;
        channel = rejoined_room.channel;
    }

    if let Some(channel) = channel {
        channel_updated(
            &channel,
            &room,
            &session.peer,
            &*session.connection_pool().await,
        );
    }

    update_user_contacts(session.user_id(), &session).await?;
    Ok(())
}

pub(super) fn notify_rejoined_projects(
    rejoined_projects: &mut Vec<RejoinedProject>,
    session: &Session,
) -> Result<()> {
    for project in rejoined_projects.iter() {
        for collaborator in &project.collaborators {
            session
                .peer
                .send(
                    collaborator.connection_id,
                    proto::UpdateProjectCollaborator {
                        project_id: project.id.to_proto(),
                        old_peer_id: Some(project.old_connection_id.into()),
                        new_peer_id: Some(session.connection_id.into()),
                    },
                )
                .trace_err();
        }
    }

    for project in rejoined_projects {
        for worktree in mem::take(&mut project.worktrees) {
            // Stream this worktree's entries.
            let message = proto::UpdateWorktree {
                project_id: project.id.to_proto(),
                worktree_id: worktree.id,
                abs_path: worktree.abs_path.clone(),
                root_name: worktree.root_name,
                root_repo_common_dir: worktree.root_repo_common_dir,
                updated_entries: worktree.updated_entries,
                removed_entries: worktree.removed_entries,
                scan_id: worktree.scan_id,
                is_last_update: worktree.completed_scan_id == worktree.scan_id,
                updated_repositories: worktree.updated_repositories,
                removed_repositories: worktree.removed_repositories,
            };
            for update in proto::split_worktree_update(message) {
                session.peer.send(session.connection_id, update)?;
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
                        project_id: project.id.to_proto(),
                        worktree_id: worktree.id,
                        path: settings_file.path,
                        content: Some(settings_file.content),
                        kind: Some(settings_file.kind.to_proto().into()),
                        outside_worktree: Some(settings_file.outside_worktree),
                    },
                )?;
            }
        }

        for repository in mem::take(&mut project.updated_repositories) {
            for update in split_repository_update(repository) {
                session.peer.send(session.connection_id, update)?;
            }
        }

        for id in mem::take(&mut project.removed_repositories) {
            session.peer.send(
                session.connection_id,
                proto::RemoveRepository {
                    project_id: project.id.to_proto(),
                    id,
                },
            )?;
        }
    }

    Ok(())
}

/// leave room disconnects from the room.
pub(super) async fn leave_room(
    _: proto::LeaveRoom,
    response: Response<proto::LeaveRoom>,
    session: MessageContext,
) -> Result<()> {
    leave_room_for_session(&session, session.connection_id).await?;
    response.send(proto::Ack {})?;
    Ok(())
}

/// Updates the permissions of someone else in the room.
pub(super) async fn set_room_participant_role(
    request: proto::SetRoomParticipantRole,
    response: Response<proto::SetRoomParticipantRole>,
    session: MessageContext,
) -> Result<()> {
    let user_id = UserId::from_proto(request.user_id);
    let role = ChannelRole::from(request.role());

    let (livekit_room, can_publish) = {
        let room = session
            .db()
            .await
            .set_room_participant_role(
                session.user_id(),
                RoomId::from_proto(request.room_id),
                user_id,
                role,
            )
            .await?;

        let livekit_room = room.livekit_room.clone();
        let can_publish = ChannelRole::from(request.role()).can_use_microphone();
        room_updated(&room, &session.peer);
        (livekit_room, can_publish)
    };

    if let Some(live_kit) = session.app_state.livekit_client.as_ref() {
        live_kit
            .update_participant(
                livekit_room.clone(),
                request.user_id.to_string(),
                livekit_api::proto::ParticipantPermission {
                    can_subscribe: true,
                    can_publish,
                    can_publish_data: can_publish,
                    hidden: false,
                    recorder: false,
                },
            )
            .await
            .trace_err();
    }

    response.send(proto::Ack {})?;
    Ok(())
}
