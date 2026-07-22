use super::*;

impl ContextMenu {
    pub(crate) fn render_submenu_item_trigger(
        &self,
        ix: usize,
        label: SharedString,
        icon: Option<IconName>,
        icon_color: Option<Color>,
        is_active_descendant: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let toggle_state = Some(ix) == self.selected_index
            || matches!(
                &self.submenu_state,
                SubmenuState::Open(open_submenu) if open_submenu.item_index == ix
            );

        div()
            .id(("context-menu-submenu-trigger", ix))
            .capture_any_mouse_down(cx.listener(move |this, event: &MouseDownEvent, _, _| {
                // This prevents on_hover(false) from closing the submenu during a click.
                if event.button == MouseButton::Left {
                    this.submenu_trigger_mouse_down = true;
                }
            }))
            .capture_any_mouse_up(cx.listener(move |this, event: &MouseUpEvent, _, _| {
                if event.button == MouseButton::Left {
                    this.submenu_trigger_mouse_down = false;
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if matches!(&this.submenu_state, SubmenuState::Open(_))
                    || this.selected_index == Some(ix)
                {
                    this.submenu_safety_threshold_x = Some(event.position.x - px(100.0));
                }

                cx.notify();
            }))
            .child(
                ListItem::new(ix)
                    .inset(true)
                    .aria_role(Role::MenuItem)
                    .when(is_active_descendant, |item| item.aria_active_descendant())
                    .aria_label(label.clone())
                    .toggle_state(toggle_state)
                    .child(
                        canvas(
                            {
                                let trigger_bounds_cell = self.submenu_trigger_bounds.clone();
                                move |bounds, _window, _cx| {
                                    if toggle_state {
                                        trigger_bounds_cell.set(Some(bounds));
                                    }
                                }
                            },
                            |_bounds, _state, _window, _cx| {},
                        )
                        .size_full()
                        .absolute()
                        .top_0()
                        .left_0(),
                    )
                    .on_hover(cx.listener(move |this, hovered, window, cx| {
                        let mouse_pos = window.mouse_position();

                        if *hovered {
                            this.clear_selected();
                            window.focus(&this.focus_handle.clone(), cx);
                            this.hover_target = HoverTarget::MainMenu;
                            this.submenu_safety_threshold_x = Some(mouse_pos.x - px(50.0));

                            if let Some(ContextMenuItem::Submenu { builder, .. }) =
                                this.items.get(ix)
                            {
                                this.open_submenu(
                                    ix,
                                    builder.clone(),
                                    SubmenuOpenTrigger::Pointer,
                                    window,
                                    cx,
                                );
                            }

                            cx.notify();
                        } else {
                            if this.submenu_trigger_mouse_down {
                                return;
                            }

                            let is_open_for_this_item = matches!(
                                &this.submenu_state,
                                SubmenuState::Open(open_submenu) if open_submenu.item_index == ix
                            );

                            let mouse_in_submenu_zone = this
                                .padded_submenu_bounds()
                                .is_some_and(|bounds| bounds.contains(&window.mouse_position()));

                            if is_open_for_this_item
                                && this.hover_target != HoverTarget::Submenu
                                && !mouse_in_submenu_zone
                            {
                                this.close_submenu(false, cx);
                                this.clear_selected();
                                window.focus(&this.focus_handle.clone(), cx);
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if matches!(
                            &this.submenu_state,
                            SubmenuState::Open(open_submenu) if open_submenu.item_index == ix
                        ) {
                            return;
                        }

                        if let Some(ContextMenuItem::Submenu { builder, .. }) = this.items.get(ix) {
                            this.open_submenu(
                                ix,
                                builder.clone(),
                                SubmenuOpenTrigger::Pointer,
                                window,
                                cx,
                            );
                        }
                    }))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .when_some(icon, |this, icon_name| {
                                        this.child(
                                            Icon::new(icon_name)
                                                .size(IconSize::Small)
                                                .color(icon_color.unwrap_or(Color::Muted)),
                                        )
                                    })
                                    .child(Label::new(label).color(Color::Default)),
                            )
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            ),
                    ),
            )
    }

    pub(crate) fn padded_submenu_bounds(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.main_menu_observed_bounds.get()?;
        Some(Bounds {
            origin: Point {
                x: bounds.origin.x - px(50.0),
                y: bounds.origin.y - px(50.0),
            },
            size: Size {
                width: bounds.size.width + px(100.0),
                height: bounds.size.height + px(100.0),
            },
        })
    }

    pub(crate) fn render_submenu_container(
        &self,
        ix: usize,
        submenu: Entity<ContextMenu>,
        offset: Pixels,
        flip_left: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bounds_cell = self.main_menu_observed_bounds.clone();
        let canvas = canvas(
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

        div()
            .id(("submenu-container", ix))
            .absolute()
            .top(offset)
            .when(flip_left, |this| this.right_full().mr_neg_0p5())
            .when(!flip_left, |this| this.left_full().ml_neg_0p5())
            .on_hover(cx.listener(|this, hovered, _, _| {
                if *hovered {
                    this.hover_target = HoverTarget::Submenu;
                }
            }))
            .child(
                anchored()
                    .anchor(if flip_left {
                        Anchor::TopRight
                    } else {
                        Anchor::TopLeft
                    })
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .id(("submenu-hover-zone", ix))
                            .occlude()
                            .child(canvas)
                            .child(submenu),
                    ),
            )
    }
}
