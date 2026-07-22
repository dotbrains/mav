use super::*;

impl DebugPanel {
    pub(crate) fn top_controls_strip(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let active_session = self.active_session.clone();
        let focus_handle = self.focus_handle.clone();
        let is_side = false;
        let div = h_flex();

        let new_session_button = || {
            IconButton::new("debug-new-session", IconName::Plus)
                .icon_size(IconSize::Small)
                .on_click({
                    move |_, window, cx| window.dispatch_action(crate::Start.boxed_clone(), cx)
                })
                .tooltip({
                    let focus_handle = focus_handle.clone();
                    move |_window, cx| {
                        Tooltip::for_action_in(
                            "Start Debug Session",
                            &crate::Start,
                            &focus_handle,
                            cx,
                        )
                    }
                })
        };

        let edit_debug_json_button = || {
            IconButton::new("debug-edit-debug-json", IconName::Code)
                .icon_size(IconSize::Small)
                .on_click(|_, window, cx| {
                    window.dispatch_action(mav_actions::OpenProjectDebugTasks.boxed_clone(), cx);
                })
                .tooltip(Tooltip::text("Edit debug.json"))
        };

        let documentation_button = || {
            IconButton::new("debug-open-documentation", IconName::CircleHelp)
                .icon_size(IconSize::Small)
                .on_click(move |_, _, cx| cx.open_url("https://mav.dev/docs/debugger"))
                .tooltip(Tooltip::text("Open Documentation"))
        };

        let logs_button = || {
            IconButton::new("debug-open-logs", IconName::Notepad)
                .icon_size(IconSize::Small)
                .on_click(move |_, window, cx| {
                    window.dispatch_action(debugger_tools::OpenDebugAdapterLogs.boxed_clone(), cx)
                })
                .tooltip(Tooltip::text("Open Debug Adapter Logs"))
        };

        let thread_status = active_session
            .as_ref()
            .map(|session| session.read(cx).running_state())
            .and_then(|state| state.read(cx).thread_status(cx))
            .unwrap_or(project::debugger::session::ThreadStatus::Exited);

        Some(
            div.w_full()
                .py_1()
                .px_1p5()
                .justify_between()
                .border_b_1()
                .border_color(cx.theme().colors().border_variant)
                .when(is_side, |this| this.gap_1().h(Tab::container_height(cx)))
                .child(
                    h_flex()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_1()
                                .w_full()
                                .when(active_session.is_none(), |this| {
                                    this.child(Label::new("Debugger").size(LabelSize::Small))
                                })
                                .when_some(
                                    active_session
                                        .as_ref()
                                        .map(|session| session.read(cx).running_state()),
                                    |this, running_state| {
                                        let capabilities = running_state.read(cx).capabilities(cx);
                                        let supports_detach =
                                            running_state.read(cx).session().read(cx).is_attached();

                                        this.map(|this| {
                                            if thread_status == ThreadStatus::Running {
                                                this.child(
                                                    IconButton::new(
                                                        "debug-pause",
                                                        IconName::DebugPause,
                                                    )
                                                    .icon_size(IconSize::Small)
                                                    .on_click(window.listener_for(
                                                        running_state,
                                                        |this, _, _window, cx| {
                                                            this.pause_thread(cx);
                                                        },
                                                    ))
                                                    .tooltip({
                                                        let focus_handle = focus_handle.clone();
                                                        move |_window, cx| {
                                                            Tooltip::for_action_in(
                                                                "Pause Program",
                                                                &Pause,
                                                                &focus_handle,
                                                                cx,
                                                            )
                                                        }
                                                    }),
                                                )
                                            } else {
                                                this.child(
                                                    IconButton::new(
                                                        "debug-continue",
                                                        IconName::DebugContinue,
                                                    )
                                                    .icon_size(IconSize::Small)
                                                    .on_click(window.listener_for(
                                                        running_state,
                                                        |this, _, _window, cx| {
                                                            this.continue_thread(cx)
                                                        },
                                                    ))
                                                    .disabled(
                                                        thread_status != ThreadStatus::Stopped,
                                                    )
                                                    .tooltip({
                                                        let focus_handle = focus_handle.clone();
                                                        move |_window, cx| {
                                                            Tooltip::for_action_in(
                                                                "Continue Program",
                                                                &Continue,
                                                                &focus_handle,
                                                                cx,
                                                            )
                                                        }
                                                    }),
                                                )
                                            }
                                        })
                                        .child(
                                            IconButton::new("step-over", IconName::DebugStepOver)
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, _window, cx| {
                                                        this.step_over(cx);
                                                    },
                                                ))
                                                .disabled(thread_status != ThreadStatus::Stopped)
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            "Step Over",
                                                            &StepOver,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        .child(
                                            IconButton::new("step-into", IconName::DebugStepInto)
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, _window, cx| {
                                                        this.step_in(cx);
                                                    },
                                                ))
                                                .disabled(thread_status != ThreadStatus::Stopped)
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            "Step In",
                                                            &StepInto,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        .child(
                                            IconButton::new("step-out", IconName::DebugStepOut)
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, _window, cx| {
                                                        this.step_out(cx);
                                                    },
                                                ))
                                                .disabled(thread_status != ThreadStatus::Stopped)
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            "Step Out",
                                                            &StepOut,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        .child(Divider::vertical())
                                        .child(
                                            IconButton::new("debug-restart", IconName::RotateCcw)
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, window, cx| {
                                                        this.rerun_session(window, cx);
                                                    },
                                                ))
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            "Rerun Session",
                                                            &RerunSession,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        .child(
                                            IconButton::new("debug-stop", IconName::Power)
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, _window, cx| {
                                                        if this.session().read(cx).is_building() {
                                                            this.session().update(
                                                                cx,
                                                                |session, cx| {
                                                                    session.shutdown(cx).detach()
                                                                },
                                                            );
                                                        } else {
                                                            this.stop_thread(cx);
                                                        }
                                                    },
                                                ))
                                                .disabled(active_session.as_ref().is_none_or(
                                                    |session| {
                                                        session
                                                            .read(cx)
                                                            .session(cx)
                                                            .read(cx)
                                                            .is_terminated()
                                                    },
                                                ))
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    let label = if capabilities
                                                        .supports_terminate_threads_request
                                                        .unwrap_or_default()
                                                    {
                                                        "Terminate Thread"
                                                    } else {
                                                        "Terminate All Threads"
                                                    };
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            label,
                                                            &Stop,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                        )
                                        .when(supports_detach, |div| {
                                            div.child(
                                                IconButton::new(
                                                    "debug-disconnect",
                                                    IconName::DebugDetach,
                                                )
                                                .disabled(
                                                    thread_status != ThreadStatus::Stopped
                                                        && thread_status != ThreadStatus::Running,
                                                )
                                                .icon_size(IconSize::Small)
                                                .on_click(window.listener_for(
                                                    running_state,
                                                    |this, _, _, cx| {
                                                        this.detach_client(cx);
                                                    },
                                                ))
                                                .tooltip({
                                                    let focus_handle = focus_handle.clone();
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            "Detach",
                                                            &Detach,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                }),
                                            )
                                        })
                                        .when(
                                            cx.has_flag::<DebuggerHistoryFeatureFlag>(),
                                            |this| {
                                                this.child(Divider::vertical()).child(
                                                    SplitButton::new(
                                                        self.render_history_button(
                                                            &running_state,
                                                            thread_status,
                                                            window,
                                                        ),
                                                        self.render_history_toggle_button(
                                                            thread_status,
                                                            &running_state,
                                                        )
                                                        .into_any_element(),
                                                    )
                                                    .style(ui::SplitButtonStyle::Outlined),
                                                )
                                            },
                                        )
                                    },
                                ),
                        )
                        .when(is_side, |this| {
                            this.child(new_session_button())
                                .child(edit_debug_json_button())
                                .child(documentation_button())
                                .child(logs_button())
                        }),
                )
                .child(
                    h_flex()
                        .gap_0p5()
                        .when(is_side, |this| this.justify_between())
                        .child(
                            h_flex().when_some(
                                active_session
                                    .as_ref()
                                    .map(|session| session.read(cx).running_state())
                                    .cloned(),
                                |this, running_state| {
                                    this.children({
                                        let threads =
                                            running_state.update(cx, |running_state, cx| {
                                                let session = running_state.session();
                                                session.read(cx).is_started().then(|| {
                                                    session.update(cx, |session, cx| {
                                                        session.threads(cx)
                                                    })
                                                })
                                            });

                                        threads.and_then(|threads| {
                                            self.render_thread_dropdown(
                                                &running_state,
                                                threads,
                                                window,
                                                cx,
                                            )
                                        })
                                    })
                                    .when(!is_side, |this| {
                                        this.gap_0p5().child(Divider::vertical())
                                    })
                                },
                            ),
                        )
                        .child(
                            h_flex()
                                .gap_0p5()
                                .children(self.render_session_menu(
                                    self.active_session(),
                                    self.running_state(cx),
                                    window,
                                    cx,
                                ))
                                .when(!is_side, |this| {
                                    this.child(new_session_button())
                                        .child(edit_debug_json_button())
                                        .child(documentation_button())
                                        .child(logs_button())
                                }),
                        ),
                ),
        )
    }

    fn render_history_button(
        &self,
        running_state: &Entity<RunningState>,
        thread_status: ThreadStatus,
        window: &mut Window,
    ) -> IconButton {
        IconButton::new("debug-back-in-history", IconName::HistoryRerun)
            .icon_size(IconSize::Small)
            .on_click(window.listener_for(running_state, |this, _, _window, cx| {
                this.session().update(cx, |session, cx| {
                    let ix = session
                        .active_snapshot_index()
                        .unwrap_or_else(|| session.historic_snapshots().len());

                    session.select_historic_snapshot(Some(ix.saturating_sub(1)), cx);
                })
            }))
            .disabled(
                thread_status == ThreadStatus::Running || thread_status == ThreadStatus::Stepping,
            )
    }

    fn render_history_toggle_button(
        &self,
        thread_status: ThreadStatus,
        running_state: &Entity<RunningState>,
    ) -> impl IntoElement {
        PopoverMenu::new("debug-back-in-history-menu")
            .trigger(
                ui::ButtonLike::new_rounded_right("debug-back-in-history-menu-trigger")
                    .layer(ui::ElevationIndex::ModalSurface)
                    .size(ui::ButtonSize::None)
                    .child(
                        div()
                            .px_1()
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    )
                    .disabled(
                        thread_status == ThreadStatus::Running
                            || thread_status == ThreadStatus::Stepping,
                    ),
            )
            .menu({
                let running_state = running_state.clone();
                move |window, cx| {
                    let handler =
                        |ix: Option<usize>, running_state: Entity<RunningState>, cx: &mut App| {
                            running_state.update(cx, |state, cx| {
                                state.session().update(cx, |session, cx| {
                                    session.select_historic_snapshot(ix, cx);
                                })
                            })
                        };

                    let running_state = running_state.clone();
                    Some(ContextMenu::build(
                        window,
                        cx,
                        move |mut context_menu, _window, cx| {
                            let history = running_state
                                .read(cx)
                                .session()
                                .read(cx)
                                .historic_snapshots();

                            context_menu = context_menu.entry("Current State", None, {
                                let running_state = running_state.clone();
                                move |_window, cx| {
                                    handler(None, running_state.clone(), cx);
                                }
                            });
                            context_menu = context_menu.separator();

                            for (ix, _) in history.iter().enumerate().rev() {
                                context_menu =
                                    context_menu.entry(format!("history-{}", ix + 1), None, {
                                        let running_state = running_state.clone();
                                        move |_window, cx| {
                                            handler(Some(ix), running_state.clone(), cx);
                                        }
                                    });
                            }

                            context_menu
                        },
                    ))
                }
            })
            .anchor(Anchor::TopRight)
    }
}
