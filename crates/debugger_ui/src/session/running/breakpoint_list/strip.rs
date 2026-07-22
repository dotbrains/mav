use super::*;

impl BreakpointOptionsStrip {
    fn is_toggled(&self, expected_mode: ActiveBreakpointStripMode) -> bool {
        self.is_selected && self.strip_mode == Some(expected_mode)
    }

    fn on_click_callback(
        &self,
        mode: ActiveBreakpointStripMode,
    ) -> impl for<'a> Fn(&ClickEvent, &mut Window, &'a mut App) + use<> {
        let list = self.breakpoint.weak.clone();
        let ix = self.index;
        move |_, window, cx| {
            list.update(cx, |this, cx| {
                if this.strip_mode != Some(mode) {
                    this.set_active_breakpoint_property(mode, window, cx);
                } else if this.selected_ix == Some(ix) {
                    this.strip_mode.take();
                } else {
                    cx.propagate();
                }
            })
            .ok();
        }
    }

    fn add_focus_styles(
        &self,
        kind: ActiveBreakpointStripMode,
        available: bool,
        window: &Window,
        cx: &App,
    ) -> impl Fn(Div) -> Div {
        move |this: Div| {
            // Avoid layout shifts in case there's no colored border
            let this = this.border_1().rounded_sm();
            let color = cx.theme().colors();

            if self.is_selected && self.strip_mode == Some(kind) {
                if self.focus_handle.is_focused(window) {
                    this.bg(color.editor_background)
                        .border_color(color.border_focused)
                } else {
                    this.border_color(color.border)
                }
            } else if !available {
                this.border_color(color.border_transparent)
            } else {
                this
            }
        }
    }
}

impl RenderOnce for BreakpointOptionsStrip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.breakpoint.id();
        let supports_logs = self.props.contains(SupportedBreakpointProperties::LOG);
        let supports_condition = self
            .props
            .contains(SupportedBreakpointProperties::CONDITION);
        let supports_hit_condition = self
            .props
            .contains(SupportedBreakpointProperties::HIT_CONDITION);
        let has_logs = self.breakpoint.has_log();
        let has_condition = self.breakpoint.has_condition();
        let has_hit_condition = self.breakpoint.has_hit_condition();
        let style_for_toggle = |mode, is_enabled| {
            if is_enabled && self.strip_mode == Some(mode) && self.is_selected {
                ui::ButtonStyle::Filled
            } else {
                ui::ButtonStyle::Subtle
            }
        };
        let color_for_toggle = |is_enabled| {
            if is_enabled {
                Color::Default
            } else {
                Color::Muted
            }
        };

        h_flex()
            .gap_px()
            .justify_end()
            .when(has_logs || self.is_selected, |this| {
                this.child(
                    div()
                    .map(self.add_focus_styles(
                        ActiveBreakpointStripMode::Log,
                        supports_logs,
                        window,
                        cx,
                    ))
                    .child(
                        IconButton::new(
                            SharedString::from(format!("{id}-log-toggle")),
                            IconName::Notepad,
                        )
                        .shape(ui::IconButtonShape::Square)
                        .style(style_for_toggle(ActiveBreakpointStripMode::Log, has_logs))
                        .icon_size(IconSize::Small)
                        .icon_color(color_for_toggle(has_logs))
                        .when(has_logs, |this| this.indicator(Indicator::dot().color(Color::Info)))
                        .disabled(!supports_logs)
                        .toggle_state(self.is_toggled(ActiveBreakpointStripMode::Log))
                        .on_click(self.on_click_callback(ActiveBreakpointStripMode::Log))
                        .tooltip(|_window, cx|  {
                            Tooltip::with_meta(
                                "Set Log Message",
                                None,
                                "Set log message to display (instead of stopping) when a breakpoint is hit.",
                                cx,
                            )
                        }),
                    )
                )
            })
            .when(has_condition || self.is_selected, |this| {
                this.child(
                    div()
                        .map(self.add_focus_styles(
                            ActiveBreakpointStripMode::Condition,
                            supports_condition,
                            window,
                            cx,
                        ))
                        .child(
                            IconButton::new(
                                SharedString::from(format!("{id}-condition-toggle")),
                                IconName::SplitAlt,
                            )
                            .shape(ui::IconButtonShape::Square)
                            .style(style_for_toggle(
                                ActiveBreakpointStripMode::Condition,
                                has_condition,
                            ))
                            .icon_size(IconSize::Small)
                            .icon_color(color_for_toggle(has_condition))
                            .when(has_condition, |this| this.indicator(Indicator::dot().color(Color::Info)))
                            .disabled(!supports_condition)
                            .toggle_state(self.is_toggled(ActiveBreakpointStripMode::Condition))
                            .on_click(self.on_click_callback(ActiveBreakpointStripMode::Condition))
                            .tooltip(|_window, cx|  {
                                Tooltip::with_meta(
                                    "Set Condition",
                                    None,
                                    "Set condition to evaluate when a breakpoint is hit. Program execution will stop only when the condition is met.",
                                    cx,
                                )
                            }),
                        )
                )
            })
            .when(has_hit_condition || self.is_selected, |this| {
                this.child(div()
                    .map(self.add_focus_styles(
                        ActiveBreakpointStripMode::HitCondition,
                        supports_hit_condition,
                        window,
                        cx,
                    ))
                    .child(
                        IconButton::new(
                            SharedString::from(format!("{id}-hit-condition-toggle")),
                            IconName::ArrowDown10,
                        )
                        .style(style_for_toggle(
                            ActiveBreakpointStripMode::HitCondition,
                            has_hit_condition,
                        ))
                        .shape(ui::IconButtonShape::Square)
                        .icon_size(IconSize::Small)
                        .icon_color(color_for_toggle(has_hit_condition))
                        .when(has_hit_condition, |this| this.indicator(Indicator::dot().color(Color::Info)))
                        .disabled(!supports_hit_condition)
                        .toggle_state(self.is_toggled(ActiveBreakpointStripMode::HitCondition))
                        .on_click(self.on_click_callback(ActiveBreakpointStripMode::HitCondition))
                        .tooltip(|_window, cx|  {
                            Tooltip::with_meta(
                                "Set Hit Condition",
                                None,
                                "Set expression that controls how many hits of the breakpoint are ignored.",
                                cx,
                            )
                        }),
                    ))

            })
    }
}
