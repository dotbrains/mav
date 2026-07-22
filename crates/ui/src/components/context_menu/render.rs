use super::*;

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font_size = theme::theme_settings(cx).ui_font_size(cx);
        let window_size = window.viewport_size();
        let rem_size = window.rem_size();
        let is_wide_window = window_size.width / rem_size > rems_from_px(800.).0;

        let mut focus_submenu: Option<FocusHandle> = None;

        let submenu_container = match &mut self.submenu_state {
            SubmenuState::Open(open_submenu) => {
                let is_initializing = open_submenu.offset.is_none();

                let computed_offset = if is_initializing {
                    let menu_bounds = self.main_menu_observed_bounds.get();
                    let trigger_bounds = open_submenu
                        .trigger_bounds
                        .or_else(|| self.submenu_trigger_bounds.get());

                    match (menu_bounds, trigger_bounds) {
                        (Some(menu_bounds), Some(trigger_bounds)) => {
                            Some(trigger_bounds.origin.y - menu_bounds.origin.y)
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(offset) = open_submenu.offset.or(computed_offset) {
                    if open_submenu.offset.is_none() {
                        open_submenu.offset = Some(offset);
                    }

                    focus_submenu = Some(open_submenu.entity.read(cx).focus_handle.clone());
                    Some((
                        open_submenu.item_index,
                        open_submenu.entity.clone(),
                        offset,
                        open_submenu.flip_left,
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };

        let aside = self.documentation_aside.clone();
        let render_aside = |aside: DocumentationAside, cx: &mut Context<Self>| {
            WithRemSize::new(ui_font_size)
                .occlude()
                .elevation_2(cx)
                .w_full()
                .p_2()
                .overflow_hidden()
                .when(is_wide_window, |this| this.max_w_96())
                .when(!is_wide_window, |this| this.max_w_48())
                .child((aside.render)(cx))
        };

        let render_menu = |cx: &mut Context<Self>, window: &mut Window| {
            let bounds_cell = self.main_menu_observed_bounds.clone();
            let menu_bounds_measure = canvas(
                {
                    move |bounds, _window, _cx| {
                        bounds_cell.set(Some(bounds));
                    }
                },
                |_bounds, _state, _window, _cx| {},
            )
            .size_full()
            .absolute()
            .top_0()
            .left_0();

            WithRemSize::new(ui_font_size)
                .occlude()
                .elevation_2(cx)
                .flex()
                .flex_row()
                .flex_shrink_0()
                .child(
                    v_flex()
                        .id("context-menu")
                        .role(Role::Menu)
                        .max_h(vh(0.75, window))
                        .flex_shrink_0()
                        .child(menu_bounds_measure)
                        .when_some(self.fixed_width, |this, width| {
                            this.w(width).overflow_x_hidden()
                        })
                        .when(self.fixed_width.is_none(), |this| {
                            this.min_w(px(200.)).flex_1()
                        })
                        .overflow_y_scroll()
                        .track_focus(&self.focus_handle(cx))
                        .key_context(self.key_context.as_ref())
                        .on_action(cx.listener(ContextMenu::select_first))
                        .on_action(cx.listener(ContextMenu::handle_select_last))
                        .on_action(cx.listener(ContextMenu::select_next))
                        .on_action(cx.listener(ContextMenu::select_previous))
                        .on_action(cx.listener(ContextMenu::select_submenu_child))
                        .on_action(cx.listener(ContextMenu::select_submenu_parent))
                        .on_action(cx.listener(ContextMenu::confirm))
                        .on_action(cx.listener(ContextMenu::secondary_confirm))
                        .on_action(cx.listener(ContextMenu::cancel))
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.hover_target = HoverTarget::MainMenu;
                                if let Some(parent) = &this.main_menu {
                                    parent.update(cx, |parent, _| {
                                        parent.hover_target = HoverTarget::Submenu;
                                    });
                                }
                            }
                        }))
                        .on_mouse_down_out(cx.listener(
                            |this, event: &MouseDownEvent, window, cx| {
                                if matches!(&this.submenu_state, SubmenuState::Open(_)) {
                                    if let Some(padded_bounds) = this.padded_submenu_bounds() {
                                        if padded_bounds.contains(&event.position) {
                                            return;
                                        }
                                    }
                                }

                                if let Some(parent) = &this.main_menu {
                                    let overridden_by_parent_trigger = parent
                                        .read(cx)
                                        .submenu_trigger_bounds
                                        .get()
                                        .is_some_and(|bounds| bounds.contains(&event.position));
                                    if overridden_by_parent_trigger {
                                        return;
                                    }
                                }

                                this.cancel(&menu::Cancel, window, cx)
                            },
                        ))
                        .when_some(self.end_slot_action.as_ref(), |el, action| {
                            el.on_boxed_action(&**action, cx.listener(ContextMenu::end_slot))
                        })
                        .when(!self.delayed, |mut el| {
                            for item in self.items.iter() {
                                if let ContextMenuItem::Entry(ContextMenuEntry {
                                    action: Some(action),
                                    disabled: false,
                                    ..
                                }) = item
                                {
                                    el = el.on_boxed_action(
                                        &**action,
                                        cx.listener(ContextMenu::on_action_dispatch),
                                    );
                                }
                            }
                            el
                        })
                        .child(
                            List::new().children(
                                self.items
                                    .iter()
                                    .enumerate()
                                    .map(|(ix, item)| self.render_menu_item(ix, item, window, cx)),
                            ),
                        ),
                )
        };

        if let Some(focus_handle) = focus_submenu.as_ref() {
            window.focus(focus_handle, cx);
        }

        if is_wide_window {
            let menu_bounds = self.main_menu_observed_bounds.get();
            let trigger_bounds = self
                .documentation_aside
                .as_ref()
                .and_then(|(ix, _)| self.aside_trigger_bounds.borrow().get(ix).copied());

            let trigger_position = match (menu_bounds, trigger_bounds) {
                (Some(menu_bounds), Some(trigger_bounds)) => {
                    let relative_top = trigger_bounds.origin.y - menu_bounds.origin.y;
                    let height = trigger_bounds.size.height;
                    Some((relative_top, height))
                }
                _ => None,
            };

            div()
                .relative()
                .child(render_menu(cx, window))
                // Only render the aside once we have trigger bounds to avoid flicker.
                .when_some(trigger_position, |this, (top, height)| {
                    this.children(aside.map(|(_, aside)| {
                        h_flex()
                            .absolute()
                            .when(aside.side == DocumentationSide::Left, |el| {
                                el.right_full().mr_1()
                            })
                            .when(aside.side == DocumentationSide::Right, |el| {
                                el.left_full().ml_1()
                            })
                            .top(top)
                            .h(height)
                            .child(render_aside(aside, cx))
                    }))
                })
                .when_some(
                    submenu_container,
                    |this, (ix, submenu, offset, flip_left)| {
                        this.child(
                            self.render_submenu_container(ix, submenu, offset, flip_left, cx),
                        )
                    },
                )
        } else {
            v_flex()
                .w_full()
                .relative()
                .gap_1()
                .justify_end()
                .children(aside.map(|(_, aside)| render_aside(aside, cx)))
                .child(render_menu(cx, window))
                .when_some(
                    submenu_container,
                    |this, (ix, submenu, offset, flip_left)| {
                        this.child(
                            self.render_submenu_container(ix, submenu, offset, flip_left, cx),
                        )
                    },
                )
        }
    }
}
