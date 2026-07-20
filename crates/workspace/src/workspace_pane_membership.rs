use super::*;

impl Workspace {
    pub fn unfollow_in_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Option<CollaboratorId> {
        let leader_id = self.leader_for_pane(pane)?;
        self.unfollow(leader_id, window, cx);
        Some(leader_id)
    }

    pub fn split_pane(
        &mut self,
        pane_to_split: Entity<Pane>,
        split_direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let new_pane = self.add_pane(window, cx);
        self.center
            .split(&pane_to_split, &new_pane, split_direction, cx);
        cx.notify();
        new_pane
    }

    pub(crate) fn split_size_hint_for_inserted_pane(
        &mut self,
        pane: &Entity<Pane>,
        target_pane: &Entity<Pane>,
        split_direction: SplitDirection,
        cx: &mut Context<Self>,
    ) -> Option<SplitSizeHint> {
        if pane.read(cx).pane_kind() != PaneKind::Project {
            return None;
        }

        if let Some(width) = self.center.horizontal_size_for_pane(pane) {
            pane.update(cx, |pane, _| {
                pane.remember_horizontal_split_size(width);
            });
        }

        if split_direction.axis() != Axis::Horizontal {
            return None;
        }

        let inserted_size = pane.read(cx).preferred_horizontal_split_size()?;
        let available_size = self
            .center
            .horizontal_size_for_pane(target_pane)
            .map(|target_size| {
                target_size
                    + self
                        .center
                        .horizontal_size_for_pane(pane)
                        .unwrap_or(Pixels::ZERO)
            });

        Some(match available_size {
            Some(available_size) => {
                SplitSizeHint::inserted_size_in_available_space(inserted_size, available_size)
            }
            None => SplitSizeHint::inserted_size(inserted_size),
        })
    }

    pub fn move_pane_to_pane(
        &mut self,
        pane_to_move: Entity<Pane>,
        target_pane: Entity<Pane>,
        split_direction: Option<SplitDirection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pane_to_move == target_pane {
            return;
        }

        if let Some(split_direction) = split_direction {
            let size_hint = self.split_size_hint_for_inserted_pane(
                &pane_to_move,
                &target_pane,
                split_direction,
                cx,
            );
            if self.center.remove(&pane_to_move, cx).unwrap_or(false) {
                self.center.split_with_size_hint(
                    &target_pane,
                    &pane_to_move,
                    split_direction,
                    size_hint,
                    cx,
                );
            }
        } else {
            self.center.swap(&pane_to_move, &target_pane, cx);
        }

        self.set_active_pane(&pane_to_move, window, cx);
        pane_to_move.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        self.serialize_workspace(window, cx);
        cx.notify();
    }

    pub fn split_and_move(
        &mut self,
        pane: Entity<Pane>,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = pane.update(cx, |pane, cx| pane.take_active_item(window, cx)) else {
            return;
        };
        let new_pane = self.add_pane(window, cx);
        new_pane.update(cx, |pane, cx| {
            pane.add_item(item, true, true, None, window, cx)
        });
        self.center.split(&pane, &new_pane, direction, cx);
        cx.notify();
    }

    pub fn split_and_clone(
        &mut self,
        pane: Entity<Pane>,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Pane>>> {
        let Some(item) = pane.read(cx).active_item() else {
            return Task::ready(None);
        };
        if !item.can_split(cx) {
            return Task::ready(None);
        }
        let task = item.clone_on_split(self.database_id(), window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Some(clone) = task.await {
                this.update_in(cx, |this, window, cx| {
                    let new_pane = this.add_pane(window, cx);
                    let nav_history = pane.read(cx).fork_nav_history();
                    new_pane.update(cx, |pane, cx| {
                        pane.set_nav_history(nav_history, cx);
                        pane.add_item(clone, true, true, None, window, cx)
                    });
                    this.center.split(&pane, &new_pane, direction, cx);
                    cx.notify();
                    new_pane
                })
                .ok()
            } else {
                None
            }
        })
    }

    pub fn join_all_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_item = self.active_pane.read(cx).active_item();
        for pane in &self.panes {
            join_pane_into_active(&self.active_pane, pane, window, cx);
        }
        if let Some(active_item) = active_item {
            self.activate_item(active_item.as_ref(), true, true, window, cx);
        }
        cx.notify();
    }

    pub fn join_pane_into_next(
        &mut self,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_pane = self
            .find_pane_in_direction(SplitDirection::Right, cx)
            .or_else(|| self.find_pane_in_direction(SplitDirection::Down, cx))
            .or_else(|| self.find_pane_in_direction(SplitDirection::Left, cx))
            .or_else(|| self.find_pane_in_direction(SplitDirection::Up, cx));
        let Some(next_pane) = next_pane else {
            return;
        };
        move_all_items(&pane, &next_pane, window, cx);
        cx.notify();
    }

    pub(crate) fn remove_pane(
        &mut self,
        pane: Entity<Pane>,
        focus_on: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.center.remove(&pane, cx).unwrap() {
            self.force_remove_pane(&pane, &focus_on, window, cx);
            self.unfollow_in_pane(&pane, window, cx);
            self.last_leaders_by_pane.remove(&pane.downgrade());
            for removed_item in pane.read(cx).items() {
                self.panes_by_item.remove(&removed_item.item_id());
            }

            cx.notify();
        } else {
            self.active_item_path_changed(true, window, cx);
        }
        cx.emit(Event::PaneRemoved);
    }

    pub fn panes_mut(&mut self) -> &mut [Entity<Pane>] {
        &mut self.panes
    }

    pub fn panes(&self) -> &[Entity<Pane>] {
        &self.panes
    }

    pub fn active_pane(&self) -> &Entity<Pane> {
        &self.active_pane
    }

    pub fn focused_pane(&self, window: &Window, cx: &App) -> Entity<Pane> {
        for dock in self.all_docks() {
            if dock.focus_handle(cx).contains_focused(window, cx)
                && let Some(pane) = dock
                    .read(cx)
                    .active_panel()
                    .and_then(|panel| panel.pane(cx))
            {
                return pane;
            }
        }
        self.active_pane().clone()
    }

    pub fn adjacent_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        self.find_pane_in_direction(SplitDirection::Right, cx)
            .unwrap_or_else(|| {
                self.split_pane(self.active_pane.clone(), SplitDirection::Right, window, cx)
            })
    }

    pub fn pane_for(&self, handle: &dyn ItemHandle) -> Option<Entity<Pane>> {
        self.pane_for_item_id(handle.item_id())
    }

    pub fn pane_for_item_id(&self, item_id: EntityId) -> Option<Entity<Pane>> {
        let weak_pane = self.panes_by_item.get(&item_id)?;
        weak_pane.upgrade()
    }

    pub fn pane_for_entity_id(&self, entity_id: EntityId) -> Option<Entity<Pane>> {
        self.panes
            .iter()
            .find(|pane| pane.entity_id() == entity_id)
            .cloned()
    }
}
