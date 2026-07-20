use super::*;

impl Workspace {
    pub fn auto_watch_state(&self) -> &AutoWatch {
        &self.auto_watch
    }

    pub(crate) fn next_watched_peer(&self, cx: &App) -> Option<PeerId> {
        self.active_call()
            .and_then(|call| call.peer_ids_with_video_tracks(cx).first().copied())
    }

    pub fn toggle_auto_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.auto_watch.enabled() {
            self.auto_watch = AutoWatch::Off;
            cx.notify();
            return;
        }

        let active_pane = self.active_pane.clone();
        self.unfollow_in_pane(&active_pane, window, cx);

        let local_is_sharing = self
            .active_call()
            .map_or(false, |call| call.is_sharing_screen(cx));

        if local_is_sharing {
            self.auto_watch = AutoWatch::Paused;
        } else {
            let watched_peer = self.next_watched_peer(cx);
            self.auto_watch = AutoWatch::Active { watched_peer };

            if let Some(peer_id) = watched_peer {
                self.open_shared_screen(peer_id, window, cx);
            }
        }

        cx.notify();
    }

    pub(crate) fn handle_auto_watch_video_tracks_changed(
        &mut self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AutoWatch::Active { watched_peer } = self.auto_watch else {
            return;
        };

        let peer_is_sharing = self.active_call().map_or(false, |call| {
            call.peer_ids_with_video_tracks(cx).contains(&peer_id)
        });
        let should_watch_peer = peer_is_sharing && watched_peer.is_none();
        let watched_peer_stopped_sharing = watched_peer == Some(peer_id) && !peer_is_sharing;

        if should_watch_peer || watched_peer_stopped_sharing {
            let next_watched_peer = if should_watch_peer {
                Some(peer_id)
            } else {
                self.next_watched_peer(cx)
            };

            self.auto_watch = AutoWatch::Active {
                watched_peer: next_watched_peer,
            };

            if let Some(next_watched_peer) = next_watched_peer {
                self.open_shared_screen(next_watched_peer, window, cx);
            }
        }
    }

    pub(crate) fn handle_auto_watch_local_share_stopped(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AutoWatch::Paused = self.auto_watch else {
            return;
        };

        let watched_peer = self.next_watched_peer(cx);
        self.auto_watch = AutoWatch::Active { watched_peer };

        if let Some(peer_id) = watched_peer {
            self.open_shared_screen(peer_id, window, cx);
        }
    }

    pub fn active_call(&self) -> Option<&dyn AnyActiveCall> {
        self.active_call.as_ref().map(|(call, _)| &*call.0)
    }

    pub fn active_global_call(&self) -> Option<GlobalAnyActiveCall> {
        self.active_call.as_ref().map(|(call, _)| call.clone())
    }

    pub(crate) fn on_active_call_event(
        &mut self,
        event: &ActiveCallEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActiveCallEvent::ParticipantLocationChanged { participant_id } => {
                self.leader_updated(participant_id, window, cx);
            }
            ActiveCallEvent::RemoteVideoTracksChanged { participant_id } => {
                self.leader_updated(participant_id, window, cx);
                self.handle_auto_watch_video_tracks_changed(*participant_id, window, cx);
            }
            ActiveCallEvent::LocalScreenShareStarted => {
                if let AutoWatch::Active { .. } = self.auto_watch {
                    self.auto_watch = AutoWatch::Paused;
                    cx.notify();
                }
            }
            ActiveCallEvent::LocalScreenShareStopped => {
                self.handle_auto_watch_local_share_stopped(window, cx);
            }
            ActiveCallEvent::RoomLeft => {
                if self.auto_watch.enabled() {
                    self.auto_watch = AutoWatch::Off;
                    cx.notify();
                }
            }
        }
    }
}
