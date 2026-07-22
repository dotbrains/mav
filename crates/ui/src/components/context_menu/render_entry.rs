use super::*;

impl ContextMenu {
    pub(crate) fn render_menu_entry(
        &self,
        ix: usize,
        entry: &ContextMenuEntry,
        is_active_descendant: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ContextMenuEntry {
            toggle,
            label,
            handler,
            icon,
            custom_icon_path,
            custom_icon_svg,
            icon_position,
            icon_size,
            icon_color,
            action,
            disabled,
            documentation_aside,
            end_slot_icon,
            end_slot_title,
            end_slot_handler,
            show_end_slot_on_hover,
            secondary_handler: _,
        } = entry;
        let this = cx.weak_entity();

        let handler = handler.clone();
        let menu = cx.entity().downgrade();

        let icon_color = if *disabled {
            Color::Muted
        } else if toggle.is_some() {
            icon_color.unwrap_or(Color::Accent)
        } else {
            icon_color.unwrap_or(Color::Default)
        };

        let label_color = if *disabled {
            Color::Disabled
        } else {
            Color::Default
        };

        let label_element = if let Some(custom_path) = custom_icon_path {
            h_flex()
                .gap_1p5()
                .when(
                    *icon_position == IconPosition::Start && toggle.is_none(),
                    |flex| {
                        flex.child(
                            Icon::from_path(custom_path.clone())
                                .size(*icon_size)
                                .color(icon_color),
                        )
                    },
                )
                .child(Label::new(label.clone()).color(label_color).truncate())
                .when(*icon_position == IconPosition::End, |flex| {
                    flex.child(
                        Icon::from_path(custom_path.clone())
                            .size(*icon_size)
                            .color(icon_color),
                    )
                })
                .into_any_element()
        } else if let Some(custom_icon_svg) = custom_icon_svg {
            h_flex()
                .gap_1p5()
                .when(
                    *icon_position == IconPosition::Start && toggle.is_none(),
                    |flex| {
                        flex.child(
                            Icon::from_external_svg(custom_icon_svg.clone())
                                .size(*icon_size)
                                .color(icon_color),
                        )
                    },
                )
                .child(Label::new(label.clone()).color(label_color).truncate())
                .when(*icon_position == IconPosition::End, |flex| {
                    flex.child(
                        Icon::from_external_svg(custom_icon_svg.clone())
                            .size(*icon_size)
                            .color(icon_color),
                    )
                })
                .into_any_element()
        } else if let Some(icon_name) = icon {
            h_flex()
                .gap_1p5()
                .when(
                    *icon_position == IconPosition::Start && toggle.is_none(),
                    |flex| flex.child(Icon::new(*icon_name).size(*icon_size).color(icon_color)),
                )
                .child(Label::new(label.clone()).color(label_color).truncate())
                .when(*icon_position == IconPosition::End, |flex| {
                    flex.child(Icon::new(*icon_name).size(*icon_size).color(icon_color))
                })
                .into_any_element()
        } else {
            Label::new(label.clone())
                .color(label_color)
                .truncate()
                .into_any_element()
        };

        let aside_trigger_bounds = self.aside_trigger_bounds.clone();

        div()
            .id(("context-menu-child", ix))
            .when_some(documentation_aside.clone(), |this, documentation_aside| {
                this.occlude()
                    .on_hover(cx.listener(move |menu, hovered, _, cx| {
                        if *hovered {
                            menu.documentation_aside = Some((ix, documentation_aside.clone()));
                        } else if matches!(menu.documentation_aside, Some((id, _)) if id == ix) {
                            menu.documentation_aside = None;
                        }
                        cx.notify();
                    }))
            })
            .when(documentation_aside.is_some(), |this| {
                this.child(
                    canvas(
                        {
                            let aside_trigger_bounds = aside_trigger_bounds.clone();
                            move |bounds, _window, _cx| {
                                aside_trigger_bounds.borrow_mut().insert(ix, bounds);
                            }
                        },
                        |_bounds, _state, _window, _cx| {},
                    )
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0(),
                )
            })
            .child(
                ListItem::new(ix)
                    .group_name("label_container")
                    .inset(true)
                    .disabled(*disabled)
                    .aria_role(if toggle.is_some() {
                        Role::MenuItemCheckBox
                    } else {
                        Role::MenuItem
                    })
                    .when(is_active_descendant, |item| item.aria_active_descendant())
                    .aria_label(label.clone())
                    .toggle_state(Some(ix) == self.selected_index)
                    .when(self.main_menu.is_none() && !*disabled, |item| {
                        item.on_hover(cx.listener(move |this, hovered, window, cx| {
                            if *hovered {
                                this.clear_selected();
                                window.focus(&this.focus_handle.clone(), cx);

                                if let SubmenuState::Open(open_submenu) = &this.submenu_state {
                                    if open_submenu.item_index != ix {
                                        this.close_submenu(false, cx);
                                        cx.notify();
                                    }
                                }
                            }
                        }))
                    })
                    .when(self.main_menu.is_some(), |item| {
                        item.on_click(cx.listener(move |this, _, window, cx| {
                            if matches!(
                                &this.submenu_state,
                                SubmenuState::Open(open_submenu) if open_submenu.item_index == ix
                            ) {
                                return;
                            }

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
                        }))
                        .on_hover(cx.listener(
                            move |this, hovered, window, cx| {
                                if *hovered {
                                    this.clear_selected();
                                    cx.notify();
                                }

                                if let Some(parent) = &this.main_menu {
                                    let mouse_pos = window.mouse_position();
                                    let parent_clone = parent.clone();

                                    if *hovered {
                                        parent.update(cx, |parent, _| {
                                            parent.clear_selected();
                                            parent.hover_target = HoverTarget::Submenu;
                                        });
                                    } else {
                                        parent_clone.update(cx, |parent, cx| {
                                            if matches!(
                                                &parent.submenu_state,
                                                SubmenuState::Open(_)
                                            ) {
                                                // Only close if mouse is to the left of the safety threshold
                                                // (prevents accidental close when moving diagonally toward submenu)
                                                let should_close = parent
                                                    .submenu_safety_threshold_x
                                                    .map(|threshold_x| mouse_pos.x < threshold_x)
                                                    .unwrap_or(true);

                                                if should_close {
                                                    parent.close_submenu(true, cx);
                                                }
                                            }
                                        });
                                    }
                                }
                            },
                        ))
                    })
                    .when_some(*toggle, |list_item, (position, toggled)| {
                        let contents = div()
                            .flex_none()
                            .child(
                                Icon::new(icon.unwrap_or(IconName::Check))
                                    .color(icon_color)
                                    .size(*icon_size),
                            )
                            .when(!toggled, |contents| contents.invisible());

                        match position {
                            IconPosition::Start => list_item.start_slot(contents),
                            IconPosition::End => list_item.end_slot(contents),
                        }
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(label_element)
                            .debug_selector(|| format!("MENU_ITEM-{}", label))
                            .children(action.as_ref().map(|action| {
                                let binding = self
                                    .action_context
                                    .as_ref()
                                    .map(|focus| KeyBinding::for_action_in(&**action, focus, cx))
                                    .unwrap_or_else(|| KeyBinding::for_action(&**action, cx));

                                div()
                                    .ml_4()
                                    .child(binding.disabled(*disabled))
                                    .when(*disabled && documentation_aside.is_some(), |parent| {
                                        parent.invisible()
                                    })
                            }))
                            .when(*disabled && documentation_aside.is_some(), |parent| {
                                parent.child(
                                    Icon::new(IconName::Info)
                                        .size(IconSize::XSmall)
                                        .color(Color::Muted),
                                )
                            }),
                    )
                    .when_some(
                        end_slot_icon
                            .as_ref()
                            .zip(self.end_slot_action.as_ref())
                            .zip(end_slot_title.as_ref())
                            .zip(end_slot_handler.as_ref()),
                        |el, (((icon, action), title), handler)| {
                            el.end_slot({
                                let icon_button = IconButton::new("end-slot-icon", *icon)
                                    .shape(IconButtonShape::Square)
                                    .style(ButtonStyle::Subtle)
                                    .tooltip({
                                        let action_context = self.action_context.clone();
                                        let title = title.clone();
                                        let action = action.boxed_clone();
                                        move |_window, cx| {
                                            action_context
                                                .as_ref()
                                                .map(|focus| {
                                                    Tooltip::for_action_in(
                                                        title.clone(),
                                                        &*action,
                                                        focus,
                                                        cx,
                                                    )
                                                })
                                                .unwrap_or_else(|| {
                                                    Tooltip::for_action(title.clone(), &*action, cx)
                                                })
                                        }
                                    })
                                    .on_click({
                                        let handler = handler.clone();
                                        move |_, window, cx| {
                                            handler(None, window, cx);
                                            this.update(cx, |this, cx| {
                                                this.rebuild(window, cx);
                                                cx.notify();
                                            })
                                            .ok();
                                        }
                                    });

                                if *show_end_slot_on_hover {
                                    div()
                                        .visible_on_hover("label_container")
                                        .child(icon_button)
                                        .into_any_element()
                                } else {
                                    icon_button.into_any_element()
                                }
                            })
                        },
                    )
                    .on_click({
                        let context = self.action_context.clone();
                        let keep_open_on_confirm = self.keep_open_on_confirm;
                        move |_, window, cx| {
                            handler(context.as_ref(), window, cx);
                            menu.update(cx, |menu, cx| {
                                menu.clicked = true;
                                if keep_open_on_confirm {
                                    menu.rebuild(window, cx);
                                } else {
                                    cx.emit(DismissEvent);
                                }
                            })
                            .ok();
                        }
                    }),
            )
            .into_any_element()
    }
}
