use super::*;

impl Room {
    pub fn channel_id(&self) -> Option<ChannelId> {
        self.channel_id
    }

    pub fn is_sharing_project(&self) -> bool {
        !self.shared_projects.is_empty()
    }

    pub fn is_connected(&self, _: &App) -> bool {
        if let Some(live_kit) = self.live_kit.as_ref() {
            live_kit.room.connection_state() == livekit::ConnectionState::Connected
        } else {
            false
        }
    }

    fn new(
        id: u64,
        channel_id: Option<ChannelId>,
        livekit_connection_info: Option<proto::LiveKitConnectionInfo>,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        spawn_room_connection(livekit_connection_info, cx);

        let maintain_connection = cx.spawn({
            let client = client.clone();
            async move |this, cx| {
                Self::maintain_connection(this, client.clone(), cx)
                    .log_err()
                    .await
            }
        });

        Audio::play_sound(Sound::Joined, cx);

        let (room_update_completed_tx, room_update_completed_rx) = watch::channel();

        Self {
            id,
            channel_id,
            live_kit: None,
            diagnostics: None,
            status: RoomStatus::Online,
            shared_projects: Default::default(),
            joined_projects: Default::default(),
            participant_user_ids: Default::default(),
            local_participant: Default::default(),
            remote_participants: Default::default(),
            pending_participants: Default::default(),
            pending_call_count: 0,
            client_subscriptions: vec![
                client.add_message_handler(cx.weak_entity(), Self::handle_room_updated),
            ],
            _subscriptions: vec![
                cx.on_release(Self::released),
                cx.on_app_quit(Self::app_will_quit),
            ],
            leave_when_empty: false,
            pending_room_update: None,
            client,
            user_store,
            follows_by_leader_id_project_id: Default::default(),
            maintain_connection: Some(maintain_connection),
            room_update_completed_tx,
            room_update_completed_rx,
            created: cx.background_executor().now(),
        }
    }

    pub(crate) fn create(
        called_user_id: u64,
        initial_project: Option<Entity<Project>>,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        cx.spawn(async move |cx| {
            let response = client.request(proto::CreateRoom {}).await?;
            let room_proto = response.room.context("invalid room")?;
            let room = cx.new(|cx| {
                let mut room = Self::new(
                    room_proto.id,
                    None,
                    response.live_kit_connection_info,
                    client,
                    user_store,
                    cx,
                );
                if let Some(participant) = room_proto.participants.first() {
                    room.local_participant.role = participant.role()
                }
                room
            });

            let initial_project_id = if let Some(initial_project) = initial_project {
                let initial_project_id = room
                    .update(cx, |room, cx| {
                        room.share_project(initial_project.clone(), cx)
                    })
                    .await?;
                Some(initial_project_id)
            } else {
                None
            };

            let did_join = room
                .update(cx, |room, cx| {
                    room.leave_when_empty = true;
                    room.call(called_user_id, initial_project_id, cx)
                })
                .await;
            match did_join {
                Ok(()) => Ok(room),
                Err(error) => Err(error.context("room creation failed")),
            }
        })
    }
    pub(crate) async fn join_channel(
        channel_id: ChannelId,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        cx: AsyncApp,
    ) -> Result<Entity<Self>> {
        Self::from_join_response(
            client
                .request(proto::JoinChannel {
                    channel_id: channel_id.0,
                })
                .await?,
            client,
            user_store,
            cx,
        )
    }

    pub(crate) async fn join(
        room_id: u64,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        cx: AsyncApp,
    ) -> Result<Entity<Self>> {
        Self::from_join_response(
            client.request(proto::JoinRoom { id: room_id }).await?,
            client,
            user_store,
            cx,
        )
    }

    fn released(&mut self, cx: &mut App) {
        if self.status.is_online() {
            self.leave_internal(cx).detach_and_log_err(cx);
        }
    }

    fn app_will_quit(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        let task = if self.status.is_online() {
            let leave = self.leave_internal(cx);
            Some(cx.background_spawn(async move {
                leave.await.log_err();
            }))
        } else {
            None
        };

        async move {
            if let Some(task) = task {
                task.await;
            }
        }
    }

    pub fn mute_on_join(cx: &App) -> bool {
        CallSettings::get_global(cx).mute_on_join || client::IMPERSONATE_LOGIN.is_some()
    }

    fn from_join_response(
        response: proto::JoinRoomResponse,
        client: Arc<Client>,
        user_store: Entity<UserStore>,
        mut cx: AsyncApp,
    ) -> Result<Entity<Self>> {
        let room_proto = response.room.context("invalid room")?;
        let room = cx.new(|cx| {
            Self::new(
                room_proto.id,
                response.channel_id.map(ChannelId),
                response.live_kit_connection_info,
                client,
                user_store,
                cx,
            )
        });
        room.update(&mut cx, |room, cx| {
            room.leave_when_empty = room.channel_id.is_none();
            room.apply_room_update(room_proto, cx)?;
            anyhow::Ok(())
        })?;
        Ok(room)
    }

    pub(super) fn should_leave(&self) -> bool {
        self.leave_when_empty
            && self.pending_room_update.is_none()
            && self.pending_participants.is_empty()
            && self.remote_participants.is_empty()
            && self.pending_call_count == 0
    }

    pub(crate) fn leave(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        cx.notify();
        self.emit_video_track_unsubscribed_events(cx);
        self.leave_internal(cx)
    }

    fn leave_internal(&mut self, cx: &mut App) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        log::info!("leaving room");
        Audio::play_sound(Sound::Leave, cx);

        self.clear_state(cx);

        let leave_room = self.client.request(proto::LeaveRoom {});
        cx.background_spawn(async move {
            leave_room.await?;
            anyhow::Ok(())
        })
    }
    pub(crate) fn clear_state(&mut self, cx: &mut App) {
        for project in self.shared_projects.drain() {
            if let Some(project) = project.upgrade() {
                project.update(cx, |project, cx| {
                    project.unshare(cx).log_err();
                });
            }
        }
        for project in self.joined_projects.drain() {
            if let Some(project) = project.upgrade() {
                project.update(cx, |project, cx| {
                    project.disconnected_from_host(cx);
                    project.close(cx);
                });
            }
        }

        self.status = RoomStatus::Offline;
        self.remote_participants.clear();
        self.pending_participants.clear();
        self.participant_user_ids.clear();
        self.client_subscriptions.clear();
        self.live_kit.take();
        self.diagnostics.take();
        self.pending_room_update.take();
        self.maintain_connection.take();
    }

    fn emit_video_track_unsubscribed_events(&self, cx: &mut Context<Self>) {
        for participant in self.remote_participants.values() {
            for sid in participant.video_tracks.keys() {
                cx.emit(Event::RemoteVideoTrackUnsubscribed { sid: sid.clone() });
            }
        }
    }

    async fn maintain_connection(
        this: WeakEntity<Self>,
        client: Arc<Client>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let mut client_status = client.status();
        loop {
            let _ = client_status.try_recv();
            let is_connected = client_status.borrow().is_connected();
            // Even if we're initially connected, any future change of the status means we momentarily disconnected.
            if !is_connected || client_status.next().await.is_some() {
                log::info!("detected client disconnection");

                this.upgrade()
                    .context("room was dropped")?
                    .update(cx, |this, cx| {
                        this.status = RoomStatus::Rejoining;
                        cx.notify();
                    });

                // Wait for client to re-establish a connection to the server.
                let executor = cx.background_executor().clone();
                let client_reconnection = async {
                    let mut remaining_attempts = 3;
                    while remaining_attempts > 0 {
                        if client_status.borrow().is_connected() {
                            log::info!("client reconnected, attempting to rejoin room");

                            let Some(this) = this.upgrade() else { break };
                            let task = this.update(cx, |this, cx| this.rejoin(cx));
                            if task.await.log_err().is_some() {
                                return true;
                            } else {
                                remaining_attempts -= 1;
                            }
                        } else if client_status.borrow().is_signed_out() {
                            return false;
                        }

                        log::info!(
                            "waiting for client status change, remaining attempts {}",
                            remaining_attempts
                        );
                        client_status.next().await;
                    }
                    false
                };

                match client_reconnection
                    .with_timeout(RECONNECT_TIMEOUT, &executor)
                    .await
                {
                    Ok(true) => {
                        log::info!("successfully reconnected to room");
                        // If we successfully joined the room, go back around the loop
                        // waiting for future connection status changes.
                        continue;
                    }
                    Ok(false) => break,
                    Err(Timeout) => {
                        log::info!("room reconnection timeout expired");
                        break;
                    }
                }
            }
        }

        // The client failed to re-establish a connection to the server
        // or an error occurred while trying to re-join the room. Either way
        // we leave the room and return an error.
        if let Some(this) = this.upgrade() {
            log::info!("reconnection failed, leaving room");
            this.update(cx, |this, cx| this.leave(cx)).await?;
        }
        anyhow::bail!("can't reconnect to room: client failed to re-establish connection");
    }

    fn rejoin(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let mut projects = HashMap::default();
        let mut reshared_projects = Vec::new();
        let mut rejoined_projects = Vec::new();
        self.shared_projects.retain(|project| {
            if let Some(handle) = project.upgrade() {
                let project = handle.read(cx);
                if let Some(project_id) = project.remote_id() {
                    projects.insert(project_id, handle.clone());
                    reshared_projects.push(proto::UpdateProject {
                        project_id,
                        worktrees: project.worktree_metadata_protos(cx),
                    });
                    return true;
                }
            }
            false
        });
        self.joined_projects.retain(|project| {
            if let Some(handle) = project.upgrade() {
                let project = handle.read(cx);
                if let Some(project_id) = project.remote_id() {
                    projects.insert(project_id, handle.clone());
                    let mut worktrees = Vec::new();
                    let mut repositories = Vec::new();
                    for worktree in project.worktrees(cx) {
                        let worktree = worktree.read(cx);
                        worktrees.push(proto::RejoinWorktree {
                            id: worktree.id().to_proto(),
                            scan_id: worktree.completed_scan_id() as u64,
                        });
                    }
                    for (entry_id, repository) in project.repositories(cx) {
                        let repository = repository.read(cx);
                        repositories.push(proto::RejoinRepository {
                            id: entry_id.to_proto(),
                            scan_id: repository.scan_id,
                        });
                    }

                    rejoined_projects.push(proto::RejoinProject {
                        id: project_id,
                        worktrees,
                        repositories,
                    });
                }
                return true;
            }
            false
        });

        let response = self.client.request_envelope(proto::RejoinRoom {
            id: self.id,
            reshared_projects,
            rejoined_projects,
        });

        cx.spawn(async move |this, cx| {
            let response = response.await?;
            let message_id = response.message_id;
            let response = response.payload;
            let room_proto = response.room.context("invalid room")?;
            this.update(cx, |this, cx| {
                this.status = RoomStatus::Online;
                this.apply_room_update(room_proto, cx)?;

                for reshared_project in response.reshared_projects {
                    if let Some(project) = projects.get(&reshared_project.id) {
                        project.update(cx, |project, cx| {
                            project.reshared(reshared_project, cx).log_err();
                        });
                    }
                }

                for rejoined_project in response.rejoined_projects {
                    if let Some(project) = projects.get(&rejoined_project.id) {
                        project.update(cx, |project, cx| {
                            project.rejoined(rejoined_project, message_id, cx).log_err();
                        });
                    }
                }

                anyhow::Ok(())
            })?
        })
    }
}
