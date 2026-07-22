use super::*;

impl Render for MultiWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let multi_workspace_enabled = self.multi_workspace_enabled(cx);
        let sidebar_side = self.sidebar_side(cx);
        let sidebar_on_right = sidebar_side == SidebarSide::Right;
        let card_gap = workspace_card_gap(cx);

        let sidebar: Option<AnyElement> = if multi_workspace_enabled && self.sidebar_open() {
            self.sidebar.as_ref().map(|sidebar_handle| {
                let weak = cx.weak_entity();

                let sidebar_width = sidebar_handle.width(cx);
                let resize_handle_overhang = SIDEBAR_RESIZE_HANDLE_SIZE / 2.;
                let resize_handle_width = card_gap + SIDEBAR_RESIZE_HANDLE_SIZE;
                let resize_handle = deferred(
                    div()
                        .id("sidebar-resize-handle")
                        .absolute()
                        .when(!sidebar_on_right, |el| {
                            el.right(-(card_gap + resize_handle_overhang))
                        })
                        .when(sidebar_on_right, |el| {
                            el.left(-(card_gap + resize_handle_overhang))
                        })
                        .top(px(0.))
                        .h_full()
                        .w(resize_handle_width)
                        .cursor_col_resize()
                        .on_drag(DraggedSidebar, |dragged, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| dragged.clone())
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_up(MouseButton::Left, move |event, _, cx| {
                            if event.click_count == 2 {
                                weak.update(cx, |this, cx| {
                                    if let Some(sidebar) = this.sidebar.as_mut() {
                                        sidebar.set_width(None, cx);
                                    }
                                    this.serialize(cx);
                                })
                                .ok();
                                cx.stop_propagation();
                            } else {
                                weak.update(cx, |this, cx| {
                                    this.serialize(cx);
                                })
                                .ok();
                            }
                        })
                        .occlude(),
                );

                div()
                    .id("sidebar-container")
                    .relative()
                    .h_full()
                    .w(sidebar_width)
                    .flex_shrink_0()
                    .child(sidebar_handle.to_any())
                    .child(resize_handle)
                    .into_any_element()
            })
        } else {
            None
        };

        let (left_sidebar, right_sidebar) = if sidebar_on_right {
            (None, sidebar)
        } else {
            (sidebar, None)
        };

        let ui_font = theme_settings::setup_ui_font(window, cx);
        let text_color = cx.theme().colors().text;
        Self::update_traffic_light_position(card_gap, window, cx);

        let workspace = self.workspace().clone();
        let workspace_key_context = workspace.update(cx, |workspace, cx| workspace.key_context(cx));
        let root = workspace.update(cx, |workspace, cx| workspace.actions(h_flex(), window, cx));

        client_side_decorations(
            root.key_context(workspace_key_context)
                .relative()
                .size_full()
                .font(ui_font)
                .text_color(text_color)
                .on_action(cx.listener(Self::close_window))
                .when(self.multi_workspace_enabled(cx), |this| {
                    this.on_action(
                        cx.listener(|this: &mut Self, _: &ToggleSidebar, window, cx| {
                            this.toggle_sidebar(window, cx);
                        }),
                    )
                    .on_action(
                        cx.listener(|this: &mut Self, _: &CloseSidebar, window, cx| {
                            this.close_sidebar_action(window, cx);
                        }),
                    )
                    .on_action(
                        cx.listener(|this: &mut Self, _: &FocusSidebar, window, cx| {
                            this.focus_sidebar(window, cx);
                        }),
                    )
                    .on_action(cx.listener(
                        |this: &mut Self, action: &ToggleThreadSwitcher, window, cx| {
                            if let Some(sidebar) = &this.sidebar {
                                sidebar.toggle_thread_switcher(action.select_last, window, cx);
                            }
                        },
                    ))
                    .on_action(cx.listener(|this: &mut Self, _: &NextProject, window, cx| {
                        if let Some(sidebar) = &this.sidebar {
                            sidebar.cycle_project(true, window, cx);
                        }
                    }))
                    .on_action(
                        cx.listener(|this: &mut Self, _: &PreviousProject, window, cx| {
                            if let Some(sidebar) = &this.sidebar {
                                sidebar.cycle_project(false, window, cx);
                            }
                        }),
                    )
                    .on_action(cx.listener(|this: &mut Self, _: &NextThread, window, cx| {
                        if let Some(sidebar) = &this.sidebar {
                            sidebar.cycle_thread(true, window, cx);
                        }
                    }))
                    .on_action(
                        cx.listener(|this: &mut Self, _: &PreviousThread, window, cx| {
                            if let Some(sidebar) = &this.sidebar {
                                sidebar.cycle_thread(false, window, cx);
                            }
                        }),
                    )
                    .when(self.project_group_keys().len() >= 2, |el| {
                        el.on_action(cx.listener(
                            |this: &mut Self, _: &MoveProjectToNewWindow, window, cx| {
                                let key =
                                    this.project_group_key_for_workspace(this.workspace(), cx);
                                this.open_project_group_in_new_window(&key, window, cx)
                                    .detach_and_log_err(cx);
                            },
                        ))
                    })
                })
                .when(
                    self.sidebar_open() && self.multi_workspace_enabled(cx),
                    |this| {
                        this.on_drag_move(cx.listener(
                            move |this: &mut Self,
                                  e: &DragMoveEvent<DraggedSidebar>,
                                  window,
                                  cx| {
                                if let Some(sidebar) = &this.sidebar {
                                    let new_width = if sidebar_on_right {
                                        window.bounds().size.width - e.event.position.x - card_gap
                                    } else {
                                        e.event.position.x - card_gap
                                    };
                                    sidebar.set_width(Some(Pixels::max(new_width, px(0.0))), cx);
                                }
                            },
                        ))
                    },
                )
                .child(
                    h_flex()
                        .size_full()
                        .flex_1()
                        .overflow_hidden()
                        .bg(cx.theme().colors().background)
                        .p(card_gap)
                        .gap(card_gap)
                        .children(left_sidebar)
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .size_full()
                                .overflow_hidden()
                                .child(self.workspace().clone()),
                        )
                        .children(right_sidebar),
                )
                .child(self.workspace().read(cx).modal_layer.clone())
                .children(self.sidebar_overlay.as_ref().map(|view| {
                    deferred(div().absolute().size_full().inset_0().occlude().child(
                        v_flex().h(px(0.0)).top_20().items_center().child(
                            h_flex().occlude().child(view.clone()).on_mouse_down(
                                MouseButton::Left,
                                |_, _, cx| {
                                    cx.stop_propagation();
                                },
                            ),
                        ),
                    ))
                    .with_priority(2)
                })),
            window,
            cx,
            Tiling::default(),
        )
    }
}

impl MultiWorkspace {
    #[cfg(target_os = "macos")]
    fn update_traffic_light_position(card_gap: Pixels, window: &Window, cx: &App) {
        let offset = if crate::title_bar_visible(cx) {
            px(0.0)
        } else {
            card_gap
        };
        let inset = TRAFFIC_LIGHT_INSET + offset;
        window.set_traffic_light_position(gpui::point(inset, inset));
    }

    #[cfg(not(target_os = "macos"))]
    fn update_traffic_light_position(_card_gap: Pixels, _window: &Window, _cx: &App) {}
}
