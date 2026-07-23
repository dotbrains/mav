use super::*;

impl Room {
    pub fn is_sharing_screen(&self) -> bool {
        self.live_kit
            .as_ref()
            .is_some_and(|live_kit| !matches!(live_kit.screen_track, LocalTrack::None))
    }

    pub fn shared_screen_id(&self) -> Option<u64> {
        self.live_kit.as_ref().and_then(|lk| match lk.screen_track {
            LocalTrack::Published { ref _stream, .. } => {
                _stream.metadata().ok().map(|meta| meta.id)
            }
            _ => None,
        })
    }

    pub fn is_sharing_mic(&self) -> bool {
        self.live_kit
            .as_ref()
            .is_some_and(|live_kit| !matches!(live_kit.microphone_track, LocalTrack::None))
    }

    pub fn is_muted(&self) -> bool {
        self.live_kit.as_ref().is_some_and(|live_kit| {
            matches!(live_kit.microphone_track, LocalTrack::None)
                || live_kit.muted_by_user
                || live_kit.deafened
        })
    }

    pub fn muted_by_user(&self) -> bool {
        self.live_kit
            .as_ref()
            .is_some_and(|live_kit| live_kit.muted_by_user)
    }

    pub fn is_speaking(&self) -> bool {
        self.live_kit
            .as_ref()
            .is_some_and(|live_kit| live_kit.speaking)
    }

    pub fn is_deafened(&self) -> Option<bool> {
        self.live_kit.as_ref().map(|live_kit| live_kit.deafened)
    }

    pub fn can_use_microphone(&self) -> bool {
        use proto::ChannelRole::*;

        match self.local_participant.role {
            Admin | Member | Talker => true,
            Guest | Banned => false,
        }
    }

    pub fn can_share_projects(&self) -> bool {
        use proto::ChannelRole::*;
        match self.local_participant.role {
            Admin | Member => true,
            Guest | Banned | Talker => false,
        }
    }

    #[track_caller]
    pub fn share_microphone(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        let (room, publish_id) = if let Some(live_kit) = self.live_kit.as_mut() {
            let publish_id = post_inc(&mut live_kit.next_publish_id);
            live_kit.microphone_track = LocalTrack::Pending { publish_id };
            cx.notify();
            (live_kit.room.clone(), publish_id)
        } else {
            return Task::ready(Err(anyhow!("live-kit was not initialized")));
        };

        let is_staff = cx.is_staff();
        let user_name = self
            .user_store
            .read(cx)
            .current_user()
            .and_then(|user| user.name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        cx.spawn(async move |this, cx| {
            let publication = room
                .publish_local_microphone_track(user_name, is_staff, cx)
                .await;
            this.update(cx, |this, cx| {
                let live_kit = this
                    .live_kit
                    .as_mut()
                    .context("live-kit was not initialized")?;

                let canceled = if let LocalTrack::Pending {
                    publish_id: cur_publish_id,
                } = &live_kit.microphone_track
                {
                    *cur_publish_id != publish_id
                } else {
                    true
                };

                match publication {
                    Ok((publication, stream, input_lag_us)) => {
                        if canceled {
                            cx.spawn(async move |_, cx| {
                                room.unpublish_local_track(publication.sid(), cx).await
                            })
                            .detach_and_log_err(cx)
                        } else {
                            if live_kit.muted_by_user || live_kit.deafened {
                                publication.mute(cx);
                            }
                            live_kit.input_lag_us = Some(input_lag_us);
                            live_kit.microphone_track = LocalTrack::Published {
                                track_publication: publication,
                                _stream: Box::new(stream),
                            };
                            cx.notify();
                        }
                        Ok(())
                    }
                    Err(error) => {
                        if canceled {
                            Ok(())
                        } else {
                            live_kit.microphone_track = LocalTrack::None;
                            cx.notify();
                            Err(error)
                        }
                    }
                }
            })?
        })
    }

    pub fn share_screen(
        &mut self,
        source: Rc<dyn ScreenCaptureSource>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }
        if self.is_sharing_screen() {
            return Task::ready(Err(anyhow!("screen was already shared")));
        }

        let (participant, publish_id) = if let Some(live_kit) = self.live_kit.as_mut() {
            let publish_id = post_inc(&mut live_kit.next_publish_id);
            live_kit.screen_track = LocalTrack::Pending { publish_id };
            cx.notify();
            (live_kit.room.local_participant(), publish_id)
        } else {
            return Task::ready(Err(anyhow!("live-kit was not initialized")));
        };

        cx.spawn(async move |this, cx| {
            let publication = participant.publish_screenshare_track(&*source, cx).await;

            this.update(cx, |this, cx| {
                let live_kit = this
                    .live_kit
                    .as_mut()
                    .context("live-kit was not initialized")?;

                let canceled = if let LocalTrack::Pending {
                    publish_id: cur_publish_id,
                } = &live_kit.screen_track
                {
                    *cur_publish_id != publish_id
                } else {
                    true
                };

                match publication {
                    Ok((publication, stream)) => {
                        if canceled {
                            cx.spawn(async move |_, cx| {
                                participant.unpublish_track(publication.sid(), cx).await
                            })
                            .detach()
                        } else {
                            live_kit.screen_track = LocalTrack::Published {
                                track_publication: publication,
                                _stream: stream,
                            };
                            cx.emit(Event::LocalScreenShareStarted);
                            cx.notify();
                        }

                        Audio::play_sound(Sound::StartScreenshare, cx);
                        Ok(())
                    }
                    Err(error) => {
                        if canceled {
                            Ok(())
                        } else {
                            live_kit.screen_track = LocalTrack::None;
                            cx.notify();
                            Err(error)
                        }
                    }
                }
            })?
        })
    }

    #[cfg(target_os = "linux")]
    pub fn share_screen_wayland(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        log::info!("will screenshare on wayland");
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }
        if self.is_sharing_screen() {
            return Task::ready(Err(anyhow!("screen was already shared")));
        }

        let (participant, publish_id) = if let Some(live_kit) = self.live_kit.as_mut() {
            let publish_id = post_inc(&mut live_kit.next_publish_id);
            live_kit.screen_track = LocalTrack::Pending { publish_id };
            cx.notify();
            (live_kit.room.local_participant(), publish_id)
        } else {
            return Task::ready(Err(anyhow!("live-kit was not initialized")));
        };

        cx.spawn(async move |this, cx| {
            let publication = participant.publish_screenshare_track_wayland(cx).await;

            this.update(cx, |this, cx| {
                let live_kit = this
                    .live_kit
                    .as_mut()
                    .context("live-kit was not initialized")?;

                let canceled = if let LocalTrack::Pending {
                    publish_id: cur_publish_id,
                } = &live_kit.screen_track
                {
                    *cur_publish_id != publish_id
                } else {
                    true
                };

                match publication {
                    Ok((publication, stream, failure_rx)) => {
                        if canceled {
                            cx.spawn(async move |_, cx| {
                                participant.unpublish_track(publication.sid(), cx).await
                            })
                            .detach()
                        } else {
                            cx.spawn(async move |this, cx| {
                                if failure_rx.await.is_ok() {
                                    log::warn!("Wayland capture died, auto-unsharing screen");
                                    let _ =
                                        this.update(cx, |this, cx| this.unshare_screen(false, cx));
                                }
                            })
                            .detach();

                            live_kit.screen_track = LocalTrack::Published {
                                track_publication: publication,
                                _stream: stream,
                            };
                            cx.notify();
                        }

                        Audio::play_sound(Sound::StartScreenshare, cx);
                        Ok(())
                    }
                    Err(error) => {
                        if canceled {
                            Ok(())
                        } else {
                            live_kit.screen_track = LocalTrack::None;
                            cx.notify();
                            Err(error)
                        }
                    }
                }
            })?
        })
    }

    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(live_kit) = self.live_kit.as_mut() {
            // When unmuting, undeafen if the user was deafened before.
            let was_deafened = live_kit.deafened;
            if live_kit.muted_by_user
                || live_kit.deafened
                || matches!(live_kit.microphone_track, LocalTrack::None)
            {
                live_kit.muted_by_user = false;
                live_kit.deafened = false;
            } else {
                live_kit.muted_by_user = true;
            }
            let muted = live_kit.muted_by_user;
            let should_undeafen = was_deafened && !live_kit.deafened;

            if let Some(task) = self.set_mute(muted, cx) {
                task.detach_and_log_err(cx);
            }

            if should_undeafen {
                self.set_deafened(false, cx);
            }
        }
    }

    pub fn toggle_deafen(&mut self, cx: &mut Context<Self>) {
        if let Some(live_kit) = self.live_kit.as_mut() {
            // When deafening, mute the microphone if it was not already muted.
            // When un-deafening, unmute the microphone, unless it was explicitly muted.
            let deafened = !live_kit.deafened;
            live_kit.deafened = deafened;
            let should_change_mute = !live_kit.muted_by_user;

            self.set_deafened(deafened, cx);

            if should_change_mute && let Some(task) = self.set_mute(deafened, cx) {
                task.detach_and_log_err(cx);
            }
        }
    }

    pub fn unshare_screen(&mut self, play_sound: bool, cx: &mut Context<Self>) -> Result<()> {
        anyhow::ensure!(!self.status.is_offline(), "room is offline");

        let live_kit = self
            .live_kit
            .as_mut()
            .context("live-kit was not initialized")?;
        match mem::take(&mut live_kit.screen_track) {
            LocalTrack::None => anyhow::bail!("screen was not shared"),
            LocalTrack::Pending { .. } => {
                cx.notify();
                Ok(())
            }
            LocalTrack::Published {
                track_publication, ..
            } => {
                {
                    let local_participant = live_kit.room.local_participant();
                    let sid = track_publication.sid();
                    cx.spawn(async move |_, cx| local_participant.unpublish_track(sid, cx).await)
                        .detach_and_log_err(cx);
                    cx.emit(Event::LocalScreenShareStopped);
                    cx.notify();
                }

                if play_sound {
                    Audio::play_sound(Sound::StopScreenshare, cx);
                }

                Ok(())
            }
        }
    }

    fn set_deafened(&mut self, deafened: bool, cx: &mut Context<Self>) -> Option<()> {
        {
            let live_kit = self.live_kit.as_mut()?;
            cx.notify();
            for (_, participant) in live_kit.room.remote_participants() {
                for (_, publication) in participant.track_publications() {
                    if publication.is_audio() {
                        publication.set_enabled(!deafened, cx);
                    }
                }
            }
        }

        None
    }

    fn set_mute(&mut self, should_mute: bool, cx: &mut Context<Room>) -> Option<Task<Result<()>>> {
        let live_kit = self.live_kit.as_mut()?;
        cx.notify();

        if should_mute {
            Audio::play_sound(Sound::Mute, cx);
        } else {
            Audio::play_sound(Sound::Unmute, cx);
        }

        match &mut live_kit.microphone_track {
            LocalTrack::None => {
                if should_mute {
                    None
                } else {
                    Some(self.share_microphone(cx))
                }
            }
            LocalTrack::Pending { .. } => None,
            LocalTrack::Published {
                track_publication, ..
            } => {
                let guard = Tokio::handle(cx);
                if should_mute {
                    track_publication.mute(cx)
                } else {
                    track_publication.unmute(cx)
                }
                drop(guard);

                None
            }
        }
    }
}
