use super::*;

impl Pane {
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        // We not only check whether our focus handle contains focus, but also
        // whether the active item might have focus, because we might have just activated an item
        // that hasn't rendered yet.
        // Before the next render, we might transfer focus
        // to the item, and `focus_handle.contains_focus` returns false because the `active_item`
        // is not hooked up to us in the dispatch tree.
        self.focus_handle.contains_focused(window, cx)
            || self
                .active_item()
                .is_some_and(|item| item.item_focus_handle(cx).contains_focused(window, cx))
    }

    pub(super) fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.was_focused {
            self.was_focused = true;
            self.update_history(self.active_item_index);
            if !self.suppress_scroll && self.items.get(self.active_item_index).is_some() {
                self.update_active_tab(self.active_item_index);
            }
            cx.emit(Event::Focus);
            cx.notify();
        }

        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.focus_changed(true, window, cx);
        });

        if let Some(active_item) = self.active_item() {
            if self.focus_handle.is_focused(window) {
                // Schedule a redraw next frame, so that the focus changes below take effect
                cx.on_next_frame(window, |_, _, cx| {
                    cx.notify();
                });

                // Pane was focused directly. We need to either focus a view inside the active item,
                // or focus the active item itself
                if let Some(weak_last_focus_handle) =
                    self.last_focus_handle_by_item.get(&active_item.item_id())
                    && let Some(focus_handle) = weak_last_focus_handle.upgrade()
                {
                    focus_handle.focus(window, cx);
                    return;
                }

                active_item.item_focus_handle(cx).focus(window, cx);
            } else if let Some(focused) = window.focused(cx)
                && !self.context_menu_focused(window, cx)
            {
                self.last_focus_handle_by_item
                    .insert(active_item.item_id(), focused.downgrade());
            }
        } else if self.should_display_welcome_page
            && let Some(welcome_page) = self.welcome_page.as_ref()
        {
            if self.focus_handle.is_focused(window) {
                welcome_page.read(cx).focus_handle(cx).focus(window, cx);
            }
        }
    }

    pub fn context_menu_focused(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.new_item_context_menu_handle.is_focused(window, cx)
            || self.split_item_context_menu_handle.is_focused(window, cx)
    }

    pub(super) fn focus_out(
        &mut self,
        _event: FocusOutEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.was_focused = false;
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.focus_changed(false, window, cx);
        });

        cx.notify();
    }

    pub(super) fn project_events(
        &mut self,
        _project: Entity<Project>,
        event: &project::Event,
        cx: &mut Context<Self>,
    ) {
        match event {
            project::Event::DiskBasedDiagnosticsFinished { .. }
            | project::Event::DiagnosticsUpdated { .. } => {
                if ItemSettings::get_global(cx).show_diagnostics != ShowDiagnostics::Off {
                    self.diagnostic_summary_update = cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                        this.update(cx, |this, cx| {
                            this.update_diagnostics(cx);
                            cx.notify();
                        })
                        .log_err();
                    });
                }
            }
            _ => {}
        }
    }

    fn update_diagnostics(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.upgrade() else {
            return;
        };
        let show_diagnostics = ItemSettings::get_global(cx).show_diagnostics;
        self.diagnostics = if show_diagnostics != ShowDiagnostics::Off {
            project
                .read(cx)
                .diagnostic_summaries(false, cx)
                .filter_map(|(project_path, _, diagnostic_summary)| {
                    if diagnostic_summary.error_count > 0 {
                        Some((project_path, DiagnosticSeverity::ERROR))
                    } else if diagnostic_summary.warning_count > 0
                        && show_diagnostics != ShowDiagnostics::Errors
                    {
                        Some((project_path, DiagnosticSeverity::WARNING))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            HashMap::default()
        }
    }

    pub(super) fn settings_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_bar_settings = TabBarSettings::get_global(cx);

        if let Some(display_nav_history_buttons) = self.display_nav_history_buttons.as_mut() {
            *display_nav_history_buttons = tab_bar_settings.show_nav_history_buttons;
        }

        self.show_tab_bar_buttons = tab_bar_settings.show_tab_bar_buttons;

        if !PreviewTabsSettings::get_global(cx).enabled {
            self.preview_item_id = None;
            self.nav_history.0.lock().preview_item_id = None;
        }

        let workspace_settings = WorkspaceSettings::get_global(cx);

        self.focus_follows_mouse = workspace_settings.focus_follows_mouse;

        let new_max_tabs = workspace_settings.max_tabs;

        if self.use_max_tabs && new_max_tabs != self.max_tabs {
            self.max_tabs = new_max_tabs;
            self.close_items_on_settings_change(window, cx);
        }

        self.update_diagnostics(cx);
        cx.notify();
    }
}
