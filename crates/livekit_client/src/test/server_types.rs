use super::*;

#[derive(Default, Debug)]
struct TestServerRoom {
    client_rooms: HashMap<ParticipantIdentity, Room>,
    video_tracks: Vec<Arc<TestServerVideoTrack>>,
    audio_tracks: Vec<Arc<TestServerAudioTrack>>,
    participant_permissions: HashMap<ParticipantIdentity, proto::ParticipantPermission>,
}

#[derive(Debug)]
pub(crate) struct TestServerVideoTrack {
    pub(crate) sid: TrackSid,
    pub(crate) publisher_id: ParticipantIdentity,
    // frames_rx: async_broadcast::Receiver<Frame>,
}

#[derive(Debug)]
pub(crate) struct TestServerAudioTrack {
    pub(crate) sid: TrackSid,
    pub(crate) publisher_id: ParticipantIdentity,
    pub(crate) muted: AtomicBool,
}
