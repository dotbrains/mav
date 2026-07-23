use super::*;

impl Editor {
    pub(crate) fn base_subscriptions(
        is_minimap: bool,
        multi_buffer: &Entity<MultiBuffer>,
        display_map: &Entity<DisplayMap>,
        blink_manager: &Entity<BlinkManager>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        if is_minimap {
            return Vec::new();
        }

        vec![
            cx.observe(multi_buffer, Self::on_buffer_changed),
            cx.subscribe_in(multi_buffer, window, Self::on_buffer_event),
            cx.observe_in(display_map, window, Self::on_display_map_changed),
            cx.observe(blink_manager, |_, _, cx| cx.notify()),
            cx.observe_global_in::<SettingsStore>(window, Self::settings_changed),
            cx.observe_global_in::<GlobalTheme>(window, Self::theme_changed),
            observe_buffer_font_size_adjustment(cx, |_, cx| cx.notify()),
            cx.observe_window_activation(window, |editor, window, cx| {
                let active = window.is_window_active();
                editor.blink_manager.update(cx, |blink_manager, cx| {
                    if active {
                        blink_manager.enable(cx);
                    } else {
                        blink_manager.disable(cx);
                    }
                });
            }),
        ]
    }
}
