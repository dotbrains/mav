use super::*;

impl Room {
    pub(super) async fn handle_room_updated(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RoomUpdated>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let room = envelope.payload.room.context("invalid room")?;
        this.update(&mut cx, |this, cx| this.apply_room_update(room, cx))
    }

    pub(super) fn apply_room_update(
        &mut self,
        room: proto::Room,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        log::trace!(
            "client {:?}. room update: {:?}",
            self.client.user_id(),
            &room
        );

        self.pending_room_update = Some(self.start_room_connection(room, cx));

        cx.notify();
        Ok(())
    }

    pub fn room_update_completed(&mut self) -> impl Future<Output = ()> + use<> {
        let mut done_rx = self.room_update_completed_rx.clone();
        async move {
            while let Some(result) = done_rx.next().await {
                if result.is_some() {
                    break;
                }
            }
        }
    }

    fn start_room_connection(&self, mut room: proto::Room, cx: &mut Context<Self>) -> Task<()> {
        // Filter ourselves out from the room's participants.
        let local_participant_ix = room
            .participants
            .iter()
            .position(|participant| Some(participant.user_id) == self.client.user_id());
        let local_participant = local_participant_ix.map(|ix| room.participants.swap_remove(ix));

        let pending_participant_user_ids = room
            .pending_participants
            .iter()
            .map(|p| p.user_id)
            .collect::<Vec<_>>();

        let remote_participant_user_ids = room
            .participants
            .iter()
            .map(|p| p.user_id)
            .collect::<Vec<_>>();

        let (remote_participants, pending_participants) =
            self.user_store.update(cx, move |user_store, cx| {
                (
                    user_store.get_users(remote_participant_user_ids, cx),
                    user_store.get_users(pending_participant_user_ids, cx),
                )
            });
        cx.spawn(async move |this, cx| {
            let (remote_participants, pending_participants) =
                futures::join!(remote_participants, pending_participants);

            this.update(cx, |this, cx| {
                this.participant_user_ids.clear();

                if let Some(participant) = local_participant {
                    let role = participant.role();
                    this.local_participant.projects = participant.projects;
                    if this.local_participant.role != role {
                        this.local_participant.role = role;

                        if role == proto::ChannelRole::Guest {
                            for project in mem::take(&mut this.shared_projects) {
                                if let Some(project) = project.upgrade() {
                                    this.unshare_project(project, cx).log_err();
                                }
                            }
                            this.local_participant.projects.clear();
                            if let Some(livekit_room) = &mut this.live_kit {
                                livekit_room.stop_publishing(cx);
                            }
                        }

                        this.joined_projects.retain(|project| {
                            if let Some(project) = project.upgrade() {
                                project.update(cx, |project, cx| project.set_role(role, cx));
                                true
                            } else {
                                false
                            }
                        });
                    }
                } else {
                    this.local_participant.projects.clear();
                }

                let livekit_participants = this
                    .live_kit
                    .as_ref()
                    .map(|live_kit| live_kit.room.remote_participants());

                if let Some(participants) = remote_participants.log_err() {
                    for (participant, user) in room.participants.into_iter().zip(participants) {
                        let Some(peer_id) = participant.peer_id else {
                            continue;
                        };
                        let participant_index = ParticipantIndex(participant.participant_index);
                        this.participant_user_ids.insert(participant.user_id);

                        let old_projects = this
                            .remote_participants
                            .get(&participant.user_id)
                            .into_iter()
                            .flat_map(|existing| &existing.projects)
                            .map(|project| project.id)
                            .collect::<HashSet<_>>();
                        let new_projects = participant
                            .projects
                            .iter()
                            .map(|project| project.id)
                            .collect::<HashSet<_>>();

                        for project in &participant.projects {
                            if !old_projects.contains(&project.id) {
                                cx.emit(Event::RemoteProjectShared {
                                    owner: user.clone(),
                                    project_id: project.id,
                                    worktree_root_names: project.worktree_root_names.clone(),
                                });
                            }
                        }

                        for unshared_project_id in old_projects.difference(&new_projects) {
                            this.joined_projects.retain(|project| {
                                if let Some(project) = project.upgrade() {
                                    project.update(cx, |project, cx| {
                                        if project.remote_id() == Some(*unshared_project_id) {
                                            project.disconnected_from_host(cx);
                                            false
                                        } else {
                                            true
                                        }
                                    })
                                } else {
                                    false
                                }
                            });
                            cx.emit(Event::RemoteProjectUnshared {
                                project_id: *unshared_project_id,
                            });
                        }

                        let role = participant.role();
                        let location = ParticipantLocation::from_proto(participant.location)
                            .unwrap_or(ParticipantLocation::External);
                        if let Some(remote_participant) =
                            this.remote_participants.get_mut(&participant.user_id)
                        {
                            remote_participant.peer_id = peer_id;
                            remote_participant.projects = participant.projects;
                            remote_participant.participant_index = participant_index;
                            if location != remote_participant.location
                                || role != remote_participant.role
                            {
                                remote_participant.location = location;
                                remote_participant.role = role;
                                cx.emit(Event::ParticipantLocationChanged {
                                    participant_id: peer_id,
                                });
                            }
                        } else {
                            this.remote_participants.insert(
                                participant.user_id,
                                RemoteParticipant {
                                    user: user.clone(),
                                    participant_index,
                                    peer_id,
                                    projects: participant.projects,
                                    location,
                                    role,
                                    muted: true,
                                    speaking: false,
                                    video_tracks: Default::default(),
                                    audio_tracks: Default::default(),
                                },
                            );

                            // When joining a room start_room_connection gets
                            // called but we have already played the join sound.
                            // Dont play extra sounds over that.
                            if this.created.elapsed() > Duration::from_millis(100) {
                                if let proto::ChannelRole::Guest = role {
                                    Audio::play_sound(Sound::GuestJoined, cx);
                                // Do not play join sound in large meetings
                                } else if this.remote_participants().len() < 10 {
                                    Audio::play_sound(Sound::Joined, cx);
                                }
                            }

                            if let Some(livekit_participants) = &livekit_participants
                                && let Some(livekit_participant) = livekit_participants
                                    .get(&ParticipantIdentity(user.legacy_id.to_string()))
                            {
                                for publication in
                                    livekit_participant.track_publications().into_values()
                                {
                                    if let Some(track) = publication.track() {
                                        this.livekit_room_updated(
                                            RoomEvent::TrackSubscribed {
                                                track,
                                                publication,
                                                participant: livekit_participant.clone(),
                                            },
                                            cx,
                                        )
                                        .warn_on_err();
                                    }
                                }
                            }
                        }
                    }

                    this.remote_participants.retain(|user_id, participant| {
                        if this.participant_user_ids.contains(user_id) {
                            true
                        } else {
                            for project in &participant.projects {
                                cx.emit(Event::RemoteProjectUnshared {
                                    project_id: project.id,
                                });
                            }
                            for sid in participant.video_tracks.keys() {
                                cx.emit(Event::RemoteVideoTrackUnsubscribed { sid: sid.clone() });
                            }
                            if !participant.video_tracks.is_empty() {
                                cx.emit(Event::RemoteVideoTracksChanged {
                                    participant_id: participant.peer_id,
                                });
                            }
                            false
                        }
                    });
                }

                if let Some(pending_participants) = pending_participants.log_err() {
                    this.pending_participants = pending_participants;
                    for participant in &this.pending_participants {
                        this.participant_user_ids.insert(participant.legacy_id);
                    }
                }

                this.follows_by_leader_id_project_id.clear();
                for follower in room.followers {
                    let project_id = follower.project_id;
                    let (leader, follower) = match (follower.leader_id, follower.follower_id) {
                        (Some(leader), Some(follower)) => (leader, follower),

                        _ => {
                            log::error!("Follower message {follower:?} missing some state");
                            continue;
                        }
                    };

                    let list = this
                        .follows_by_leader_id_project_id
                        .entry((leader, project_id))
                        .or_default();
                    if !list.contains(&follower) {
                        list.push(follower);
                    }
                }

                this.pending_room_update.take();
                if this.should_leave() {
                    log::info!("room is empty, leaving");
                    this.leave(cx).detach();
                }

                this.user_store.update(cx, |user_store, cx| {
                    let participant_indices_by_user_id = this
                        .remote_participants
                        .iter()
                        .map(|(user_id, participant)| (*user_id, participant.participant_index))
                        .collect();
                    user_store.set_participant_indices(participant_indices_by_user_id, cx);
                });

                this.check_invariants();
                this.room_update_completed_tx.try_send(Some(())).ok();
                cx.notify();
            })
            .ok();
        })
    }

    pub(super) fn livekit_room_updated(
        &mut self,
        event: RoomEvent,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        log::trace!(
            "client {:?}. livekit event: {:?}",
            self.client.user_id(),
            &event
        );

        match event {
            RoomEvent::TrackSubscribed {
                track,
                participant,
                publication,
            } => {
                let user_id = participant.identity().0.parse()?;
                let track_id = track.sid();
                let participant =
                    self.remote_participants
                        .get_mut(&user_id)
                        .with_context(|| {
                            format!(
                                "{:?} subscribed to track by unknown participant {user_id}",
                                self.client.user_id()
                            )
                        })?;
                if self.live_kit.as_ref().is_none_or(|kit| kit.deafened) && publication.is_audio() {
                    publication.set_enabled(false, cx);
                }
                match track {
                    livekit_client::RemoteTrack::Audio(track) => {
                        cx.emit(Event::RemoteAudioTracksChanged {
                            participant_id: participant.peer_id,
                        });
                        if let Some(live_kit) = self.live_kit.as_ref() {
                            let stream = live_kit.room.play_remote_audio_track(&track, cx)?;
                            participant.audio_tracks.insert(track_id, (track, stream));
                            participant.muted = publication.is_muted();
                        }
                    }
                    livekit_client::RemoteTrack::Video(track) => {
                        cx.emit(Event::RemoteVideoTracksChanged {
                            participant_id: participant.peer_id,
                        });
                        participant.video_tracks.insert(track_id, track);
                    }
                }
            }

            RoomEvent::TrackUnsubscribed {
                track, participant, ..
            } => {
                let user_id = participant.identity().0.parse()?;
                let participant =
                    self.remote_participants
                        .get_mut(&user_id)
                        .with_context(|| {
                            format!(
                                "{:?}, unsubscribed from track by unknown participant {user_id}",
                                self.client.user_id()
                            )
                        })?;
                match track {
                    livekit_client::RemoteTrack::Audio(track) => {
                        participant.audio_tracks.remove(&track.sid());
                        participant.muted = true;
                        cx.emit(Event::RemoteAudioTracksChanged {
                            participant_id: participant.peer_id,
                        });
                    }
                    livekit_client::RemoteTrack::Video(track) => {
                        participant.video_tracks.remove(&track.sid());
                        cx.emit(Event::RemoteVideoTracksChanged {
                            participant_id: participant.peer_id,
                        });
                        cx.emit(Event::RemoteVideoTrackUnsubscribed { sid: track.sid() });
                    }
                }
            }

            RoomEvent::ActiveSpeakersChanged { speakers } => {
                let mut speaker_ids = speakers
                    .into_iter()
                    .filter_map(|speaker| speaker.identity().0.parse().ok())
                    .collect::<Vec<u64>>();
                speaker_ids.sort_unstable();
                for (sid, participant) in &mut self.remote_participants {
                    participant.speaking = speaker_ids.binary_search(sid).is_ok();
                }
                if let Some(id) = self.client.user_id()
                    && let Some(room) = &mut self.live_kit
                {
                    room.speaking = speaker_ids.binary_search(&id).is_ok();
                }
            }

            RoomEvent::TrackMuted {
                participant,
                publication,
            }
            | RoomEvent::TrackUnmuted {
                participant,
                publication,
            } => {
                let mut found = false;
                let user_id = participant.identity().0.parse()?;
                let track_id = publication.sid();
                if let Some(participant) = self.remote_participants.get_mut(&user_id) {
                    for (track, _) in participant.audio_tracks.values() {
                        if track.sid() == track_id {
                            found = true;
                            break;
                        }
                    }
                    if found {
                        participant.muted = publication.is_muted();
                    }
                }
            }

            RoomEvent::LocalTrackUnpublished { publication, .. } => {
                log::info!("unpublished track {}", publication.sid());
                if let Some(room) = &mut self.live_kit {
                    if let LocalTrack::Published {
                        track_publication, ..
                    } = &room.microphone_track
                        && track_publication.sid() == publication.sid()
                    {
                        room.microphone_track = LocalTrack::None;
                    }
                    if let LocalTrack::Published {
                        track_publication, ..
                    } = &room.screen_track
                        && track_publication.sid() == publication.sid()
                    {
                        room.screen_track = LocalTrack::None;
                    }
                }
            }

            RoomEvent::LocalTrackPublished { publication, .. } => {
                log::info!("published track {:?}", publication.sid());
            }

            RoomEvent::Disconnected { reason } => {
                log::info!("disconnected from room: {reason:?}");
                self.leave(cx).detach_and_log_err(cx);
            }
            _ => {}
        }

        cx.notify();
        Ok(())
    }

    fn check_invariants(&self) {
        #[cfg(any(test, feature = "test-support"))]
        {
            for participant in self.remote_participants.values() {
                assert!(
                    self.participant_user_ids
                        .contains(&participant.user.legacy_id)
                );
                assert_ne!(participant.user.legacy_id, self.client.user_id().unwrap());
            }

            for participant in &self.pending_participants {
                assert!(self.participant_user_ids.contains(&participant.legacy_id));
                assert_ne!(participant.legacy_id, self.client.user_id().unwrap());
            }

            assert_eq!(
                self.participant_user_ids.len(),
                self.remote_participants.len() + self.pending_participants.len()
            );
        }
    }
}
