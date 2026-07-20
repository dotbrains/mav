use std::sync::atomic::AtomicBool;

use super::*;

impl Focusable for Workspace {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.active_pane.focus_handle(cx)
    }
}

#[derive(Clone)]
pub(crate) struct DraggedDock(pub(crate) DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        static FIRST_PAINT: AtomicBool = AtomicBool::new(true);
        if FIRST_PAINT.swap(false, std::sync::atomic::Ordering::Relaxed) {
            log::info!("Rendered first frame");
        }

        let centered_layout = self.centered_layout
            && self.center.panes().len() == 1
            && self.active_item(cx).is_some();
        let render_padding = |size| {
            (size > 0.0).then(|| {
                div()
                    .h_full()
                    .w(relative(size))
                    .bg(cx.theme().colors().editor_background)
                    .border_color(cx.theme().colors().pane_group_border)
            })
        };
        let paddings = if centered_layout {
            let settings = WorkspaceSettings::get_global(cx).centered_layout;
            (
                render_padding(Self::adjust_padding(
                    settings.left_padding.map(|padding| padding.0),
                )),
                render_padding(Self::adjust_padding(
                    settings.right_padding.map(|padding| padding.0),
                )),
            )
        } else {
            (None, None)
        };
        let ui_font = theme_settings::setup_ui_font(window, cx);

        let theme = cx.theme().clone();
        let colors = theme.colors();
        let status_bar_visible = self.status_bar_visible(cx);
        let notification_entities = self
            .notifications
            .iter()
            .map(|(_, notification)| notification.entity_id())
            .collect::<Vec<_>>();
        let pane_render_context = PaneRenderContext {
            follower_states: &self.follower_states,
            active_call: self.active_call(),
            active_pane: &self.active_pane,
            app_state: &self.app_state,
            project: &self.project,
            workspace: &self.weak_self,
        };

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .font(ui_font)
            .gap_0()
            .justify_start()
            .items_start()
            .text_color(colors.text)
            .overflow_hidden()
            .on_modifiers_changed(move |_, _, cx| {
                for &id in &notification_entities {
                    cx.notify(id);
                }
            })
            .child(
                div()
                    .size_full()
                    .relative()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("workspace")
                            .bg(colors.background)
                            .relative()
                            .flex_1()
                            .w_full()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .border_color(colors.border)
                            .when(status_bar_visible, |this| this.border_b_1())
                            .child({
                                let this = cx.entity();
                                canvas(
                                    move |bounds, window, cx| {
                                        this.update(cx, |this, cx| {
                                            let bounds_changed = this.bounds != bounds;
                                            this.bounds = bounds;

                                            if bounds_changed {
                                                this.left_dock.update(cx, |dock, cx| {
                                                    dock.clamp_panel_size(
                                                        bounds.size.width,
                                                        window,
                                                        cx,
                                                    )
                                                });

                                                this.right_dock.update(cx, |dock, cx| {
                                                    dock.clamp_panel_size(
                                                        bounds.size.width,
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }
                                        })
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            })
                            .when(self.zoomed.is_none(), |this| {
                                this.on_drag_move(cx.listener(
                                    move |workspace, e: &DragMoveEvent<DraggedDock>, window, cx| {
                                        if workspace.previous_dock_drag_coordinates
                                            != Some(e.event.position)
                                        {
                                            workspace.previous_dock_drag_coordinates =
                                                Some(e.event.position);

                                            match e.drag(cx).0 {
                                                DockPosition::Left => {
                                                    workspace.resize_left_dock(
                                                        e.event.position.x
                                                            - workspace.bounds.left(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                                DockPosition::Right => {
                                                    workspace.resize_right_dock(
                                                        workspace.bounds.right()
                                                            - e.event.position.x,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                                DockPosition::Bottom => {}
                                            };
                                            workspace.serialize_workspace(window, cx);
                                        }
                                    },
                                ))
                            })
                            .child(
                                div().flex().flex_row().h_full().child(
                                    h_flex()
                                        .flex_1()
                                        .overflow_hidden()
                                        .when_some(paddings.0, |this, p| this.child(p.border_r_1()))
                                        .child(self.center.render(
                                            self.zoomed.as_ref(),
                                            &pane_render_context,
                                            window,
                                            cx,
                                        ))
                                        .when_some(paddings.1, |this, p| {
                                            this.child(p.border_l_1())
                                        }),
                                ),
                            )
                            .children(self.zoomed.as_ref().and_then(|view| {
                                let zoomed_view = view.upgrade()?;
                                let zoomed_content = match zoomed_view.downcast::<Pane>() {
                                    Ok(pane) => {
                                        pane_group::render_pane_card(pane, &pane_render_context, cx)
                                    }
                                    Err(zoomed_view) => zoomed_view.into_any_element(),
                                };

                                let div = div()
                                    .occlude()
                                    .absolute()
                                    .overflow_hidden()
                                    .bg(colors.background)
                                    .child(zoomed_content)
                                    .inset_0()
                                    .shadow_lg();

                                Some(div)
                            }))
                            .children(self.render_notifications(window, cx)),
                    )
                    .when(status_bar_visible, |parent| {
                        parent.child(self.status_bar.clone())
                    })
                    .child(self.toast_layer.clone()),
            )
    }
}

impl FollowerState {
    pub(super) fn pane(&self) -> &Entity<Pane> {
        self.dock_pane.as_ref().unwrap_or(&self.center_pane)
    }
}
