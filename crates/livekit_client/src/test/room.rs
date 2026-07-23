use super::*;

pub(crate) struct RoomState {
    pub(crate) url: String,
    pub(crate) token: String,
    pub(crate) local_identity: ParticipantIdentity,
    pub(crate) connection_state: ConnectionState,
    pub(crate) paused_audio_tracks: HashSet<TrackSid>,
    pub(crate) updates_tx: mpsc::Sender<RoomEvent>,
}

#[derive(Clone, Debug)]
pub struct Room(pub(crate) Arc<Mutex<RoomState>>);

#[derive(Clone, Debug)]
pub(crate) struct WeakRoom(Weak<Mutex<RoomState>>);

impl std::fmt::Debug for RoomState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Room")
            .field("url", &self.url)
            .field("token", &self.token)
            .field("local_identity", &self.local_identity)
            .field("connection_state", &self.connection_state)
            .field("paused_audio_tracks", &self.paused_audio_tracks)
            .finish()
    }
}

impl Room {
    pub(crate) fn downgrade(&self) -> WeakRoom {
        WeakRoom(Arc::downgrade(&self.0))
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.0.lock().connection_state
    }

    pub fn local_participant(&self) -> LocalParticipant {
        let identity = self.0.lock().local_identity.clone();
        LocalParticipant {
            identity,
            room: self.clone(),
        }
    }

    pub async fn connect(
        url: String,
        token: String,
        _cx: &mut AsyncApp,
    ) -> Result<(Self, mpsc::Receiver<RoomEvent>)> {
        let server = TestServer::get(&url)?;
        let (updates_tx, updates_rx) = mpsc::channel(1024);
        let this = Self(Arc::new(Mutex::new(RoomState {
            local_identity: ParticipantIdentity(String::new()),
            url: url.to_string(),
            token: token.to_string(),
            connection_state: ConnectionState::Disconnected,
            paused_audio_tracks: Default::default(),
            updates_tx,
        })));

        let identity = server
            .join_room(token.to_string(), this.clone())
            .await
            .context("room join")?;
        {
            let mut state = this.0.lock();
            state.local_identity = identity;
            state.connection_state = ConnectionState::Connected;
        }

        Ok((this, updates_rx))
    }

    pub fn remote_participants(&self) -> HashMap<ParticipantIdentity, RemoteParticipant> {
        self.test_server()
            .remote_participants(self.0.lock().token.clone())
            .unwrap()
    }

    pub(crate) fn test_server(&self) -> Arc<TestServer> {
        TestServer::get(&self.0.lock().url).unwrap()
    }

    pub(crate) fn token(&self) -> String {
        self.0.lock().token.clone()
    }

    pub fn name(&self) -> String {
        "test_room".to_string()
    }

    pub async fn sid(&self) -> String {
        "RM_test_session".to_string()
    }

    pub fn play_remote_audio_track(
        &self,
        _track: &RemoteAudioTrack,
        _cx: &App,
    ) -> anyhow::Result<AudioStream> {
        Ok(AudioStream {})
    }

    pub async fn unpublish_local_track(&self, sid: TrackSid, cx: &mut AsyncApp) -> Result<()> {
        self.local_participant().unpublish_track(sid, cx).await
    }

    pub async fn publish_local_microphone_track(
        &self,
        _track_name: String,
        _is_staff: bool,
        cx: &mut AsyncApp,
    ) -> Result<(LocalTrackPublication, AudioStream, Arc<AtomicU64>)> {
        self.local_participant().publish_microphone_track(cx).await
    }

    pub async fn get_stats(&self) -> Result<SessionStats> {
        Ok(SessionStats::default())
    }

    pub fn stats_task(&self, _cx: &impl gpui::AppContext) -> gpui::Task<Result<SessionStats>> {
        gpui::Task::ready(Ok(SessionStats::default()))
    }
}

impl Drop for RoomState {
    fn drop(&mut self) {
        if self.connection_state == ConnectionState::Connected
            && let Ok(server) = TestServer::get(&self.url)
        {
            let executor = server.executor.clone();
            let token = self.token.clone();
            executor
                .spawn(async move { server.leave_room(token).await.ok() })
                .detach();
        }
    }
}

impl WeakRoom {
    pub(crate) fn upgrade(&self) -> Option<Room> {
        self.0.upgrade().map(Room)
    }
}
