use super::*;

impl TestServer {
    pub(crate) async fn publish_video_track(
        &self,
        token: String,
        _local_track: LocalVideoTrack,
    ) -> Result<TrackSid> {
        self.simulate_random_delay().await;

        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap();

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;

        let can_publish = room
            .participant_permissions
            .get(&identity)
            .map(|permission| permission.can_publish)
            .or(claims.video.can_publish)
            .unwrap_or(true);

        anyhow::ensure!(can_publish, "user is not allowed to publish");

        let sid: TrackSid = format!("TR_{}", nanoid::nanoid!(17)).try_into().unwrap();
        let server_track = Arc::new(TestServerVideoTrack {
            sid: sid.clone(),
            publisher_id: identity.clone(),
        });

        room.video_tracks.push(server_track.clone());

        for (room_identity, client_room) in &room.client_rooms {
            if *room_identity != identity {
                let track = RemoteTrack::Video(RemoteVideoTrack {
                    server_track: server_track.clone(),
                    _room: client_room.downgrade(),
                });
                let publication = RemoteTrackPublication {
                    sid: sid.clone(),
                    room: client_room.downgrade(),
                    track: track.clone(),
                };
                let participant = RemoteParticipant {
                    identity: identity.clone(),
                    room: client_room.downgrade(),
                };
                client_room
                    .0
                    .lock()
                    .updates_tx
                    .blocking_send(RoomEvent::TrackSubscribed {
                        track,
                        publication,
                        participant,
                    })
                    .unwrap();
            }
        }

        Ok(sid)
    }

    pub(crate) async fn publish_audio_track(
        &self,
        token: String,
        _local_track: &LocalAudioTrack,
    ) -> Result<TrackSid> {
        self.simulate_random_delay().await;

        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap();

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;

        let can_publish = room
            .participant_permissions
            .get(&identity)
            .map(|permission| permission.can_publish)
            .or(claims.video.can_publish)
            .unwrap_or(true);

        anyhow::ensure!(can_publish, "user is not allowed to publish");

        let sid: TrackSid = format!("TR_{}", nanoid::nanoid!(17)).try_into().unwrap();
        let server_track = Arc::new(TestServerAudioTrack {
            sid: sid.clone(),
            publisher_id: identity.clone(),
            muted: AtomicBool::new(false),
        });

        room.audio_tracks.push(server_track.clone());

        for (room_identity, client_room) in &room.client_rooms {
            if *room_identity != identity {
                let track = RemoteTrack::Audio(RemoteAudioTrack {
                    server_track: server_track.clone(),
                    room: client_room.downgrade(),
                });
                let publication = RemoteTrackPublication {
                    sid: sid.clone(),
                    room: client_room.downgrade(),
                    track: track.clone(),
                };
                let participant = RemoteParticipant {
                    identity: identity.clone(),
                    room: client_room.downgrade(),
                };
                client_room
                    .0
                    .lock()
                    .updates_tx
                    .blocking_send(RoomEvent::TrackSubscribed {
                        track,
                        publication,
                        participant,
                    })
                    .ok();
            }
        }

        Ok(sid)
    }

    pub(crate) async fn unpublish_track(&self, token: String, track_sid: &TrackSid) -> Result<()> {
        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap();

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;

        if let Some(video_to_unpublish) = room.video_tracks.iter().position(|t| t.sid == *track_sid)
        {
            let video_to_unpublish = room.video_tracks.remove(video_to_unpublish);
            for client_room in room
                .client_rooms
                .iter()
                .filter(|(id, _)| **id != identity)
                .map(|(_, room)| room)
            {
                let track = RemoteTrack::Video(RemoteVideoTrack {
                    server_track: video_to_unpublish.clone(),
                    _room: client_room.downgrade(),
                });
                let publication = RemoteTrackPublication {
                    sid: track_sid.clone(),
                    room: client_room.downgrade(),
                    track: track.clone(),
                };
                let participant = RemoteParticipant {
                    identity: identity.clone(),
                    room: client_room.downgrade(),
                };
                let event = RoomEvent::TrackUnsubscribed {
                    track,
                    publication,
                    participant,
                };

                client_room.0.lock().updates_tx.blocking_send(event).ok();
            }
        }

        if let Some(audio_to_unpublish) = room.audio_tracks.iter().position(|t| t.sid == *track_sid)
        {
            let audio_to_unpublish = room.audio_tracks.remove(audio_to_unpublish);
            for client_room in room
                .client_rooms
                .iter()
                .filter(|(id, _)| **id != identity)
                .map(|(_, room)| room)
            {
                let track = RemoteTrack::Audio(RemoteAudioTrack {
                    server_track: audio_to_unpublish.clone(),
                    room: client_room.downgrade(),
                });
                let publication = RemoteTrackPublication {
                    sid: track_sid.clone(),
                    room: client_room.downgrade(),
                    track: track.clone(),
                };
                let participant = RemoteParticipant {
                    identity: identity.clone(),
                    room: client_room.downgrade(),
                };
                let event = RoomEvent::TrackUnsubscribed {
                    track,
                    publication,
                    participant,
                };

                client_room.0.lock().updates_tx.blocking_send(event).ok();
            }
        }

        Ok(())
    }

    pub(crate) fn set_track_muted(
        &self,
        token: &str,
        track_sid: &TrackSid,
        muted: bool,
    ) -> Result<()> {
        let claims = livekit_api::token::validate(token, &self.secret_key)?;
        let room_name = claims.video.room.unwrap();
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;
        if let Some(track) = room
            .audio_tracks
            .iter_mut()
            .find(|track| track.sid == *track_sid)
        {
            track.muted.store(muted, SeqCst);
            for (id, client_room) in room.client_rooms.iter() {
                if *id != identity {
                    let participant = Participant::Remote(RemoteParticipant {
                        identity: identity.clone(),
                        room: client_room.downgrade(),
                    });
                    let track = RemoteTrack::Audio(RemoteAudioTrack {
                        server_track: track.clone(),
                        room: client_room.downgrade(),
                    });
                    let publication = TrackPublication::Remote(RemoteTrackPublication {
                        sid: track_sid.clone(),
                        room: client_room.downgrade(),
                        track,
                    });

                    let event = if muted {
                        RoomEvent::TrackMuted {
                            participant,
                            publication,
                        }
                    } else {
                        RoomEvent::TrackUnmuted {
                            participant,
                            publication,
                        }
                    };

                    client_room
                        .0
                        .lock()
                        .updates_tx
                        .blocking_send(event)
                        .unwrap();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn is_track_muted(&self, token: &str, track_sid: &TrackSid) -> Option<bool> {
        let claims = livekit_api::token::validate(token, &self.secret_key).ok()?;
        let room_name = claims.video.room.unwrap();

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms.get_mut(&*room_name)?;
        room.audio_tracks.iter().find_map(|track| {
            if track.sid == *track_sid {
                Some(track.muted.load(SeqCst))
            } else {
                None
            }
        })
    }

    pub(crate) fn video_tracks(&self, token: String) -> Result<Vec<RemoteVideoTrack>> {
        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let room_name = claims.video.room.unwrap();
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;
        let client_room = room
            .client_rooms
            .get(&identity)
            .context("not a participant in room")?;
        Ok(room
            .video_tracks
            .iter()
            .map(|track| RemoteVideoTrack {
                server_track: track.clone(),
                _room: client_room.downgrade(),
            })
            .collect())
    }

    pub(crate) fn audio_tracks(&self, token: String) -> Result<Vec<RemoteAudioTrack>> {
        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let room_name = claims.video.room.unwrap();
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;
        let client_room = room
            .client_rooms
            .get(&identity)
            .context("not a participant in room")?;
        Ok(room
            .audio_tracks
            .iter()
            .map(|track| RemoteAudioTrack {
                server_track: track.clone(),
                room: client_room.downgrade(),
            })
            .collect())
    }
}
