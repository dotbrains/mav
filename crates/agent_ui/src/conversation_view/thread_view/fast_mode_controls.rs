use super::fast_mode_warning::{fast_mode_warning_dismissed, set_fast_mode_warning_dismissed};
use super::*;

impl ThreadView {
    pub(super) fn fast_mode_available(&self, cx: &Context<Self>) -> bool {
        self.as_native_thread(cx)
            .and_then(|thread| thread.read(cx).model())
            .map(|model| model.supports_fast_mode())
            .unwrap_or(false)
    }

    pub(super) fn render_fast_mode_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.fast_mode_available(cx) {
            return None;
        }

        let thread = self.as_native_thread(cx)?.read(cx);
        let is_fast = matches!(thread.speed(), Some(Speed::Fast));

        let model_identity = thread
            .model()
            .map(|model| (model.provider_id(), model.id()));

        let (tooltip_label, color, icon, new_speed) = if is_fast {
            (
                "Disable Fast Mode",
                Color::Accent,
                IconName::FastForward,
                Speed::Standard,
            )
        } else {
            (
                "Enable Fast Mode",
                Color::Custom(cx.theme().colors().icon_disabled.opacity(0.8)),
                IconName::FastForwardOff,
                Speed::Fast,
            )
        };

        let focus_handle = self.message_editor.focus_handle(cx);

        let pending_confirmation = (!is_fast)
            .then(|| self.pending_fast_mode_confirmation(cx))
            .flatten();

        let icon_button = IconButton::new("fast-mode", icon)
            .icon_size(IconSize::Small)
            .icon_color(color);

        if let Some((provider_id, model_id, confirmation)) = pending_confirmation {
            let weak_self = cx.entity().downgrade();
            let tooltip_focus = focus_handle;

            return Some(
                PopoverMenu::new("fast-mode-warning")
                    .with_handle(self.fast_mode_menu_handle.clone())
                    .trigger_with_tooltip(icon_button, move |_, cx| {
                        Tooltip::for_action_in(tooltip_label, &ToggleFastMode, &tooltip_focus, cx)
                    })
                    .menu(move |window, cx| {
                        let weak_self = weak_self.clone();
                        let confirmation = confirmation.clone();
                        let provider_id = provider_id.clone();
                        let model_id = model_id.clone();

                        Some(ContextMenu::build(window, cx, move |menu, _window, _cx| {
                            let message = confirmation.message.clone();
                            menu.custom_row(move |_window, _cx| {
                                div()
                                    .max_w_72()
                                    .child(Label::new(confirmation.title.clone()))
                                    .child(Label::new(message.clone()).color(Color::Muted))
                                    .into_any_element()
                            })
                            .separator()
                            .item(ContextMenuEntry::new("Enable Now").handler({
                                let weak_self = weak_self.clone();
                                move |_window, cx| {
                                    weak_self
                                        .update(cx, |this, cx| {
                                            this.apply_fast_mode_speed(Speed::Fast, cx);
                                        })
                                        .log_err();
                                }
                            }))
                            .item(
                                ContextMenuEntry::new("Enable and Don't Show Again").handler({
                                    let weak_self = weak_self.clone();
                                    let provider_id = provider_id.clone();
                                    let model_id = model_id;
                                    move |_window, cx| {
                                        weak_self
                                            .update(cx, |this, cx| {
                                                this.apply_fast_mode_speed(Speed::Fast, cx);
                                            })
                                            .log_err();
                                        set_fast_mode_warning_dismissed(
                                            &provider_id,
                                            &model_id,
                                            cx,
                                        );
                                    }
                                }),
                            )
                        }))
                    })
                    .offset(gpui::Point {
                        x: px(0.0),
                        y: px(-2.0),
                    })
                    .anchor(gpui::Anchor::BottomLeft)
                    .into_any_element(),
            );
        }

        let _ = model_identity;

        Some(
            icon_button
                .tooltip(move |_, cx| {
                    Tooltip::for_action_in(tooltip_label, &ToggleFastMode, &focus_handle, cx)
                })
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.apply_fast_mode_speed(new_speed, cx);
                }))
                .into_any_element(),
        )
    }

    pub(super) fn pending_fast_mode_confirmation(
        &self,
        cx: &App,
    ) -> Option<(
        LanguageModelProviderId,
        LanguageModelId,
        FastModeConfirmation,
    )> {
        let thread = self.as_native_thread(cx)?.read(cx);
        let model = thread.model()?;
        let provider_id = model.provider_id();
        let model_id = model.id();
        let confirmation = LanguageModelRegistry::read_global(cx)
            .provider(&provider_id)
            .and_then(|provider| provider.fast_mode_confirmation(cx))?;
        if fast_mode_warning_dismissed(&provider_id, &model_id, cx) {
            return None;
        }
        Some((provider_id, model_id, confirmation))
    }
}
