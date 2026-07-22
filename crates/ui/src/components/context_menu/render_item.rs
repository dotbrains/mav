use super::*;

impl ContextMenu {
    pub(crate) fn render_menu_item(
        &self,
        ix: usize,
        item: &ContextMenuItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // The menu keeps real focus on its container, so for assistive
        // technology to track the selected item we report it as the active
        // descendant. GPUI only honors this while the menu actually holds
        // focus, so we mark the selected item unconditionally here.
        let is_active_descendant = |selectable: bool| selectable && Some(ix) == self.selected_index;
        match item {
            ContextMenuItem::Separator => ListSeparator.into_any_element(),
            ContextMenuItem::Header(header) => ListSubHeader::new(header.clone())
                .inset(true)
                .into_any_element(),
            ContextMenuItem::HeaderWithLink(header, label, url) => {
                let url = url.clone();
                let link_id = ElementId::Name(format!("link-{}", url).into());
                ListSubHeader::new(header.clone())
                    .inset(true)
                    .end_slot(
                        Button::new(link_id, label.clone())
                            .color(Color::Muted)
                            .label_size(LabelSize::Small)
                            .size(ButtonSize::None)
                            .style(ButtonStyle::Transparent)
                            .on_click(move |_, _, cx| {
                                let url = url.clone();
                                cx.open_url(&url);
                            })
                            .into_any_element(),
                    )
                    .into_any_element()
            }
            ContextMenuItem::Label(label) => ListItem::new(ix)
                .inset(true)
                .disabled(true)
                .child(Label::new(label.clone()))
                .into_any_element(),
            ContextMenuItem::Entry(entry) => self
                .render_menu_entry(ix, entry, is_active_descendant(true), cx)
                .into_any_element(),
            ContextMenuItem::CustomEntry {
                entry_render,
                handler,
                selectable,
                documentation_aside,
                ..
            } => {
                let handler = handler.clone();
                let menu = cx.entity().downgrade();
                let selectable = *selectable;
                let aside_trigger_bounds = self.aside_trigger_bounds.clone();

                div()
                    .id(("context-menu-child", ix))
                    .when_some(documentation_aside.clone(), |this, documentation_aside| {
                        this.occlude()
                            .on_hover(cx.listener(move |menu, hovered, _, cx| {
                            if *hovered {
                                menu.documentation_aside = Some((ix, documentation_aside.clone()));
                            } else if matches!(menu.documentation_aside, Some((id, _)) if id == ix)
                            {
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
                            .inset(true)
                            .when(selectable, |item| item.aria_role(Role::MenuItem))
                            .when(is_active_descendant(selectable), |item| {
                                item.aria_active_descendant()
                            })
                            .toggle_state(Some(ix) == self.selected_index)
                            .selectable(selectable)
                            .when(selectable, |item| {
                                item.on_click({
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
                                })
                            })
                            .child(entry_render(window, cx)),
                    )
                    .into_any_element()
            }
            ContextMenuItem::Submenu {
                label,
                icon,
                icon_color,
                ..
            } => self
                .render_submenu_item_trigger(
                    ix,
                    label.clone(),
                    *icon,
                    *icon_color,
                    is_active_descendant(true),
                    cx,
                )
                .into_any_element(),
        }
    }
}
