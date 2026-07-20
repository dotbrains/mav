use super::*;

impl Workspace {
    pub(crate) fn collaborator_left(
        &mut self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.follower_states.retain(|leader_id, state| {
            if *leader_id == CollaboratorId::PeerId(peer_id) {
                for item in state.items_by_leader_view_id.values() {
                    item.view.set_leader_id(None, window, cx);
                }
                false
            } else {
                true
            }
        });
        cx.notify();
    }

    pub fn start_following(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let leader_id = leader_id.into();
        let pane = self.active_pane().clone();

        self.last_leaders_by_pane
            .insert(pane.downgrade(), leader_id);
        self.unfollow(leader_id, window, cx);
        self.unfollow_in_pane(&pane, window, cx);
        self.auto_watch = AutoWatch::Off;
        self.follower_states.insert(
            leader_id,
            FollowerState {
                center_pane: pane.clone(),
                dock_pane: None,
                active_view_id: None,
                items_by_leader_view_id: Default::default(),
            },
        );
        cx.notify();

        match leader_id {
            CollaboratorId::PeerId(leader_peer_id) => {
                let room_id = self.active_call()?.room_id(cx)?;
                let project_id = self.project.read(cx).remote_id();
                let request = self.app_state.client.request(proto::Follow {
                    room_id,
                    project_id,
                    leader_id: Some(leader_peer_id),
                });

                Some(cx.spawn_in(window, async move |this, cx| {
                    let response = request.await?;
                    this.update(cx, |this, _| {
                        let state = this
                            .follower_states
                            .get_mut(&leader_id)
                            .context("following interrupted")?;
                        state.active_view_id = response
                            .active_view
                            .as_ref()
                            .and_then(|view| ViewId::from_proto(view.id.clone()?).ok());
                        anyhow::Ok(())
                    })??;
                    if let Some(view) = response.active_view {
                        Self::add_view_from_leader(this.clone(), leader_peer_id, &view, cx).await?;
                    }
                    this.update_in(cx, |this, window, cx| {
                        this.leader_updated(leader_id, window, cx)
                    })?;
                    Ok(())
                }))
            }
            CollaboratorId::Agent => {
                self.leader_updated(leader_id, window, cx)?;
                Some(Task::ready(Ok(())))
            }
        }
    }

    pub fn follow_next_collaborator(
        &mut self,
        _: &FollowNextCollaborator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collaborators = self.project.read(cx).collaborators();
        let next_leader_id = if let Some(leader_id) = self.leader_for_pane(&self.active_pane) {
            let mut collaborators = collaborators.keys().copied();
            for peer_id in collaborators.by_ref() {
                if CollaboratorId::PeerId(peer_id) == leader_id {
                    break;
                }
            }
            collaborators.next().map(CollaboratorId::PeerId)
        } else if let Some(last_leader_id) =
            self.last_leaders_by_pane.get(&self.active_pane.downgrade())
        {
            match last_leader_id {
                CollaboratorId::PeerId(peer_id) => {
                    if collaborators.contains_key(peer_id) {
                        Some(*last_leader_id)
                    } else {
                        None
                    }
                }
                CollaboratorId::Agent => Some(CollaboratorId::Agent),
            }
        } else {
            None
        };

        let pane = self.active_pane.clone();
        let Some(leader_id) = next_leader_id.or_else(|| {
            Some(CollaboratorId::PeerId(
                collaborators.keys().copied().next()?,
            ))
        }) else {
            return;
        };
        if self.unfollow_in_pane(&pane, window, cx) == Some(leader_id) {
            return;
        }
        if let Some(task) = self.start_following(leader_id, window, cx) {
            task.detach_and_log_err(cx)
        }
    }

    pub fn follow(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leader_id = leader_id.into();

        if let CollaboratorId::PeerId(peer_id) = leader_id {
            let Some(active_call) = GlobalAnyActiveCall::try_global(cx) else {
                return;
            };
            let Some(remote_participant) =
                active_call.0.remote_participant_for_peer_id(peer_id, cx)
            else {
                return;
            };

            let project = self.project.read(cx);

            let other_project_id = match remote_participant.location {
                ParticipantLocation::External => None,
                ParticipantLocation::UnsharedProject => None,
                ParticipantLocation::SharedProject { project_id } => {
                    if Some(project_id) == project.remote_id() {
                        None
                    } else {
                        Some(project_id)
                    }
                }
            };

            // if they are active in another project, follow there.
            if let Some(project_id) = other_project_id {
                let app_state = self.app_state.clone();
                crate::join_in_room_project(
                    project_id,
                    remote_participant.user.legacy_id,
                    app_state,
                    cx,
                )
                .detach_and_prompt_err(
                    "Failed to join project",
                    window,
                    cx,
                    |error, _, _| Some(format!("{error:#}")),
                );
            }
        }

        // if you're already following, find the right pane and focus it.
        if let Some(follower_state) = self.follower_states.get(&leader_id) {
            window.focus(&follower_state.pane().focus_handle(cx), cx);

            return;
        }

        // Otherwise, follow.
        if let Some(task) = self.start_following(leader_id, window, cx) {
            task.detach_and_log_err(cx)
        }
    }

    pub fn unfollow(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        cx.notify();

        let leader_id = leader_id.into();
        let state = self.follower_states.remove(&leader_id)?;
        for (_, item) in state.items_by_leader_view_id {
            item.view.set_leader_id(None, window, cx);
        }

        if let CollaboratorId::PeerId(leader_peer_id) = leader_id {
            let project_id = self.project.read(cx).remote_id();
            let room_id = self.active_call()?.room_id(cx)?;
            self.app_state
                .client
                .send(proto::Unfollow {
                    room_id,
                    project_id,
                    leader_id: Some(leader_peer_id),
                })
                .log_err();
        }

        Some(())
    }

    pub fn is_being_followed(&self, id: impl Into<CollaboratorId>) -> bool {
        self.follower_states.contains_key(&id.into())
    }
}
