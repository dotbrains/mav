use super::*;

pub struct ParticipantIdentity(pub String);

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct TrackSid(pub(crate) String);

impl std::fmt::Display for TrackSid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for TrackSid {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(TrackSid(value))
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ConnectionState {
    Connected,
    Disconnected,
}

#[derive(Clone, Debug, Default)]
pub struct SessionStats {
    pub publisher_stats: Vec<RtcStats>,
    pub subscriber_stats: Vec<RtcStats>,
}

#[derive(Clone, Debug)]
pub enum RtcStats {}
