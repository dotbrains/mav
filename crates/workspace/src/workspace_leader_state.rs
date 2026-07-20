use super::*;

impl Workspace {
    pub(crate) fn handle_agent_location_changed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(follower_state) = self.follower_states.get_mut(&CollaboratorId::Agent) else {
            return;
        };

        if let Some(agent_location) = self.project.read(cx).agent_location() {
            let buffer_entity_id = agent_location.buffer.entity_id();
            let view_id = ViewId {
                creator: CollaboratorId::Agent,
                id: buffer_entity_id.as_u64(),
            };
            follower_state.active_view_id = Some(view_id);

            let item = match follower_state.items_by_leader_view_id.entry(view_id) {
                hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
                hash_map::Entry::Vacant(entry) => {
                    let existing_view =
                        follower_state
                            .center_pane
                            .read(cx)
                            .items()
                            .find_map(|item| {
                                let item = item.to_followable_item_handle(cx)?;
                                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                                    && item.project_item_model_ids(cx).as_slice()
                                        == [buffer_entity_id]
                                {
                                    Some(item)
                                } else {
                                    None
                                }
                            });
                    let view = existing_view.or_else(|| {
                        agent_location.buffer.upgrade().and_then(|buffer| {
                            cx.update_default_global(|registry: &mut ProjectItemRegistry, cx| {
                                registry.build_item(buffer, self.project.clone(), None, window, cx)
                            })?
                            .to_followable_item_handle(cx)
                        })
                    });

                    view.map(|view| {
                        entry.insert(FollowerView {
                            view,
                            location: None,
                        })
                    })
                }
            };

            if let Some(item) = item {
                item.view
                    .set_leader_id(Some(CollaboratorId::Agent), window, cx);
                item.view
                    .update_agent_location(agent_location.position, window, cx);
            }
        } else {
            follower_state.active_view_id = None;
        }

        self.leader_updated(CollaboratorId::Agent, window, cx);
    }

    pub fn update_active_view_for_followers(&mut self, window: &mut Window, cx: &mut App) {
        let mut is_project_item = true;
        let mut update = proto::UpdateActiveView::default();
        if window.is_window_active() {
            let (active_item, panel_id) = self.active_item_for_followers(window, cx);

            if let Some(item) = active_item
                && item.item_focus_handle(cx).contains_focused(window, cx)
            {
                let leader_id = self
                    .pane_for(&*item)
                    .and_then(|pane| self.leader_for_pane(&pane));
                let leader_peer_id = match leader_id {
                    Some(CollaboratorId::PeerId(peer_id)) => Some(peer_id),
                    Some(CollaboratorId::Agent) | None => None,
                };

                if let Some(item) = item.to_followable_item_handle(cx) {
                    let id = item
                        .remote_id(&self.app_state.client, window, cx)
                        .map(|id| id.to_proto());

                    if let Some(id) = id
                        && let Some(variant) = item.to_state_proto(window, cx)
                    {
                        let view = Some(proto::View {
                            id,
                            leader_id: leader_peer_id,
                            variant: Some(variant),
                            panel_id: panel_id.map(|id| id as i32),
                        });

                        is_project_item = item.is_project_item(window, cx);
                        update = proto::UpdateActiveView { view };
                    };
                }
            }
        }

        let active_view_id = update.view.as_ref().and_then(|view| view.id.as_ref());
        if active_view_id != self.last_active_view_id.as_ref() {
            self.last_active_view_id = active_view_id.cloned();
            self.update_followers(
                is_project_item,
                proto::update_followers::Variant::UpdateActiveView(update),
                window,
                cx,
            );
        }
    }

    pub(crate) fn active_item_for_followers(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> (Option<Box<dyn ItemHandle>>, Option<proto::PanelId>) {
        let mut active_item = None;
        let mut panel_id = None;
        for dock in self.all_docks() {
            if dock.focus_handle(cx).contains_focused(window, cx)
                && let Some(panel) = dock.read(cx).active_panel()
                && let Some(pane) = panel.pane(cx)
                && let Some(item) = pane.read(cx).active_item()
            {
                active_item = Some(item);
                panel_id = panel.remote_id();
                break;
            }
        }

        if active_item.is_none() {
            active_item = self.active_pane().read(cx).active_item();
        }
        (active_item, panel_id)
    }

    pub(crate) fn update_followers(
        &self,
        project_only: bool,
        update: proto::update_followers::Variant,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<()> {
        // If this update only applies to for followers in the current project,
        // then skip it unless this project is shared. If it applies to all
        // followers, regardless of project, then set `project_id` to none,
        // indicating that it goes to all followers.
        let project_id = if project_only {
            Some(self.project.read(cx).remote_id()?)
        } else {
            None
        };
        self.app_state().workspace_store.update(cx, |store, cx| {
            store.update_followers(project_id, update, cx)
        })
    }

    pub fn leader_for_pane(&self, pane: &Entity<Pane>) -> Option<CollaboratorId> {
        self.follower_states.iter().find_map(|(leader_id, state)| {
            if state.center_pane == *pane || state.dock_pane.as_ref() == Some(pane) {
                Some(*leader_id)
            } else {
                None
            }
        })
    }

    pub(crate) fn leader_updated(
        &mut self,
        leader_id: impl Into<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Box<dyn ItemHandle>> {
        cx.notify();

        let leader_id = leader_id.into();
        let (panel_id, item) = match leader_id {
            CollaboratorId::PeerId(peer_id) => self.active_item_for_peer(peer_id, window, cx)?,
            CollaboratorId::Agent => (None, self.active_item_for_agent()?),
        };

        let state = self.follower_states.get(&leader_id)?;
        let mut transfer_focus = state.center_pane.read(cx).has_focus(window, cx);
        let pane;
        if let Some(panel_id) = panel_id {
            pane = self
                .activate_panel_for_proto_id(panel_id, window, cx)?
                .pane(cx)?;
            let state = self.follower_states.get_mut(&leader_id)?;
            state.dock_pane = Some(pane.clone());
        } else {
            pane = state.center_pane.clone();
            let state = self.follower_states.get_mut(&leader_id)?;
            if let Some(dock_pane) = state.dock_pane.take() {
                transfer_focus |= dock_pane.focus_handle(cx).contains_focused(window, cx);
            }
        }

        pane.update(cx, |pane, cx| {
            let focus_active_item = pane.has_focus(window, cx) || transfer_focus;
            if let Some(index) = pane.index_for_item(item.as_ref()) {
                pane.activate_item(index, false, false, window, cx);
            } else {
                pane.add_item(item.boxed_clone(), false, false, None, window, cx)
            }

            if focus_active_item {
                pane.focus_active_item(window, cx)
            }
        });

        Some(item)
    }

    pub(crate) fn active_item_for_agent(&self) -> Option<Box<dyn ItemHandle>> {
        let state = self.follower_states.get(&CollaboratorId::Agent)?;
        let active_view_id = state.active_view_id?;
        Some(
            state
                .items_by_leader_view_id
                .get(&active_view_id)?
                .view
                .boxed_clone(),
        )
    }

    pub(crate) fn active_item_for_peer(
        &self,
        peer_id: PeerId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Option<PanelId>, Box<dyn ItemHandle>)> {
        let call = self.active_call()?;
        let participant = call.remote_participant_for_peer_id(peer_id, cx)?;
        let leader_in_this_app;
        let leader_in_this_project;
        match participant.location {
            ParticipantLocation::SharedProject { project_id } => {
                leader_in_this_app = true;
                leader_in_this_project = Some(project_id) == self.project.read(cx).remote_id();
            }
            ParticipantLocation::UnsharedProject => {
                leader_in_this_app = true;
                leader_in_this_project = false;
            }
            ParticipantLocation::External => {
                leader_in_this_app = false;
                leader_in_this_project = false;
            }
        };
        let state = self.follower_states.get(&peer_id.into())?;
        let mut item_to_activate = None;
        if let (Some(active_view_id), true) = (state.active_view_id, leader_in_this_app) {
            if let Some(item) = state.items_by_leader_view_id.get(&active_view_id)
                && (leader_in_this_project || !item.view.is_project_item(window, cx))
            {
                item_to_activate = Some((item.location, item.view.boxed_clone()));
            }
        } else if let Some(shared_screen) =
            self.shared_screen_for_peer(peer_id, &state.center_pane, window, cx)
        {
            item_to_activate = Some((None, Box::new(shared_screen)));
        }
        item_to_activate
    }

    pub(crate) fn shared_screen_for_peer(
        &self,
        peer_id: PeerId,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<SharedScreen>> {
        self.active_call()?
            .create_shared_screen(peer_id, pane, window, cx)
    }
}
