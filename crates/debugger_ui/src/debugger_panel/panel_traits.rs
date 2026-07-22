use super::*;

impl EventEmitter<PanelEvent> for DebugPanel {}

impl Focusable for DebugPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for DebugPanel {
    fn persistent_name() -> &'static str {
        "DebugPanel"
    }

    fn panel_key() -> &'static str {
        DEBUG_PANEL_KEY
    }

    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        DebuggerSettings::get_global(cx).dock.into()
    }

    fn position_is_valid(&self, _: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if position.axis() != self.position(window, cx).axis() {
            self.sessions_with_children.keys().for_each(|session_item| {
                session_item.update(cx, |item, cx| {
                    item.running_state()
                        .update(cx, |state, cx| state.invert_axies(cx))
                })
            });
        }

        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.debugger.get_or_insert_default().dock = Some(position.into());
        });
    }

    fn default_size(&self, _window: &Window, _: &App) -> Pixels {
        px(300.)
    }

    fn remote_id() -> Option<proto::PanelId> {
        Some(proto::PanelId::DebugPanel)
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        DebuggerSettings::get_global(cx)
            .button
            .then_some(IconName::Debug)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<&'static str> {
        if DebuggerSettings::get_global(cx).button {
            Some("Debug Panel")
        } else {
            None
        }
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn pane(&self) -> Option<Entity<Pane>> {
        None
    }

    fn activation_priority(&self) -> u32 {
        7
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.debugger.get_or_insert_default().button = Some(false);
        }))
    }

    fn set_active(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) {}

    fn is_zoomed(&self, _window: &Window, _cx: &App) -> bool {
        self.is_zoomed
    }

    fn set_zoomed(&mut self, zoomed: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.is_zoomed = zoomed;
        cx.notify();
    }
}

impl Item for DebugPanel {
    type Event = PanelEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Debugger".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Debug).color(Color::Muted))
    }

    fn to_item_events(_event: &PanelEvent, _f: &mut dyn FnMut(ItemEvent)) {}
}
