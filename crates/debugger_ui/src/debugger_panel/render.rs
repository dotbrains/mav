use super::*;

impl Render for DebugPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();

        if self
            .active_session
            .as_ref()
            .map(|session| session.read(cx).running_state())
            .map(|state| state.read(cx).has_open_context_menu(cx))
            .unwrap_or(false)
        {
            self.context_menu.take();
        }

        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .key_context("DebugPanel")
            .track_focus(&self.focus_handle(cx))
            .on_action({
                let this = this.clone();
                move |_: &workspace::ActivatePaneLeft, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_pane_in_direction(SplitDirection::Left, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &workspace::ActivatePaneRight, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_pane_in_direction(SplitDirection::Right, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &workspace::ActivatePaneUp, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_pane_in_direction(SplitDirection::Up, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &workspace::ActivatePaneDown, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_pane_in_direction(SplitDirection::Down, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusConsole, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::Console, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusVariables, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::Variables, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusBreakpointList, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::BreakpointList, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusFrames, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::Frames, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusModules, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::Modules, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusLoadedSources, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::LoadedSources, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &FocusTerminal, window, cx| {
                    this.update(cx, |this, cx| {
                        this.activate_item(DebuggerPaneItem::Terminal, window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                let this = this.clone();
                move |_: &ToggleThreadPicker, window, cx| {
                    this.update(cx, |this, cx| {
                        this.toggle_thread_picker(window, cx);
                    })
                    .ok();
                }
            })
            .on_action({
                move |_: &ToggleSessionPicker, window, cx| {
                    this.update(cx, |this, cx| {
                        this.toggle_session_picker(window, cx);
                    })
                    .ok();
                }
            })
            .on_action(cx.listener(Self::toggle_zoom))
            .on_action(cx.listener(|panel, _: &ToggleExpandItem, _, cx| {
                let Some(session) = panel.active_session() else {
                    return;
                };
                let active_pane = session
                    .read(cx)
                    .running_state()
                    .read(cx)
                    .active_pane()
                    .clone();
                active_pane.update(cx, |pane, cx| {
                    let is_zoomed = pane.is_zoomed();
                    pane.set_zoomed(!is_zoomed, cx);
                });
                cx.notify();
            }))
            .on_action(cx.listener(Self::copy_debug_adapter_arguments))
            .when(self.active_session.is_some(), |this| {
                this.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if this
                            .active_session
                            .as_ref()
                            .map(|session| {
                                let state = session.read(cx).running_state();
                                state.read(cx).has_pane_at_position(event.position)
                            })
                            .unwrap_or(false)
                        {
                            this.deploy_context_menu(event.position, window, cx);
                        }
                    }),
                )
                .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                    deferred(
                        anchored()
                            .position(*position)
                            .anchor(gpui::Anchor::TopLeft)
                            .child(menu.clone()),
                    )
                    .with_priority(1)
                }))
            })
            .map(|this| {
                if let Some(active_session) = self.active_session.clone() {
                    this.child(
                        v_flex()
                            .size_full()
                            .child(h_flex().children(self.top_controls_strip(window, cx)))
                            .child(div().flex_1().min_h_0().child(active_session)),
                    )
                } else {
                    let welcome_experience = v_flex()
                        .flex_1()
                        .w_full()
                        .min_h_0()
                        .pr_8()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .child(
                            Button::new("spawn-new-session-empty-state", "New Session")
                                .start_icon(
                                    Icon::new(IconName::Plus)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(crate::Start.boxed_clone(), cx);
                                }),
                        )
                        .child(
                            Button::new("edit-debug-settings", "Edit debug.json")
                                .start_icon(
                                    Icon::new(IconName::Code)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        mav_actions::OpenProjectDebugTasks.boxed_clone(),
                                        cx,
                                    );
                                }),
                        )
                        .child(
                            Button::new("open-debugger-docs", "Debugger Docs")
                                .start_icon(
                                    Icon::new(IconName::Book)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .on_click(|_, _, cx| cx.open_url("https://mav.dev/docs/debugger")),
                        )
                        .child(
                            Button::new(
                                "spawn-new-session-install-extensions",
                                "Debugger Extensions",
                            )
                            .start_icon(
                                Icon::new(IconName::Blocks)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(
                                    mav_actions::Extensions {
                                        category_filter: Some(
                                            mav_actions::ExtensionCategoryFilter::DebugAdapters,
                                        ),
                                        id: None,
                                    }
                                    .boxed_clone(),
                                    cx,
                                );
                            }),
                        );

                    let has_breakpoints = self
                        .project
                        .read(cx)
                        .breakpoint_store()
                        .read(cx)
                        .all_source_breakpoints(cx)
                        .values()
                        .any(|breakpoints| !breakpoints.is_empty());

                    let breakpoint_list = v_flex()
                        .group("base-breakpoint-list")
                        .min_w_1_3()
                        .h_full()
                        .child(
                            h_flex()
                                .track_focus(&self.breakpoint_list.focus_handle(cx))
                                .h(Tab::content_height(cx))
                                .p_1p5()
                                .w_full()
                                .justify_between()
                                .border_b_1()
                                .border_color(cx.theme().colors().border_variant)
                                .child(Label::new("Breakpoints").size(LabelSize::Small))
                                .child(
                                    h_flex().visible_on_hover("base-breakpoint-list").child(
                                        self.breakpoint_list.read(cx).render_control_strip(),
                                    ),
                                ),
                        )
                        .when(has_breakpoints, |this| {
                            this.child(self.breakpoint_list.clone())
                        })
                        .when(!has_breakpoints, |this| {
                            this.child(
                                v_flex().size_full().items_center().justify_center().child(
                                    Label::new("No Breakpoints Set")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                            )
                        });

                    let dashboard = v_flex()
                        .w_2_3()
                        .h_full()
                        .min_w_0()
                        .child(h_flex().children(self.top_controls_strip(window, cx)))
                        .child(welcome_experience);

                    this.child(
                        v_flex()
                            .size_full()
                            .overflow_hidden()
                            .gap_1()
                            .items_center()
                            .justify_center()
                            .child(
                                h_flex()
                                    .size_full()
                                    .child(breakpoint_list)
                                    .child(Divider::vertical())
                                    .child(dashboard),
                            ),
                    )
                }
            })
            .into_any()
    }
}
