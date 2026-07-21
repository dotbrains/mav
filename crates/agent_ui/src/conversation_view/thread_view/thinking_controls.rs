use super::*;

impl ThreadView {
    pub(super) fn render_thinking_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.as_native_thread(cx)?.read(cx);
        let model = thread.model()?;

        let supports_thinking = model.supports_thinking();
        if !supports_thinking {
            return None;
        }

        // A toggle would be dishonest for models that always think: only
        // offer the effort selector.
        if !model.supports_disabling_thinking() {
            let effort_levels = model.supported_effort_levels();
            if effort_levels.is_empty() {
                return None;
            }
            return Some(
                self.render_effort_selector(
                    effort_levels,
                    thread.thinking_effort().cloned(),
                    true,
                    cx,
                )
                .into_any_element(),
            );
        }

        let thinking = thread.thinking_enabled();

        let (tooltip_label, icon, color) = if thinking {
            (
                "Disable Thinking Mode",
                IconName::ThinkingMode,
                Color::Muted,
            )
        } else {
            (
                "Enable Thinking Mode",
                IconName::ThinkingModeOff,
                Color::Custom(cx.theme().colors().icon_disabled.opacity(0.8)),
            )
        };

        let focus_handle = self.message_editor.focus_handle(cx);

        let thinking_toggle = IconButton::new("thinking-mode", icon)
            .icon_size(IconSize::Small)
            .icon_color(color)
            .tooltip(move |_, cx| {
                Tooltip::for_action_in(tooltip_label, &ToggleThinkingMode, &focus_handle, cx)
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                if let Some(thread) = this.as_native_thread(cx) {
                    thread.update(cx, |thread, cx| {
                        let enable_thinking = !thread.thinking_enabled();
                        thread.set_thinking_enabled(enable_thinking, cx);

                        let favorite_key = thread.model().map(|model| {
                            (model.provider_id().0.to_string(), model.id().0.to_string())
                        });
                        let fs = thread.project().read(cx).fs().clone();
                        update_settings_file(fs, cx, move |settings, _| {
                            if let Some(agent) = settings.agent.as_mut() {
                                if let Some(default_model) = agent.default_model.as_mut() {
                                    default_model.enable_thinking = enable_thinking;
                                }
                                if let Some((provider_id, model_id)) = &favorite_key {
                                    agent.update_favorite_model(
                                        provider_id,
                                        model_id,
                                        |favorite| favorite.enable_thinking = enable_thinking,
                                    );
                                }
                            }
                        });
                    });
                }
            }));

        if model.supported_effort_levels().is_empty() {
            return Some(thinking_toggle.into_any_element());
        }

        if !model.supported_effort_levels().is_empty() && !thinking {
            return Some(thinking_toggle.into_any_element());
        }

        let left_btn = thinking_toggle;
        let right_btn = self.render_effort_selector(
            model.supported_effort_levels(),
            thread.thinking_effort().cloned(),
            false,
            cx,
        );

        Some(
            SplitButton::new(left_btn, right_btn.into_any_element())
                .style(SplitButtonStyle::Transparent)
                .into_any_element(),
        )
    }

    fn render_effort_selector(
        &self,
        supported_effort_levels: Vec<LanguageModelEffortLevel>,
        selected_effort: Option<String>,
        standalone: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let weak_self = cx.weak_entity();

        let default_effort_level = supported_effort_levels
            .iter()
            .find(|effort_level| effort_level.is_default)
            .cloned();

        let selected = selected_effort.and_then(|effort| {
            supported_effort_levels
                .iter()
                .find(|level| level.value == effort)
                .cloned()
        });

        let label = selected
            .clone()
            .or(default_effort_level)
            .map_or("Select Effort".into(), |effort| effort.name);

        let (label_color, icon) = if self.thinking_effort_menu_handle.is_deployed() {
            (Color::Accent, IconName::ChevronUp)
        } else {
            (Color::Muted, IconName::ChevronDown)
        };

        let focus_handle = self.message_editor.focus_handle(cx);
        let show_cycle_row = supported_effort_levels.len() > 1;

        let tooltip = Tooltip::element({
            move |_, cx| {
                let mut content = v_flex().gap_1().child(
                    h_flex()
                        .gap_2()
                        .justify_between()
                        .child(Label::new("Change Thinking Effort"))
                        .child(KeyBinding::for_action_in(
                            &ToggleThinkingEffortMenu,
                            &focus_handle,
                            cx,
                        )),
                );

                if show_cycle_row {
                    content = content.child(
                        h_flex()
                            .pt_1()
                            .gap_2()
                            .justify_between()
                            .border_t_1()
                            .border_color(cx.theme().colors().border_variant)
                            .child(Label::new("Cycle Thinking Effort"))
                            .child(KeyBinding::for_action_in(
                                &CycleThinkingEffort,
                                &focus_handle,
                                cx,
                            )),
                    );
                }

                content.into_any_element()
            }
        });

        let trigger = if standalone {
            ButtonLike::new("effort-selector-trigger").child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::ThinkingMode)
                            .size(IconSize::Small)
                            .color(label_color),
                    )
                    .child(Label::new(label).size(LabelSize::Small).color(label_color))
                    .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted)),
            )
        } else {
            ButtonLike::new_rounded_right("effort-selector-trigger")
                .child(Label::new(label).size(LabelSize::Small).color(label_color))
                .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
        };

        PopoverMenu::new("effort-selector")
            .trigger_with_tooltip(
                trigger.selected_style(ButtonStyle::Tinted(TintColor::Accent)),
                tooltip,
            )
            .menu(move |window, cx| {
                Some(ContextMenu::build(window, cx, |mut menu, _window, _cx| {
                    menu = menu.header("Change Thinking Effort");

                    for effort_level in supported_effort_levels.clone() {
                        let is_selected = selected
                            .as_ref()
                            .is_some_and(|selected| selected.value == effort_level.value);
                        let entry = ContextMenuEntry::new(effort_level.name)
                            .toggleable(IconPosition::End, is_selected);

                        menu.push_item(entry.handler({
                            let effort = effort_level.value.clone();
                            let weak_self = weak_self.clone();
                            move |_window, cx| {
                                let effort = effort.clone();
                                weak_self
                                    .update(cx, |this, cx| {
                                        if let Some(thread) = this.as_native_thread(cx) {
                                            thread.update(cx, |thread, cx| {
                                                thread.set_thinking_effort(
                                                    Some(effort.to_string()),
                                                    cx,
                                                );

                                                let favorite_key = thread.model().map(|model| {
                                                    (
                                                        model.provider_id().0.to_string(),
                                                        model.id().0.to_string(),
                                                    )
                                                });
                                                let fs = thread.project().read(cx).fs().clone();
                                                update_settings_file(fs, cx, move |settings, _| {
                                                    if let Some(agent) = settings.agent.as_mut() {
                                                        if let Some(default_model) =
                                                            agent.default_model.as_mut()
                                                        {
                                                            default_model.effort =
                                                                Some(effort.to_string());
                                                        }
                                                        if let Some((provider_id, model_id)) =
                                                            &favorite_key
                                                        {
                                                            agent.update_favorite_model(
                                                                provider_id,
                                                                model_id,
                                                                |favorite| {
                                                                    favorite.effort =
                                                                        Some(effort.to_string())
                                                                },
                                                            );
                                                        }
                                                    }
                                                });
                                            });
                                        }
                                    })
                                    .ok();
                            }
                        }));
                    }

                    menu
                }))
            })
            .with_handle(self.thinking_effort_menu_handle.clone())
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            })
            .anchor(gpui::Anchor::BottomLeft)
    }
}
