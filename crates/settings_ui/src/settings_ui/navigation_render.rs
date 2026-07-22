use super::*;

impl SettingsWindow {
    fn render_nav(
        &self,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement {
        let visible_count = self.visible_navbar_entries().count();

        let focus_keybind_label = if self
            .navbar_focus_handle
            .read(cx)
            .handle
            .contains_focused(window, cx)
            || self
                .visible_navbar_entries()
                .any(|(_, entry)| entry.focus_handle.is_focused(window))
        {
            "Focus Content"
        } else {
            "Focus Navbar"
        };

        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("NavigationMenu");
        key_context.add("menu");
        if self.search_bar.focus_handle(cx).is_focused(window) {
            key_context.add("search");
        }

        v_flex()
            .key_context(key_context)
            .on_action(cx.listener(|this, _: &CollapseNavEntry, window, cx| {
                let Some(focused_entry) = this.focused_nav_entry(window, cx) else {
                    return;
                };
                let focused_entry_parent = this.root_entry_containing(focused_entry);
                if this.navbar_entries[focused_entry_parent].expanded {
                    this.toggle_navbar_entry(focused_entry_parent);
                    window.focus(&this.navbar_entries[focused_entry_parent].focus_handle, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ExpandNavEntry, window, cx| {
                let Some(focused_entry) = this.focused_nav_entry(window, cx) else {
                    return;
                };
                if !this.navbar_entries[focused_entry].is_root {
                    return;
                }
                if !this.navbar_entries[focused_entry].expanded {
                    this.toggle_navbar_entry(focused_entry);
                }
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &FocusPreviousRootNavEntry, window, cx| {
                    let entry_index = this
                        .focused_nav_entry(window, cx)
                        .unwrap_or(this.navbar_entry);
                    let mut root_index = None;
                    for (index, entry) in this.visible_navbar_entries() {
                        if index >= entry_index {
                            break;
                        }
                        if entry.is_root {
                            root_index = Some(index);
                        }
                    }
                    let Some(previous_root_index) = root_index else {
                        return;
                    };
                    this.focus_and_scroll_to_nav_entry(previous_root_index, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &FocusNextRootNavEntry, window, cx| {
                let entry_index = this
                    .focused_nav_entry(window, cx)
                    .unwrap_or(this.navbar_entry);
                let mut root_index = None;
                for (index, entry) in this.visible_navbar_entries() {
                    if index <= entry_index {
                        continue;
                    }
                    if entry.is_root {
                        root_index = Some(index);
                        break;
                    }
                }
                let Some(next_root_index) = root_index else {
                    return;
                };
                this.focus_and_scroll_to_nav_entry(next_root_index, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusFirstNavEntry, window, cx| {
                if let Some((first_entry_index, _)) = this.visible_navbar_entries().next() {
                    this.focus_and_scroll_to_nav_entry(first_entry_index, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusLastNavEntry, window, cx| {
                if let Some((last_entry_index, _)) = this.visible_navbar_entries().last() {
                    this.focus_and_scroll_to_nav_entry(last_entry_index, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusNextNavEntry, window, cx| {
                let entry_index = this
                    .focused_nav_entry(window, cx)
                    .unwrap_or(this.navbar_entry);
                let mut next_index = None;
                for (index, _) in this.visible_navbar_entries() {
                    if index > entry_index {
                        next_index = Some(index);
                        break;
                    }
                }
                let Some(next_entry_index) = next_index else {
                    return;
                };
                this.open_and_scroll_to_navbar_entry(
                    next_entry_index,
                    Some(gpui::ScrollStrategy::Bottom),
                    false,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &FocusPreviousNavEntry, window, cx| {
                let entry_index = this
                    .focused_nav_entry(window, cx)
                    .unwrap_or(this.navbar_entry);
                let mut prev_index = None;
                for (index, _) in this.visible_navbar_entries() {
                    if index >= entry_index {
                        break;
                    }
                    prev_index = Some(index);
                }
                let Some(prev_entry_index) = prev_index else {
                    return;
                };
                this.open_and_scroll_to_navbar_entry(
                    prev_entry_index,
                    Some(gpui::ScrollStrategy::Top),
                    false,
                    window,
                    cx,
                );
            }))
            .w_56()
            .h_full()
            .p_2p5()
            .when(cfg!(target_os = "macos"), |this| this.pt_10())
            .flex_none()
            .border_r_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().panel_background)
            .child(self.render_search(window, cx))
            .child(
                v_flex()
                    .id("settings-ui-nav")
                    .role(Role::Tree)
                    .aria_label("Settings Navigation")
                    .flex_1()
                    .overflow_hidden()
                    .track_focus(&self.navbar_focus_handle.focus_handle(cx))
                    .tab_group()
                    .tab_index(NAVBAR_GROUP_TAB_INDEX)
                    .child(
                        uniform_list(
                            "settings-ui-nav-bar",
                            visible_count + 1,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                this.visible_navbar_entries()
                                    .skip(range.start.saturating_sub(1))
                                    .take(range.len())
                                    .map(|(entry_index, entry)| {
                                        TreeViewItem::new(
                                            ("settings-ui-navbar-entry", entry_index),
                                            entry.title,
                                        )
                                        .track_focus(&entry.focus_handle)
                                        .root_item(entry.is_root)
                                        .toggle_state(this.is_navbar_entry_selected(entry_index))
                                        .when(entry.is_root, |item| {
                                            item.expanded(entry.expanded || this.has_query)
                                                .on_toggle(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.toggle_and_focus_navbar_entry(
                                                            entry_index,
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                        })
                                        .on_click({
                                            let category = this.pages[entry.page_index].title;
                                            let subcategory =
                                                (!entry.is_root).then_some(entry.title);

                                            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                if this.toggle_navbar_entry_on_double_click(
                                                        entry_index,
                                                        event,
                                                        window,
                                                        cx,
                                                    )
                                                {
                                                    return;
                                                }

                                                telemetry::event!(
                                                    "Settings Navigation Clicked",
                                                    category = category,
                                                    subcategory = subcategory
                                                );

                                                this.open_and_scroll_to_navbar_entry(
                                                    entry_index,
                                                    None,
                                                    true,
                                                    window,
                                                    cx,
                                                );
                                            })
                                        })
                                    })
                                    .collect()
                            }),
                        )
                        .size_full()
                        .track_scroll(&self.navbar_scroll_handle),
                    )
                    .vertical_scrollbar_for(&self.navbar_scroll_handle, window, cx),
            )
            .child(
                h_flex()
                    .w_full()
                    .h_8()
                    .p_2()
                    .pb_0p5()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        KeybindingHint::new(
                            KeyBinding::for_action_in(
                                &ToggleFocusNav,
                                &self.navbar_focus_handle.focus_handle(cx),
                                cx,
                            ),
                            cx.theme().colors().surface_background.opacity(0.5),
                        )
                        .suffix(focus_keybind_label),
                    ),
            )
    }

    fn open_and_scroll_to_navbar_entry(
        &mut self,
        navbar_entry_index: usize,
        scroll_strategy: Option<gpui::ScrollStrategy>,
        focus_content: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_navbar_entry_page(navbar_entry_index);
        cx.notify();

        let mut handle_to_focus = None;

        if self.navbar_entries[navbar_entry_index].is_root
            || !self.is_nav_entry_visible(navbar_entry_index)
        {
            if let Some(scroll_handle) = self.current_sub_page_scroll_handle() {
                scroll_handle.set_offset(point(px(0.), px(0.)));
            }

            if focus_content {
                let Some(first_item_index) =
                    self.visible_page_items().next().map(|(index, _)| index)
                else {
                    return;
                };
                handle_to_focus = Some(self.focus_handle_for_content_element(first_item_index, cx));
            } else if !self.is_nav_entry_visible(navbar_entry_index) {
                let Some(first_visible_nav_entry_index) =
                    self.visible_navbar_entries().next().map(|(index, _)| index)
                else {
                    return;
                };
                self.focus_and_scroll_to_nav_entry(first_visible_nav_entry_index, window, cx);
            } else {
                handle_to_focus =
                    Some(self.navbar_entries[navbar_entry_index].focus_handle.clone());
            }
        } else {
            let entry_item_index = self.navbar_entries[navbar_entry_index]
                .item_index
                .expect("Non-root items should have an item index");
            self.scroll_to_content_item(entry_item_index, window, cx);
            if focus_content {
                handle_to_focus = Some(self.focus_handle_for_content_element(entry_item_index, cx));
            } else {
                handle_to_focus =
                    Some(self.navbar_entries[navbar_entry_index].focus_handle.clone());
            }
        }

        if let Some(scroll_strategy) = scroll_strategy
            && let Some(logical_entry_index) = self
                .visible_navbar_entries()
                .into_iter()
                .position(|(index, _)| index == navbar_entry_index)
        {
            self.navbar_scroll_handle
                .scroll_to_item(logical_entry_index + 1, scroll_strategy);
        }

        // Page scroll handle updates the active item index
        // in it's next paint call after using scroll_handle.scroll_to_top_of_item
        // The call after that updates the offset of the scroll handle. So to
        // ensure the scroll handle doesn't lag behind we need to render three frames
        // back to back.
        cx.on_next_frame(window, move |_, window, cx| {
            if let Some(handle) = handle_to_focus.as_ref() {
                window.focus(handle, cx);
            }

            cx.on_next_frame(window, |_, _, cx| {
                cx.notify();
            });
            cx.notify();
        });
        cx.notify();
    }

    fn scroll_to_content_item(
        &self,
        content_item_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self
            .visible_page_items()
            .position(|(index, _)| index == content_item_index)
            .unwrap_or(0);
        if index == 0 {
            if let Some(scroll_handle) = self.current_sub_page_scroll_handle() {
                scroll_handle.set_offset(point(px(0.), px(0.)));
            }

            self.list_state.scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            return;
        }
        self.list_state.scroll_to(gpui::ListOffset {
            item_ix: index + 1,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    fn is_nav_entry_visible(&self, nav_entry_index: usize) -> bool {
        self.visible_navbar_entries()
            .any(|(index, _)| index == nav_entry_index)
    }

    fn focus_and_scroll_to_first_visible_nav_entry(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(nav_entry_index) = self.visible_navbar_entries().next().map(|(index, _)| index)
        {
            self.focus_and_scroll_to_nav_entry(nav_entry_index, window, cx);
        }
    }

    fn focus_and_scroll_to_nav_entry(
        &self,
        nav_entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self
            .visible_navbar_entries()
            .position(|(index, _)| index == nav_entry_index)
        else {
            return;
        };
        self.navbar_scroll_handle
            .scroll_to_item(position, gpui::ScrollStrategy::Top);
        window.focus(&self.navbar_entries[nav_entry_index].focus_handle, cx);
        cx.notify();
    }

    fn current_sub_page_scroll_handle(&self) -> Option<&ScrollHandle> {
        self.sub_page_stack.last().map(|page| &page.scroll_handle)
    }

    fn visible_page_items(&self) -> impl Iterator<Item = (usize, &SettingsPageItem)> {
        let page_idx = self.current_page_index();

        self.current_page()
            .items
            .iter()
            .enumerate()
            .filter(move |&(item_index, _)| self.filter_table[page_idx][item_index])
    }
}
