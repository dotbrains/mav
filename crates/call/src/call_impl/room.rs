use crate::{
    call_settings::CallSettings,
    participant::{LocalParticipant, RemoteParticipant},
};
use anyhow::{Context as _, Result, anyhow};
use audio::{Audio, Sound};
use client::{
    ChannelId, Client, ParticipantIndex, TypedEnvelope, User, UserStore,
    proto::{self, PeerId},
};
use collections::{BTreeMap, HashMap, HashSet};
use feature_flags::FeatureFlagAppExt;
use fs::Fs;
use futures::StreamExt;
use gpui::{
    App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, FutureExt as _,
    ScreenCaptureSource, ScreenCaptureStream, Task, TaskExt, Timeout, WeakEntity,
};
use gpui_tokio::Tokio;
use language::LanguageRegistry;
use livekit::{LocalTrackPublication, ParticipantIdentity, RoomEvent};
use livekit_client::{self as livekit, AudioStream, TrackSid};
use postage::{sink::Sink, stream::Stream, watch};
use project::{CURRENT_PROJECT_FEATURES, Project};
use settings::Settings as _;
use std::sync::atomic::AtomicU64;
use std::{future::Future, mem, rc::Rc, sync::Arc, time::Duration, time::Instant};

use super::diagnostics::CallDiagnostics;
use util::{ResultExt, TryFutureExt, paths::PathStyle, post_inc};
use workspace::ParticipantLocation;

pub const RECONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    RoomJoined {
        channel_id: Option<ChannelId>,
    },
    ParticipantLocationChanged {
        participant_id: proto::PeerId,
    },
    RemoteVideoTracksChanged {
        participant_id: proto::PeerId,
    },
    RemoteVideoTrackUnsubscribed {
        sid: TrackSid,
    },
    RemoteAudioTracksChanged {
        participant_id: proto::PeerId,
    },
    RemoteProjectShared {
        owner: Arc<User>,
        project_id: u64,
        worktree_root_names: Vec<String>,
    },
    RemoteProjectUnshared {
        project_id: u64,
    },
    RemoteProjectJoined {
        project_id: u64,
    },
    RemoteProjectInvitationDiscarded {
        project_id: u64,
    },
    RoomLeft {
        channel_id: Option<ChannelId>,
    },
    LocalScreenShareStarted,
    LocalScreenShareStopped,
}

pub struct Room {
    id: u64,
    channel_id: Option<ChannelId>,
    live_kit: Option<LiveKitRoom>,
    diagnostics: Option<Entity<CallDiagnostics>>,
    status: RoomStatus,
    shared_projects: HashSet<WeakEntity<Project>>,
    joined_projects: HashSet<WeakEntity<Project>>,
    local_participant: LocalParticipant,
    remote_participants: BTreeMap<u64, RemoteParticipant>,
    pending_participants: Vec<Arc<User>>,
    participant_user_ids: HashSet<u64>,
    pending_call_count: usize,
    leave_when_empty: bool,
    client: Arc<Client>,
    user_store: Entity<UserStore>,
    follows_by_leader_id_project_id: HashMap<(PeerId, u64), Vec<PeerId>>,
    client_subscriptions: Vec<client::Subscription>,
    _subscriptions: Vec<gpui::Subscription>,
    room_update_completed_tx: watch::Sender<Option<()>>,
    room_update_completed_rx: watch::Receiver<Option<()>>,
    pending_room_update: Option<Task<()>>,
    maintain_connection: Option<Task<Option<()>>>,
    created: Instant,
}

impl EventEmitter<Event> for Room {}

mod lifecycle;
mod media;
mod participants;
mod projects;
mod updates;

fn spawn_room_connection(
    livekit_connection_info: Option<proto::LiveKitConnectionInfo>,
    cx: &mut Context<Room>,
) {
    if let Some(connection_info) = livekit_connection_info {
        cx.spawn(async move |this, cx| {
            let (room, mut events) =
                livekit::Room::connect(connection_info.server_url, connection_info.token, cx)
                    .await?;

            let weak_room = this.clone();
            this.update(cx, |this, cx| {
                let _handle_updates = cx.spawn(async move |this, cx| {
                    while let Some(event) = events.next().await {
                        if this
                            .update(cx, |this, cx| {
                                this.livekit_room_updated(event, cx).warn_on_err();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                let muted_by_user = Room::mute_on_join(cx);
                this.live_kit = Some(LiveKitRoom {
                    room: Rc::new(room),
                    screen_track: LocalTrack::None,
                    microphone_track: LocalTrack::None,
                    input_lag_us: None,
                    next_publish_id: 0,
                    muted_by_user,
                    deafened: false,
                    speaking: false,
                    _handle_updates,
                });
                this.diagnostics = Some(cx.new(|cx| CallDiagnostics::new(weak_room, cx)));

                // Always open the microphone track on join, even when
                // `muted_by_user` is set. Note that the microphone will still
                // be muted, as it is still gated in `share_microphone` by
                // `muted_by_user`. For users that have `mute_on_join` enabled,
                // this moves the Bluetooth profile switch (A2DP -> HFP) (which
                // can cause 1-2 seconds of audio silence on some Bluetooth
                // headphones) from first unmute to channel join, where
                // instability is expected.
                if this.can_use_microphone() {
                    this.share_microphone(cx)
                } else {
                    Task::ready(Ok(()))
                }
            })?
            .await
        })
        .detach_and_log_err(cx);
    }
}

struct LiveKitRoom {
    room: Rc<livekit::Room>,
    screen_track: LocalTrack<dyn ScreenCaptureStream>,
    microphone_track: LocalTrack<AudioStream>,
    /// Shared atomic storing the most recent input lag measurement in microseconds.
    /// Written by the audio capture/transmit pipeline, read here for diagnostics.
    input_lag_us: Option<Arc<AtomicU64>>,
    /// Tracks whether we're currently in a muted state due to auto-mute from deafening or manual mute performed by user.
    muted_by_user: bool,
    deafened: bool,
    speaking: bool,
    next_publish_id: usize,
    _handle_updates: Task<()>,
}

impl LiveKitRoom {
    fn stop_publishing(&mut self, cx: &mut Context<Room>) {
        let mut tracks_to_unpublish = Vec::new();
        if let LocalTrack::Published {
            track_publication, ..
        } = mem::replace(&mut self.microphone_track, LocalTrack::None)
        {
            tracks_to_unpublish.push(track_publication.sid());
            self.input_lag_us = None;
            cx.notify();
        }

        if let LocalTrack::Published {
            track_publication, ..
        } = mem::replace(&mut self.screen_track, LocalTrack::None)
        {
            tracks_to_unpublish.push(track_publication.sid());
            cx.notify();
        }

        let participant = self.room.local_participant();
        cx.spawn(async move |_, cx| {
            for sid in tracks_to_unpublish {
                participant.unpublish_track(sid, cx).await.log_err();
            }
        })
        .detach();
    }
}

#[derive(Default)]
enum LocalTrack<Stream: ?Sized> {
    #[default]
    None,
    Pending {
        publish_id: usize,
    },
    Published {
        track_publication: LocalTrackPublication,
        _stream: Box<Stream>,
    },
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum RoomStatus {
    Online,
    Rejoining,
    Offline,
}

impl RoomStatus {
    pub fn is_offline(&self) -> bool {
        matches!(self, RoomStatus::Offline)
    }

    pub fn is_online(&self) -> bool {
        matches!(self, RoomStatus::Online)
    }
}
