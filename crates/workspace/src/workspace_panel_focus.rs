use super::*;

impl Workspace {
    pub fn is_dock_at_position_open(&self, position: DockPosition, cx: &mut Context<Self>) -> bool {
        self.dock_at_position(position).read(cx).is_open()
    }

    pub fn toggle_dock(
        &mut self,
        dock_side: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut focus_center = false;
        let mut reveal_dock = false;

        let other_is_zoomed = self.zoomed.is_some() && self.zoomed_position != Some(dock_side);
        let was_visible = self.is_dock_at_position_open(dock_side, cx) && !other_is_zoomed;

        if let Some(panel) = self.dock_at_position(dock_side).read(cx).active_panel() {
            telemetry::event!(
                "Panel Button Clicked",
                name = panel.persistent_name(),
                toggle_state = !was_visible
            );
        }
        let dock = self.dock_at_position(dock_side);
        dock.update(cx, |dock, cx| {
            dock.set_open(!was_visible, window, cx);

            if dock.active_panel().is_none() {
                let Some(panel_ix) = dock
                    .first_enabled_panel_idx(cx)
                    .log_with_level(log::Level::Info)
                else {
                    return;
                };
                dock.activate_panel(panel_ix, window, cx);
            }

            if let Some(active_panel) = dock.active_panel() {
                if was_visible {
                    if active_panel
                        .panel_focus_handle(cx)
                        .contains_focused(window, cx)
                    {
                        focus_center = true;
                    }
                } else {
                    let focus_handle = &active_panel.panel_focus_handle(cx);
                    window.focus(focus_handle, cx);
                    reveal_dock = true;
                }
            }
        });

        if reveal_dock {
            self.dismiss_zoomed_items_to_reveal(Some(dock_side), window, cx);
        }

        if focus_center {
            self.active_pane
                .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
        }

        cx.notify();
        self.serialize_workspace(window, cx);
    }

    pub(crate) fn active_dock(&self, window: &Window, cx: &Context<Self>) -> Option<&Entity<Dock>> {
        self.all_docks().into_iter().find(|&dock| {
            dock.read(cx).is_open() && dock.focus_handle(cx).contains_focused(window, cx)
        })
    }

    /// Transfer focus to the panel of the given type.
    pub fn focus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<T>> {
        if let Some(panel) = self.activate_panel_item::<T>(true, window, cx) {
            return panel.to_any().downcast().ok();
        }

        let panel = self.focus_or_unfocus_panel::<T>(window, cx, &mut |_, _, _| true)?;
        panel.to_any().downcast().ok()
    }

    /// Focus the panel of the given type if it isn't already focused. If it is
    /// already focused, then transfer focus back to the workspace center.
    /// When the `close_panel_on_toggle` setting is enabled, also closes the
    /// panel when transferring focus back to the center.
    pub fn toggle_panel_focus<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some((_, _, panel)) = self.panel_item_for::<T>(cx) {
            let did_focus_panel = !panel.panel_focus_handle(cx).contains_focused(window, cx);
            if did_focus_panel {
                self.activate_panel_item::<T>(true, window, cx);
            } else if let Some(pane) = self.last_tabbed_pane(cx) {
                pane.update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
            }

            telemetry::event!(
                "Panel Button Clicked",
                name = T::persistent_name(),
                toggle_state = did_focus_panel
            );

            return did_focus_panel;
        }

        let mut did_focus_panel = false;
        self.focus_or_unfocus_panel::<T>(window, cx, &mut |panel, window, cx| {
            did_focus_panel = !panel.panel_focus_handle(cx).contains_focused(window, cx);
            did_focus_panel
        });

        if !did_focus_panel && WorkspaceSettings::get_global(cx).close_panel_on_toggle {
            self.close_panel::<T>(window, cx);
        }

        telemetry::event!(
            "Panel Button Clicked",
            name = T::persistent_name(),
            toggle_state = did_focus_panel
        );

        did_focus_panel
    }

    pub fn focus_center_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self.active_item(cx) {
            item.item_focus_handle(cx).focus(window, cx);
        } else {
            log::error!("Could not find a focus target when switching focus to the center panes",);
        }
    }

    pub fn activate_panel_for_proto_id(
        &mut self,
        panel_id: PanelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        if let Some((pane, ix, panel)) = self.panel_item_for_proto_id(panel_id, cx) {
            pane.update(cx, |pane, cx| {
                pane.activate_item(ix, true, true, window, cx);
            });
            panel.panel_focus_handle(cx).focus(window, cx);
            cx.notify();
            self.serialize_workspace(window, cx);
            return Some(panel);
        }

        let mut panel = None;
        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_proto_id(panel_id) {
                panel = dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);
                    dock.set_open(true, window, cx);
                    dock.active_panel().cloned()
                });
                break;
            }
        }

        if panel.is_some() {
            cx.notify();
            self.serialize_workspace(window, cx);
        }

        panel
    }

    /// Focus or unfocus the given panel type, depending on the given callback.
    fn focus_or_unfocus_panel<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        should_focus: &mut dyn FnMut(&dyn PanelHandle, &mut Window, &mut Context<Dock>) -> bool,
    ) -> Option<Arc<dyn PanelHandle>> {
        let mut result_panel = None;
        let mut serialize = false;
        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                let mut focus_center = false;
                let panel = dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);

                    let panel = dock.active_panel().cloned();
                    if let Some(panel) = panel.as_ref() {
                        if should_focus(&**panel, window, cx) {
                            dock.set_open(true, window, cx);
                            panel.panel_focus_handle(cx).focus(window, cx);
                        } else {
                            focus_center = true;
                        }
                    }
                    panel
                });

                if focus_center {
                    self.active_pane
                        .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
                }

                result_panel = panel;
                serialize = true;
                break;
            }
        }

        if serialize {
            self.serialize_workspace(window, cx);
        }

        cx.notify();
        result_panel
    }

    /// Open the panel of the given type
    pub fn open_panel<T: Panel>(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_panel_item::<T>(false, window, cx).is_some() {
            return;
        }

        for dock in self.all_docks() {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);
                    dock.set_open(true, window, cx);
                });
            }
        }
    }

    /// Open the panel of the given type, dismissing any zoomed items that
    /// would obscure it (e.g. a zoomed terminal).
    pub fn reveal_panel<T: Panel>(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_panel_item::<T>(false, window, cx).is_some() {
            return;
        }

        let dock_position = self.all_docks().iter().find_map(|dock| {
            let dock = dock.read(cx);
            dock.panel_index_for_type::<T>().map(|_| dock.position())
        });
        self.dismiss_zoomed_items_to_reveal(dock_position, window, cx);
        self.open_panel::<T>(window, cx);
    }

    pub fn close_panel<T: Panel>(&self, window: &mut Window, cx: &mut Context<Self>) {
        for dock in self.all_docks().iter() {
            dock.update(cx, |dock, cx| {
                if dock.panel::<T>().is_some() {
                    dock.set_open(false, window, cx)
                }
            })
        }
    }

    pub fn panel<T: Panel>(&self, cx: &App) -> Option<Entity<T>> {
        self.all_docks()
            .iter()
            .find_map(|dock| dock.read(cx).panel::<T>())
    }

    fn panel_item_for<T: Panel>(
        &self,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                item.is_panel::<T>()
                    .then(|| (pane.clone(), ix, item.panel()))
            })
        })
    }

    fn panel_item_for_id(
        &self,
        panel_id: EntityId,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                (item.panel_id() == panel_id).then(|| (pane.clone(), ix, item.panel()))
            })
        })
    }

    fn panel_item_for_proto_id(
        &self,
        panel_id: PanelId,
        cx: &App,
    ) -> Option<(Entity<Pane>, usize, Arc<dyn PanelHandle>)> {
        self.panes.iter().find_map(|pane| {
            if !self.pane_is_in_center(pane) {
                return None;
            }
            pane.read(cx).items().enumerate().find_map(|(ix, item)| {
                let item = item.downcast::<PanelItem>()?;
                let item = item.read(cx);
                let panel = item.panel();
                (panel.remote_id() == Some(panel_id)).then(|| (pane.clone(), ix, panel))
            })
        })
    }

    pub(crate) fn activate_panel_item_for_id(
        &mut self,
        panel_id: EntityId,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        let (pane, ix, panel) = self.panel_item_for_id(panel_id, cx)?;
        let was_hidden = pane.update(cx, |pane, cx| {
            let was_hidden = !pane.is_visible();
            pane.set_visible(true, cx);
            pane.activate_item(ix, true, focus, window, cx);
            was_hidden
        });
        if was_hidden {
            self.center.mark_positions(cx);
        }
        if focus {
            panel.panel_focus_handle(cx).focus(window, cx);
        }
        if was_hidden {
            self.serialize_workspace(window, cx);
        }
        Some(panel)
    }

    fn activate_panel_item<T: Panel>(
        &mut self,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn PanelHandle>> {
        let (pane, ix, panel) = self.panel_item_for::<T>(cx)?;
        let was_hidden = pane.update(cx, |pane, cx| {
            let was_hidden = !pane.is_visible();
            pane.set_visible(true, cx);
            pane.activate_item(ix, true, focus, window, cx);
            was_hidden
        });
        if was_hidden {
            self.center.mark_positions(cx);
        }
        if focus {
            panel.panel_focus_handle(cx).focus(window, cx);
        }
        if was_hidden {
            self.serialize_workspace(window, cx);
        }
        Some(panel)
    }

    pub(crate) fn dismiss_zoomed_items_to_reveal(
        &mut self,
        dock_to_reveal: Option<DockPosition>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // If a center pane is zoomed, unzoom it.
        for pane in &self.panes {
            if pane != &self.active_pane || dock_to_reveal.is_some() {
                pane.update(cx, |pane, cx| pane.set_zoomed(false, cx));
            }
        }

        // If another dock is zoomed, hide it.
        let mut focus_center = false;
        for dock in self.all_docks() {
            dock.update(cx, |dock, cx| {
                if Some(dock.position()) != dock_to_reveal
                    && let Some(panel) = dock.active_panel()
                    && panel.is_zoomed(window, cx)
                {
                    focus_center |= panel.panel_focus_handle(cx).contains_focused(window, cx);
                    dock.set_open(false, window, cx);
                }
            });
        }

        if focus_center {
            self.active_pane
                .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx))
        }

        if self.zoomed_position != dock_to_reveal {
            self.zoomed = None;
            self.zoomed_position = None;
            cx.emit(Event::ZoomChanged);
        }

        cx.notify();
    }
}
