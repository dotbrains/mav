use channel::ChannelBuffer;
use client::{Collaborator, ParticipantIndex, proto::PeerId};
use collections::HashMap;
use editor::CollaborationHub;
use gpui::{App, Entity, SharedString};

pub(super) struct ChannelBufferCollaborationHub(pub(super) Entity<ChannelBuffer>);

impl CollaborationHub for ChannelBufferCollaborationHub {
    fn collaborators<'a>(&self, cx: &'a App) -> &'a HashMap<PeerId, Collaborator> {
        self.0.read(cx).collaborators()
    }

    fn user_participant_indices<'a>(&self, cx: &'a App) -> &'a HashMap<u64, ParticipantIndex> {
        self.0.read(cx).user_store().read(cx).participant_indices()
    }

    fn user_names(&self, cx: &App) -> HashMap<u64, SharedString> {
        let user_ids = self.collaborators(cx).values().map(|c| c.user_id);
        self.0
            .read(cx)
            .user_store()
            .read(cx)
            .participant_names(user_ids, cx)
    }
}
