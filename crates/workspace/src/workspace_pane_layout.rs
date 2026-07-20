use super::*;

impl Workspace {
    pub(crate) fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        self.add_pane_with_kind(PaneKind::Tabs, true, window, cx)
    }

    pub(crate) fn add_pane_with_kind(
        &mut self,
        pane_kind: PaneKind,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let pane = cx.new(|cx| {
            let mut pane = Pane::new(
                self.weak_handle(),
                self.project.clone(),
                self.pane_history_timestamp.clone(),
                None,
                NewFile.boxed_clone(),
                pane_kind.is_tabbed(),
                window,
                cx,
            );
            pane.set_can_split(Some(Arc::new(|_, _, _, _| true)));
            match pane_kind {
                PaneKind::Tabs => {}
                PaneKind::Project => configure_project_pane(&mut pane, cx),
                PaneKind::Agent => configure_agent_pane(&mut pane, cx),
            }
            pane
        });
        cx.subscribe_in(&pane, window, Self::handle_pane_event)
            .detach();
        self.panes.push(pane.clone());

        if focus {
            window.focus(&pane.focus_handle(cx), cx);
        }

        cx.emit(Event::PaneAdded(pane.clone()));
        pane
    }

    pub(crate) fn last_tabbed_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        self.last_active_center_pane
            .as_ref()
            .and_then(|pane| pane.upgrade())
            .filter(|pane| {
                pane.read(cx).is_tabbed()
                    && pane.read(cx).is_visible()
                    && self.pane_is_in_center(pane)
            })
            .or_else(|| {
                self.panes
                    .iter()
                    .find(|pane| {
                        pane.read(cx).is_tabbed()
                            && pane.read(cx).is_visible()
                            && self.pane_is_in_center(pane)
                    })
                    .cloned()
            })
    }

    pub(crate) fn ensure_tabbed_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        if let Some(pane) = self.last_tabbed_pane(cx) {
            return pane;
        }

        if let Some(pane) = self
            .center
            .panes()
            .into_iter()
            .find(|pane| pane.read(cx).pane_kind() == PaneKind::Tabs)
            .cloned()
        {
            pane.update(cx, |pane, cx| pane.set_visible(true, cx));
            self.center.mark_positions(cx);
            return pane;
        }

        let split_target = self
            .panel_pane_for_kind(PaneKind::Project, cx)
            .or_else(|| self.panel_pane_for_kind(PaneKind::Agent, cx))
            .unwrap_or_else(|| self.center.first_pane());
        let split_direction = match split_target.read(cx).pane_kind() {
            PaneKind::Project => SplitDirection::Left,
            PaneKind::Agent | PaneKind::Tabs => SplitDirection::Right,
        };

        let pane = self.add_pane_with_kind(PaneKind::Tabs, false, window, cx);
        self.center.split(&split_target, &pane, split_direction, cx);
        cx.notify();
        pane
    }

    pub(crate) fn ensure_visible_center_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tabbed_pane = self
            .last_tabbed_pane(cx)
            .or_else(|| {
                let pane = self
                    .center
                    .panes()
                    .into_iter()
                    .find(|pane| pane.read(cx).pane_kind() == PaneKind::Tabs)
                    .cloned()?;
                pane.update(cx, |pane, cx| pane.set_visible(true, cx));
                Some(pane)
            })
            .unwrap_or_else(|| self.ensure_tabbed_pane(window, cx));

        if !self.pane_is_in_center(&self.active_pane) || !self.active_pane.read(cx).is_visible() {
            self.set_active_pane(&tabbed_pane, window, cx);
            tabbed_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        }

        self.center.mark_positions(cx);
        cx.notify();
    }

    pub(crate) fn existing_tabbed_pane(
        &self,
        pane: Option<WeakEntity<Pane>>,
        cx: &App,
    ) -> Option<Entity<Pane>> {
        pane.and_then(|pane| pane.upgrade())
            .filter(|pane| pane.read(cx).is_tabbed() && pane.read(cx).is_visible())
            .or_else(|| self.last_tabbed_pane(cx))
    }

    pub(crate) fn last_focusable_center_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        self.last_active_center_pane
            .as_ref()
            .and_then(|pane| pane.upgrade())
            .filter(|pane| {
                pane.read(cx).is_visible()
                    && pane.read(cx).active_item().is_some()
                    && self.pane_is_in_center(pane)
            })
            .or_else(|| {
                self.center
                    .panes()
                    .into_iter()
                    .find(|pane| {
                        pane.read(cx).is_visible() && pane.read(cx).active_item().is_some()
                    })
                    .cloned()
            })
    }

    pub(crate) fn pane_is_in_center(&self, pane: &Entity<Pane>) -> bool {
        self.center
            .panes()
            .into_iter()
            .any(|center_pane| center_pane == pane)
    }

    pub fn panel_pane_for_kind(&self, pane_kind: PaneKind, cx: &App) -> Option<Entity<Pane>> {
        self.center
            .panes()
            .into_iter()
            .find(|pane| pane.read(cx).pane_kind() == pane_kind)
            .cloned()
    }

    pub fn panel_pane_visible(&self, pane_kind: PaneKind, cx: &App) -> bool {
        self.panel_pane_for_kind(pane_kind, cx)
            .is_some_and(|pane| pane.read(cx).is_visible())
    }

    pub fn panel_pane_visible_except(
        &self,
        pane_kind: PaneKind,
        excluded_pane: &Entity<Pane>,
        cx: &App,
    ) -> bool {
        self.center
            .panes()
            .into_iter()
            .find(|pane| *pane != excluded_pane && pane.read(cx).pane_kind() == pane_kind)
            .is_some_and(|pane| pane.read(cx).is_visible())
    }

    pub fn panel_pane_should_reserve_traffic_light_space(
        &self,
        pane_kind: PaneKind,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.panel_pane_for_kind(pane_kind, cx)
            .is_some_and(|pane| pane.read(cx).should_reserve_traffic_light_space(window, cx))
    }

    pub fn toggle_panel_pane_visibility(
        &mut self,
        pane_kind: PaneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = if let Some(pane) = self.panel_pane_for_kind(pane_kind, cx) {
            pane
        } else {
            match pane_kind {
                PaneKind::Project => self.ensure_panel_pane(PanelPaneKind::Project, window, cx),
                PaneKind::Agent => self.ensure_panel_pane(PanelPaneKind::Agent, window, cx),
                PaneKind::Tabs => self.ensure_tabbed_pane(window, cx),
            }
        };

        let visible = pane.read(cx).is_visible();
        let fallback_pane = if visible {
            self.last_tabbed_pane(cx).or_else(|| {
                self.center
                    .panes()
                    .into_iter()
                    .find(|candidate| *candidate != &pane && candidate.read(cx).is_visible())
                    .cloned()
            })
        } else {
            None
        };
        let fallback_pane = if visible && fallback_pane.is_none() {
            Some(self.ensure_tabbed_pane(window, cx))
        } else {
            fallback_pane
        };

        pane.update(cx, |pane, cx| pane.set_visible(!visible, cx));
        self.center.mark_positions(cx);

        if visible {
            if self.active_pane == pane
                && let Some(fallback_pane) = fallback_pane
            {
                self.set_active_pane(&fallback_pane, window, cx);
                fallback_pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }
        } else {
            self.set_active_pane(&pane, window, cx);
            pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        }

        self.serialize_workspace(window, cx);
        cx.notify();
    }

    fn ensure_panel_pane(
        &mut self,
        panel_pane_kind: PanelPaneKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let pane_kind = panel_pane_kind.pane_kind();
        let existing_pane = self
            .panes
            .iter()
            .find(|pane| pane.read(cx).pane_kind() == pane_kind && self.pane_is_in_center(pane))
            .cloned();
        if let Some(pane) = existing_pane {
            return pane;
        }

        let split_direction = match panel_pane_kind {
            PanelPaneKind::Project => SplitDirection::Right,
            PanelPaneKind::Agent => SplitDirection::Left,
        };
        let split_target = self
            .last_tabbed_pane(cx)
            .unwrap_or_else(|| self.center.first_pane());

        let pane = self.add_pane_with_kind(pane_kind, false, window, cx);
        self.center.split(&split_target, &pane, split_direction, cx);
        cx.notify();
        pane
    }

    pub(crate) fn add_panel_to_panel_pane<T: Panel>(
        &mut self,
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel_handle: Arc<dyn PanelHandle> = Arc::new(panel.clone());
        let activate = panel.read(cx).starts_open(window, cx);
        self.add_panel_handle_to_panel_pane(panel_handle, activate, window, cx);
    }

    fn add_panel_handle_to_panel_pane(
        &mut self,
        panel_handle: Arc<dyn PanelHandle>,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel_pane_kind) = PanelPaneKind::for_panel_key(panel_handle.panel_key()) else {
            return;
        };

        let panel_id = panel_handle.panel_id();
        let pane = self.ensure_panel_pane(panel_pane_kind, window, cx);
        pane.update(cx, |pane, cx| {
            let existing_index = pane.items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                (item.read(cx).panel_id() == panel_id).then_some(ix)
            });
            if let Some(existing_index) = existing_index {
                if activate {
                    pane.activate_item(existing_index, true, false, window, cx);
                }
                return;
            }

            let panel_priority = panel_handle.activation_priority(cx);
            let destination_index = pane.items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let existing_priority = item.read(cx).panel().activation_priority(cx);
                (existing_priority > panel_priority).then_some(ix)
            });
            let activate = pane.items_len() == 0 || activate;
            let panel_item = cx.new(|_| PanelItem::new(panel_handle.clone()));
            pane.add_item_inner(
                Box::new(panel_item),
                false,
                false,
                activate,
                destination_index,
                window,
                cx,
            );
        });
    }

    pub(crate) fn sync_panel_panes_from_docks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.enforce_singleton_panel_panes(window, cx);

        let mut panels = Vec::new();
        let mut active_panel_ids_by_pane_kind = HashMap::default();
        for dock in self.all_docks() {
            let dock = dock.read(cx);
            if let Some(active_panel) = dock.active_panel()
                && let Some(panel_pane_kind) =
                    PanelPaneKind::for_panel_key(active_panel.panel_key())
            {
                active_panel_ids_by_pane_kind
                    .entry(panel_pane_kind)
                    .or_insert_with(|| active_panel.panel_id());
            }

            for panel in dock.panel_handles() {
                panels.push(panel);
            }
        }

        for panel in panels {
            let activate = PanelPaneKind::for_panel_key(panel.panel_key())
                .and_then(|panel_pane_kind| active_panel_ids_by_pane_kind.get(&panel_pane_kind))
                .is_some_and(|active_panel_id| *active_panel_id == panel.panel_id());
            self.add_panel_handle_to_panel_pane(panel, activate, window, cx);
        }

        self.enforce_singleton_panel_panes(window, cx);
    }

    fn enforce_singleton_panel_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for pane_kind in [PaneKind::Agent, PaneKind::Project] {
            let mut panes = self
                .center
                .panes()
                .into_iter()
                .filter(|pane| pane.read(cx).pane_kind() == pane_kind)
                .cloned()
                .collect::<Vec<_>>();
            if panes.len() <= 1 {
                continue;
            }

            let keep_pane = panes.remove(0);
            for duplicate_pane in panes {
                self.merge_panel_pane_items(&duplicate_pane, &keep_pane, window, cx);
                self.remove_pane(duplicate_pane, Some(keep_pane.clone()), window, cx);
            }
        }
    }

    fn merge_panel_pane_items(
        &mut self,
        source_pane: &Entity<Pane>,
        target_pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_panel_id = source_pane
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<PanelItem>())
            .map(|panel_item| panel_item.read(cx).panel_id());
        let panel_items = source_pane
            .read(cx)
            .items()
            .filter_map(|item| {
                let panel_item = item.downcast::<PanelItem>()?;
                Some((panel_item.read(cx).panel_id(), item.clone()))
            })
            .collect::<Vec<_>>();

        target_pane.update(cx, |target_pane, cx| {
            for (panel_id, item) in panel_items {
                let existing_index =
                    target_pane
                        .items()
                        .enumerate()
                        .find_map(|(index, existing_item)| {
                            existing_item
                                .downcast::<PanelItem>()
                                .is_some_and(|panel_item| {
                                    panel_item.read(cx).panel_id() == panel_id
                                })
                                .then_some(index)
                        });
                if let Some(existing_index) = existing_index {
                    if active_panel_id == Some(panel_id) {
                        target_pane.activate_item(existing_index, true, false, window, cx);
                    }
                    continue;
                }

                let activate = target_pane.items_len() == 0 || active_panel_id == Some(panel_id);
                target_pane.add_item_inner(item, false, false, activate, None, window, cx);
            }
        });
    }
}
