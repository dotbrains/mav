use super::*;

impl Room {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn room_id(&self) -> impl Future<Output = Option<String>> + 'static {
        let room = self.live_kit.as_ref().map(|lk| lk.room.clone());
        async move {
            let room = room?;
            let sid = room.sid().await;
            let name = room.name();
            Some(format!("{} (sid: {sid})", name))
        }
    }

    pub fn get_stats(&self, cx: &App) -> Task<Option<livekit::SessionStats>> {
        match self.live_kit.as_ref() {
            Some(lk) => {
                let task = lk.room.stats_task(cx);
                cx.background_executor()
                    .spawn(async move { task.await.ok() })
            }
            None => Task::ready(None),
        }
    }

    pub fn input_lag(&self) -> Option<Duration> {
        let us = self
            .live_kit
            .as_ref()?
            .input_lag_us
            .as_ref()?
            .load(std::sync::atomic::Ordering::Relaxed);
        if us > 0 {
            Some(Duration::from_micros(us))
        } else {
            None
        }
    }

    pub fn diagnostics(&self) -> Option<&Entity<CallDiagnostics>> {
        self.diagnostics.as_ref()
    }

    pub fn connection_quality(&self) -> livekit::ConnectionQuality {
        self.live_kit
            .as_ref()
            .map(|lk| lk.room.local_participant().connection_quality())
            .unwrap_or(livekit::ConnectionQuality::Lost)
    }

    pub fn status(&self) -> RoomStatus {
        self.status
    }

    pub fn local_participant(&self) -> &LocalParticipant {
        &self.local_participant
    }

    pub fn local_participant_user(&self, cx: &App) -> Option<Arc<User>> {
        self.user_store.read(cx).current_user()
    }

    pub fn remote_participants(&self) -> &BTreeMap<u64, RemoteParticipant> {
        &self.remote_participants
    }

    pub fn remote_participant_for_peer_id(&self, peer_id: PeerId) -> Option<&RemoteParticipant> {
        self.remote_participants
            .values()
            .find(|p| p.peer_id == peer_id)
    }

    pub fn role_for_user(&self, user_id: u64) -> Option<proto::ChannelRole> {
        self.remote_participants
            .get(&user_id)
            .map(|participant| participant.role)
    }

    pub fn contains_guests(&self) -> bool {
        self.local_participant.role == proto::ChannelRole::Guest
            || self
                .remote_participants
                .values()
                .any(|p| p.role == proto::ChannelRole::Guest)
    }

    pub fn local_participant_is_admin(&self) -> bool {
        self.local_participant.role == proto::ChannelRole::Admin
    }

    pub fn local_participant_is_guest(&self) -> bool {
        self.local_participant.role == proto::ChannelRole::Guest
    }

    pub fn set_participant_role(
        &mut self,
        user_id: u64,
        role: proto::ChannelRole,
        cx: &Context<Self>,
    ) -> Task<Result<()>> {
        let client = self.client.clone();
        let room_id = self.id;
        let role = role.into();
        cx.spawn(async move |_, _| {
            client
                .request(proto::SetRoomParticipantRole {
                    room_id,
                    user_id,
                    role,
                })
                .await
                .map(|_| ())
        })
    }

    pub fn pending_participants(&self) -> &[Arc<User>] {
        &self.pending_participants
    }

    pub fn contains_participant(&self, user_id: u64) -> bool {
        self.participant_user_ids.contains(&user_id)
    }

    pub fn followers_for(&self, leader_id: PeerId, project_id: u64) -> &[PeerId] {
        self.follows_by_leader_id_project_id
            .get(&(leader_id, project_id))
            .map_or(&[], |v| v.as_slice())
    }

    /// Returns the most 'active' projects, defined as most people in the project
    pub fn most_active_project(&self, cx: &App) -> Option<(u64, u64)> {
        let mut project_hosts_and_guest_counts = HashMap::<u64, (Option<u64>, u32)>::default();
        for participant in self.remote_participants.values() {
            match participant.location {
                ParticipantLocation::SharedProject { project_id } => {
                    project_hosts_and_guest_counts
                        .entry(project_id)
                        .or_default()
                        .1 += 1;
                }
                ParticipantLocation::External | ParticipantLocation::UnsharedProject => {}
            }
            for project in &participant.projects {
                project_hosts_and_guest_counts
                    .entry(project.id)
                    .or_default()
                    .0 = Some(participant.user.legacy_id);
            }
        }

        if let Some(user) = self.user_store.read(cx).current_user() {
            for project in &self.local_participant.projects {
                project_hosts_and_guest_counts
                    .entry(project.id)
                    .or_default()
                    .0 = Some(user.legacy_id);
            }
        }

        project_hosts_and_guest_counts
            .into_iter()
            .filter_map(|(id, (host, guest_count))| Some((id, host?, guest_count)))
            .max_by_key(|(_, _, guest_count)| *guest_count)
            .map(|(id, host, _)| (id, host))
    }
}
