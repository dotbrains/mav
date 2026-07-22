use super::*;

impl Render for CollabPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = *self.client.status().borrow();

        let is_collaboration_disabled = self
            .user_store
            .read(cx)
            .current_organization_configuration()
            .is_some_and(|config| !config.is_collaboration_enabled);

        v_flex()
            .key_context(self.dispatch_context(window, cx))
            .on_action(cx.listener(CollabPanel::cancel))
            .on_action(cx.listener(CollabPanel::select_next))
            .on_action(cx.listener(CollabPanel::select_previous))
            .on_action(cx.listener(CollabPanel::confirm))
            .on_action(cx.listener(CollabPanel::insert_space))
            .on_action(cx.listener(CollabPanel::remove_selected_channel))
            .on_action(cx.listener(CollabPanel::show_inline_context_menu))
            .on_action(cx.listener(CollabPanel::rename_selected_channel))
            .on_action(cx.listener(CollabPanel::open_selected_channel_notes))
            .on_action(cx.listener(CollabPanel::toggle_selected_channel_favorite))
            .on_action(cx.listener(CollabPanel::collapse_selected_channel))
            .on_action(cx.listener(CollabPanel::expand_selected_channel))
            .on_action(cx.listener(CollabPanel::start_move_selected_channel))
            .on_action(cx.listener(CollabPanel::move_channel_up))
            .on_action(cx.listener(CollabPanel::move_channel_down))
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(if is_collaboration_disabled {
                self.render_disabled_by_organization(cx)
            } else if !status.is_or_was_connected() || status.is_signing_in() {
                self.render_signed_out(cx)
            } else {
                self.render_signed_in(window, cx)
            })
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(gpui::Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

impl EventEmitter<PanelEvent> for CollabPanel {}

impl Panel for CollabPanel {
    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        CollaborationPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.collaboration_panel.get_or_insert_default().dock = Some(position.into())
        });
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        CollaborationPanelSettings::get_global(cx).default_width
    }

    fn set_active(&mut self, active: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if active && self.current_notification_toast.is_some() {
            self.current_notification_toast.take();
            let workspace = self.workspace.clone();
            cx.defer(move |cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        let id = NotificationId::unique::<CollabNotificationToast>();
                        workspace.dismiss_notification(&id, cx)
                    })
                    .ok();
            });
        }
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::UserGroup)
    }

    fn button_visible(&self, cx: &App) -> bool {
        CollaborationPanelSettings::get_global(cx).button
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Collab Panel")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(ToggleFocus)
    }

    fn persistent_name() -> &'static str {
        "CollabPanel"
    }

    fn panel_key() -> &'static str {
        COLLABORATION_PANEL_KEY
    }

    fn activation_priority(&self) -> u32 {
        5
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.collaboration_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

impl Focusable for CollabPanel {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.filter_editor.focus_handle(cx)
    }
}
