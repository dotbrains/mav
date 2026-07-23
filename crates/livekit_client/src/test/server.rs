use super::*;

static SERVERS: Mutex<BTreeMap<String, Arc<TestServer>>> = Mutex::new(BTreeMap::new());

pub struct TestServer {
    pub url: String,
    pub api_key: String,
    pub secret_key: String,
    pub(super) rooms: Mutex<HashMap<String, TestServerRoom>>,
    pub(super) executor: BackgroundExecutor,
}

impl TestServer {
    pub fn create(
        url: String,
        api_key: String,
        secret_key: String,
        executor: BackgroundExecutor,
    ) -> Result<Arc<TestServer>> {
        let mut servers = SERVERS.lock();
        if let BTreeEntry::Vacant(e) = servers.entry(url.clone()) {
            let server = Arc::new(TestServer {
                url,
                api_key,
                secret_key,
                rooms: Default::default(),
                executor,
            });
            e.insert(server.clone());
            Ok(server)
        } else {
            anyhow::bail!("a server with url {url:?} already exists");
        }
    }

    pub(super) fn get(url: &str) -> Result<Arc<TestServer>> {
        Ok(SERVERS
            .lock()
            .get(url)
            .context("no server found for url")?
            .clone())
    }

    pub fn teardown(&self) -> Result<()> {
        SERVERS
            .lock()
            .remove(&self.url)
            .with_context(|| format!("server with url {:?} does not exist", self.url))?;
        Ok(())
    }

    pub fn create_api_client(&self) -> TestApiClient {
        TestApiClient {
            url: self.url.clone(),
        }
    }

    pub async fn create_room(&self, room: String) -> Result<()> {
        self.simulate_random_delay().await;

        let mut server_rooms = self.rooms.lock();
        if let Entry::Vacant(e) = server_rooms.entry(room.clone()) {
            e.insert(Default::default());
            Ok(())
        } else {
            anyhow::bail!("{room:?} already exists");
        }
    }

    pub(super) async fn delete_room(&self, room: String) -> Result<()> {
        self.simulate_random_delay().await;

        let mut server_rooms = self.rooms.lock();
        server_rooms
            .remove(&room)
            .with_context(|| format!("room {room:?} does not exist"))?;
        Ok(())
    }

    pub(super) async fn join_room(
        &self,
        token: String,
        client_room: Room,
    ) -> Result<ParticipantIdentity> {
        self.simulate_random_delay().await;

        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap();
        let mut server_rooms = self.rooms.lock();
        let room = (*server_rooms).entry(room_name.to_string()).or_default();

        if let Entry::Vacant(e) = room.client_rooms.entry(identity.clone()) {
            for server_track in &room.video_tracks {
                let track = RemoteTrack::Video(RemoteVideoTrack {
                    server_track: server_track.clone(),
                    _room: client_room.downgrade(),
                });
                client_room
                    .0
                    .lock()
                    .updates_tx
                    .blocking_send(RoomEvent::TrackSubscribed {
                        track: track.clone(),
                        publication: RemoteTrackPublication {
                            sid: server_track.sid.clone(),
                            room: client_room.downgrade(),
                            track,
                        },
                        participant: RemoteParticipant {
                            room: client_room.downgrade(),
                            identity: server_track.publisher_id.clone(),
                        },
                    })
                    .unwrap();
            }
            for server_track in &room.audio_tracks {
                let track = RemoteTrack::Audio(RemoteAudioTrack {
                    server_track: server_track.clone(),
                    room: client_room.downgrade(),
                });
                client_room
                    .0
                    .lock()
                    .updates_tx
                    .blocking_send(RoomEvent::TrackSubscribed {
                        track: track.clone(),
                        publication: RemoteTrackPublication {
                            sid: server_track.sid.clone(),
                            room: client_room.downgrade(),
                            track,
                        },
                        participant: RemoteParticipant {
                            room: client_room.downgrade(),
                            identity: server_track.publisher_id.clone(),
                        },
                    })
                    .unwrap();
            }
            e.insert(client_room);
            Ok(identity)
        } else {
            anyhow::bail!("{identity:?} attempted to join room {room_name:?} twice");
        }
    }

    pub(super) async fn leave_room(&self, token: String) -> Result<()> {
        self.simulate_random_delay().await;

        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap();
        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&*room_name)
            .with_context(|| format!("room {room_name:?} does not exist"))?;
        room.client_rooms.remove(&identity).with_context(|| {
            format!("{identity:?} attempted to leave room {room_name:?} before joining it")
        })?;
        Ok(())
    }

    pub(super) fn remote_participants(
        &self,
        token: String,
    ) -> Result<HashMap<ParticipantIdentity, RemoteParticipant>> {
        let claims = livekit_api::token::validate(&token, &self.secret_key)?;
        let local_identity = ParticipantIdentity(claims.sub.unwrap().to_string());
        let room_name = claims.video.room.unwrap().to_string();

        if let Some(server_room) = self.rooms.lock().get(&room_name) {
            let room = server_room
                .client_rooms
                .get(&local_identity)
                .unwrap()
                .downgrade();
            Ok(server_room
                .client_rooms
                .iter()
                .filter(|(identity, _)| *identity != &local_identity)
                .map(|(identity, _)| {
                    (
                        identity.clone(),
                        RemoteParticipant {
                            room: room.clone(),
                            identity: identity.clone(),
                        },
                    )
                })
                .collect())
        } else {
            Ok(Default::default())
        }
    }

    pub(super) async fn remove_participant(
        &self,
        room_name: String,
        identity: ParticipantIdentity,
    ) -> Result<()> {
        self.simulate_random_delay().await;

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;
        room.client_rooms
            .remove(&identity)
            .with_context(|| format!("participant {identity:?} did not join room {room_name:?}"))?;
        Ok(())
    }

    pub(super) async fn update_participant(
        &self,
        room_name: String,
        identity: String,
        permission: proto::ParticipantPermission,
    ) -> Result<()> {
        self.simulate_random_delay().await;

        let mut server_rooms = self.rooms.lock();
        let room = server_rooms
            .get_mut(&room_name)
            .with_context(|| format!("room {room_name} does not exist"))?;
        room.participant_permissions
            .insert(ParticipantIdentity(identity), permission);
        Ok(())
    }

    pub async fn disconnect_client(&self, client_identity: String) {
        let client_identity = ParticipantIdentity(client_identity);

        self.simulate_random_delay().await;

        let mut server_rooms = self.rooms.lock();
        for room in server_rooms.values_mut() {
            if let Some(room) = room.client_rooms.remove(&client_identity) {
                let mut room = room.0.lock();
                room.connection_state = ConnectionState::Disconnected;
                room.updates_tx
                    .blocking_send(RoomEvent::Disconnected {
                        reason: "SIGNAL_CLOSED",
                    })
                    .ok();
            }
        }
    }

    pub(super) async fn simulate_random_delay(&self) {
        #[cfg(any(test, feature = "test-support"))]
        self.executor.simulate_random_delay().await;
    }
}
