use super::*;

impl Workspace {
    pub(crate) fn active_view_for_follower(
        &self,
        follower_project_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<proto::View> {
        let (item, panel_id) = self.active_item_for_followers(window, cx);
        let item = item?;
        let leader_id = self
            .pane_for(&*item)
            .and_then(|pane| self.leader_for_pane(&pane));
        let leader_peer_id = match leader_id {
            Some(CollaboratorId::PeerId(peer_id)) => Some(peer_id),
            Some(CollaboratorId::Agent) | None => None,
        };

        let item_handle = item.to_followable_item_handle(cx)?;
        let id = item_handle.remote_id(&self.app_state.client, window, cx)?;
        let variant = item_handle.to_state_proto(window, cx)?;

        if item_handle.is_project_item(window, cx)
            && (follower_project_id.is_none()
                || follower_project_id != self.project.read(cx).remote_id())
        {
            return None;
        }

        Some(proto::View {
            id: id.to_proto(),
            leader_id: leader_peer_id,
            variant: Some(variant),
            panel_id: panel_id.map(|id| id as i32),
        })
    }

    pub(crate) fn handle_follow(
        &mut self,
        follower_project_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> proto::FollowResponse {
        let active_view = self.active_view_for_follower(follower_project_id, window, cx);

        cx.notify();
        proto::FollowResponse {
            views: active_view.iter().cloned().collect(),
            active_view,
        }
    }

    pub(crate) fn handle_update_followers(
        &mut self,
        leader_id: PeerId,
        message: proto::UpdateFollowers,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.leader_updates_tx
            .unbounded_send((leader_id, message))
            .ok();
    }

    pub(crate) async fn process_leader_update(
        this: &WeakEntity<Self>,
        leader_id: PeerId,
        update: proto::UpdateFollowers,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        match update.variant.context("invalid update")? {
            proto::update_followers::Variant::CreateView(view) => {
                let view_id = ViewId::from_proto(view.id.clone().context("invalid view id")?)?;
                let should_add_view = this.update(cx, |this, _| {
                    if let Some(state) = this.follower_states.get_mut(&leader_id.into()) {
                        anyhow::Ok(!state.items_by_leader_view_id.contains_key(&view_id))
                    } else {
                        anyhow::Ok(false)
                    }
                })??;

                if should_add_view {
                    Self::add_view_from_leader(this.clone(), leader_id, &view, cx).await?
                }
            }
            proto::update_followers::Variant::UpdateActiveView(update_active_view) => {
                let should_add_view = this.update(cx, |this, _| {
                    if let Some(state) = this.follower_states.get_mut(&leader_id.into()) {
                        state.active_view_id = update_active_view
                            .view
                            .as_ref()
                            .and_then(|view| ViewId::from_proto(view.id.clone()?).ok());

                        if state.active_view_id.is_some_and(|view_id| {
                            !state.items_by_leader_view_id.contains_key(&view_id)
                        }) {
                            anyhow::Ok(true)
                        } else {
                            anyhow::Ok(false)
                        }
                    } else {
                        anyhow::Ok(false)
                    }
                })??;

                if should_add_view && let Some(view) = update_active_view.view {
                    Self::add_view_from_leader(this.clone(), leader_id, &view, cx).await?
                }
            }
            proto::update_followers::Variant::UpdateView(update_view) => {
                let variant = update_view.variant.context("missing update view variant")?;
                let id = update_view.id.context("missing update view id")?;
                let mut tasks = Vec::new();
                this.update_in(cx, |this, window, cx| {
                    let project = this.project.clone();
                    if let Some(state) = this.follower_states.get(&leader_id.into()) {
                        let view_id = ViewId::from_proto(id.clone())?;
                        if let Some(item) = state.items_by_leader_view_id.get(&view_id) {
                            tasks.push(item.view.apply_update_proto(
                                &project,
                                variant.clone(),
                                window,
                                cx,
                            ));
                        }
                    }
                    anyhow::Ok(())
                })??;
                try_join_all(tasks).await.log_err();
            }
        }
        this.update_in(cx, |this, window, cx| {
            this.leader_updated(leader_id, window, cx)
        })?;
        Ok(())
    }

    pub(crate) async fn add_view_from_leader(
        this: WeakEntity<Self>,
        leader_id: PeerId,
        view: &proto::View,
        cx: &mut AsyncWindowContext,
    ) -> Result<()> {
        let this = this.upgrade().context("workspace dropped")?;

        let Some(id) = view.id.clone() else {
            anyhow::bail!("no id for view");
        };
        let id = ViewId::from_proto(id)?;
        let panel_id = view.panel_id.and_then(proto::PanelId::from_i32);

        let pane = this.update(cx, |this, _cx| {
            let state = this
                .follower_states
                .get(&leader_id.into())
                .context("stopped following")?;
            anyhow::Ok(state.pane().clone())
        })?;
        let existing_item = pane.update_in(cx, |pane, window, cx| {
            let client = this.read(cx).client().clone();
            pane.items().find_map(|item| {
                let item = item.to_followable_item_handle(cx)?;
                if item.remote_id(&client, window, cx) == Some(id) {
                    Some(item)
                } else {
                    None
                }
            })
        })?;
        let item = if let Some(existing_item) = existing_item {
            existing_item
        } else {
            let variant = view.variant.clone();
            anyhow::ensure!(variant.is_some(), "missing view variant");

            let task = cx.update(|window, cx| {
                FollowableViewRegistry::from_state_proto(this.clone(), id, variant, window, cx)
            })?;

            let Some(task) = task else {
                anyhow::bail!(
                    "failed to construct view from leader (maybe from a different version of mav?)"
                );
            };

            let mut new_item = task.await?;
            pane.update_in(cx, |pane, window, cx| {
                let mut item_to_remove = None;
                for (ix, item) in pane.items().enumerate() {
                    if let Some(item) = item.to_followable_item_handle(cx) {
                        match new_item.dedup(item.as_ref(), window, cx) {
                            Some(item::Dedup::KeepExisting) => {
                                new_item =
                                    item.boxed_clone().to_followable_item_handle(cx).unwrap();
                                break;
                            }
                            Some(item::Dedup::ReplaceExisting) => {
                                item_to_remove = Some((ix, item.item_id()));
                                break;
                            }
                            None => {}
                        }
                    }
                }

                if let Some((ix, id)) = item_to_remove {
                    pane.remove_item(id, false, false, window, cx);
                    pane.add_item(new_item.boxed_clone(), false, false, Some(ix), window, cx);
                }
            })?;

            new_item
        };

        this.update_in(cx, |this, window, cx| {
            let state = this.follower_states.get_mut(&leader_id.into())?;
            item.set_leader_id(Some(leader_id.into()), window, cx);
            state.items_by_leader_view_id.insert(
                id,
                FollowerView {
                    view: item,
                    location: panel_id,
                },
            );

            Some(())
        })
        .context("no follower state")?;

        Ok(())
    }
}
