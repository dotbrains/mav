use super::*;

pub(super) const FILTER_OCCUPIED_CHANNELS_KEY: &str = "filter_occupied_channels";
pub(super) const FAVORITE_CHANNELS_KEY: &str = "favorite_channels";
pub(super) const COLLABORATION_PANEL_KEY: &str = "CollaborationPanel";
pub(super) const TOAST_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct ChannelMoveClipboard {
    pub(super) channel_id: ChannelId,
}

#[derive(Debug)]
pub enum ChannelEditingState {
    Create {
        location: Option<ChannelId>,
        pending_name: Option<String>,
    },
    Rename {
        location: ChannelId,
        pending_name: Option<String>,
    },
}

impl ChannelEditingState {
    pub(super) fn pending_name(&self) -> Option<String> {
        match self {
            ChannelEditingState::Create { pending_name, .. } => pending_name.clone(),
            ChannelEditingState::Rename { pending_name, .. } => pending_name.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct SerializedCollabPanel {
    pub(super) collapsed_channels: Option<Vec<u64>>,
}
