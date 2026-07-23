use super::*;

pub(super) fn connection_quality_from_livekit(
    quality: livekit::prelude::ConnectionQuality,
) -> ConnectionQuality {
    match quality {
        livekit::prelude::ConnectionQuality::Excellent => ConnectionQuality::Excellent,
        livekit::prelude::ConnectionQuality::Good => ConnectionQuality::Good,
        livekit::prelude::ConnectionQuality::Poor => ConnectionQuality::Poor,
        livekit::prelude::ConnectionQuality::Lost => ConnectionQuality::Lost,
    }
}

fn participant_from_livekit(participant: livekit::participant::Participant) -> Participant {
    match participant {
        livekit::participant::Participant::Local(local) => {
            Participant::Local(LocalParticipant(local))
        }
        livekit::participant::Participant::Remote(remote) => {
            Participant::Remote(RemoteParticipant(remote))
        }
    }
}

fn publication_from_livekit(
    publication: livekit::publication::TrackPublication,
) -> TrackPublication {
    match publication {
        livekit::publication::TrackPublication::Local(local) => {
            TrackPublication::Local(LocalTrackPublication(local))
        }
        livekit::publication::TrackPublication::Remote(remote) => {
            TrackPublication::Remote(RemoteTrackPublication(remote))
        }
    }
}

fn remote_track_from_livekit(track: livekit::track::RemoteTrack) -> RemoteTrack {
    match track {
        livekit::track::RemoteTrack::Audio(audio) => RemoteTrack::Audio(RemoteAudioTrack(audio)),
        livekit::track::RemoteTrack::Video(video) => RemoteTrack::Video(RemoteVideoTrack(video)),
    }
}

fn local_track_from_livekit(track: livekit::track::LocalTrack) -> LocalTrack {
    match track {
        livekit::track::LocalTrack::Audio(audio) => LocalTrack::Audio(LocalAudioTrack(audio)),
        livekit::track::LocalTrack::Video(video) => LocalTrack::Video(LocalVideoTrack(video)),
    }
}

pub(super) fn room_event_from_livekit(event: livekit::RoomEvent) -> Option<RoomEvent> {
    let event = match event {
        livekit::RoomEvent::ParticipantConnected(remote_participant) => {
            RoomEvent::ParticipantConnected(RemoteParticipant(remote_participant))
        }
        livekit::RoomEvent::ParticipantDisconnected(remote_participant) => {
            RoomEvent::ParticipantDisconnected(RemoteParticipant(remote_participant))
        }
        livekit::RoomEvent::LocalTrackPublished {
            publication,
            track,
            participant,
        } => RoomEvent::LocalTrackPublished {
            publication: LocalTrackPublication(publication),
            track: local_track_from_livekit(track),
            participant: LocalParticipant(participant),
        },
        livekit::RoomEvent::LocalTrackUnpublished {
            publication,
            participant,
        } => RoomEvent::LocalTrackUnpublished {
            publication: LocalTrackPublication(publication),
            participant: LocalParticipant(participant),
        },
        livekit::RoomEvent::LocalTrackSubscribed { track } => RoomEvent::LocalTrackSubscribed {
            track: local_track_from_livekit(track),
        },
        livekit::RoomEvent::TrackSubscribed {
            track,
            publication,
            participant,
        } => RoomEvent::TrackSubscribed {
            track: remote_track_from_livekit(track),
            publication: RemoteTrackPublication(publication),
            participant: RemoteParticipant(participant),
        },
        livekit::RoomEvent::TrackUnsubscribed {
            track,
            publication,
            participant,
        } => RoomEvent::TrackUnsubscribed {
            track: remote_track_from_livekit(track),
            publication: RemoteTrackPublication(publication),
            participant: RemoteParticipant(participant),
        },
        livekit::RoomEvent::TrackSubscriptionFailed {
            participant,
            error: _,
            track_sid,
        } => RoomEvent::TrackSubscriptionFailed {
            participant: RemoteParticipant(participant),
            track_sid,
        },
        livekit::RoomEvent::TrackPublished {
            publication,
            participant,
        } => RoomEvent::TrackPublished {
            publication: RemoteTrackPublication(publication),
            participant: RemoteParticipant(participant),
        },
        livekit::RoomEvent::TrackUnpublished {
            publication,
            participant,
        } => RoomEvent::TrackUnpublished {
            publication: RemoteTrackPublication(publication),
            participant: RemoteParticipant(participant),
        },
        livekit::RoomEvent::TrackMuted {
            participant,
            publication,
        } => RoomEvent::TrackMuted {
            publication: publication_from_livekit(publication),
            participant: participant_from_livekit(participant),
        },
        livekit::RoomEvent::TrackUnmuted {
            participant,
            publication,
        } => RoomEvent::TrackUnmuted {
            publication: publication_from_livekit(publication),
            participant: participant_from_livekit(participant),
        },
        livekit::RoomEvent::RoomMetadataChanged {
            old_metadata,
            metadata,
        } => RoomEvent::RoomMetadataChanged {
            old_metadata,
            metadata,
        },
        livekit::RoomEvent::ParticipantMetadataChanged {
            participant,
            old_metadata,
            metadata,
        } => RoomEvent::ParticipantMetadataChanged {
            participant: participant_from_livekit(participant),
            old_metadata,
            metadata,
        },
        livekit::RoomEvent::ParticipantNameChanged {
            participant,
            old_name,
            name,
        } => RoomEvent::ParticipantNameChanged {
            participant: participant_from_livekit(participant),
            old_name,
            name,
        },
        livekit::RoomEvent::ParticipantAttributesChanged {
            participant,
            changed_attributes,
        } => RoomEvent::ParticipantAttributesChanged {
            participant: participant_from_livekit(participant),
            changed_attributes: changed_attributes.into_iter().collect(),
        },
        livekit::RoomEvent::ActiveSpeakersChanged { speakers } => {
            RoomEvent::ActiveSpeakersChanged {
                speakers: speakers.into_iter().map(participant_from_livekit).collect(),
            }
        }
        livekit::RoomEvent::Connected {
            participants_with_tracks,
        } => RoomEvent::Connected {
            participants_with_tracks: participants_with_tracks
                .into_iter()
                .map({
                    |(p, t)| {
                        (
                            RemoteParticipant(p),
                            t.into_iter().map(RemoteTrackPublication).collect(),
                        )
                    }
                })
                .collect(),
        },
        livekit::RoomEvent::Disconnected { reason } => RoomEvent::Disconnected {
            reason: reason.as_str_name(),
        },
        livekit::RoomEvent::Reconnecting => RoomEvent::Reconnecting,
        livekit::RoomEvent::Reconnected => RoomEvent::Reconnected,
        livekit::RoomEvent::ConnectionQualityChanged {
            quality,
            participant,
        } => RoomEvent::ConnectionQualityChanged {
            participant: participant_from_livekit(participant),
            quality: connection_quality_from_livekit(quality),
        },
        _ => {
            log::trace!("dropping livekit event: {:?}", event);
            return None;
        }
    };

    Some(event)
}
