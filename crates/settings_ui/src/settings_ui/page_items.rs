use super::*;

impl SettingsWindow {
    fn render_sub_page_breadcrumbs(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let scope_name: SharedString = self
            .display_name(&self.current_file)
            .unwrap_or_else(|| self.current_file.setting_type().to_string())
            .into();

        // Only offer scopes in which every sub-page in the stack is available.
        let allowed_mask = self
            .sub_page_stack
            .iter()
            .fold(USER | PROJECT | SERVER, |mask, sub_page| {
                mask & sub_page.link.files
            });
        let allowed_file_indices: Vec<usize> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, (file, _))| allowed_mask.contains(file.mask()))
            .map(|(ix, _)| ix)
            .collect();

        let scope_element = if allowed_file_indices.len() > 1 {
            let this = cx.entity();
            DropdownMenu::new(
                "sub-page-scope-picker",
                scope_name,
                ContextMenu::build(window, cx, move |mut menu, _, _| {
                    menu = menu.header("Scope");

                    for ix in allowed_file_indices {
                        let (file, focus_handle) = &self.files[ix];
                        let display_name = self
                            .display_name(file)
                            .expect("Files should always have a name");

                        menu = menu.toggleable_entry(
                            display_name,
                            file == &self.current_file,
                            IconPosition::End,
                            None,
                            {
                                let this = this.clone();
                                let focus_handle = focus_handle.clone();
                                move |window, cx| {
                                    this.update(cx, |this, cx| {
                                        this.change_file_in_sub_page(ix, window, cx);
                                    });
                                    focus_handle.focus(window, cx);
                                }
                            },
                        );
                    }

                    menu
                }),
            )
            .style(DropdownStyle::Subtle)
            .trigger_tooltip(Tooltip::text("Change Scope"))
            .attach(gpui::Anchor::BottomLeft)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(2.0),
            })
            .tab_index(0)
            .into_any_element()
        } else {
            Label::new(scope_name)
                .color(Color::Muted)
                .into_any_element()
        };

        h_flex()
            .min_w_0()
            .gap_1()
            .overflow_x_hidden()
            .child(scope_element)
            .child(Label::new("/").color(Color::Muted))
            .children(
                itertools::intersperse(
                    std::iter::once(self.current_page().title.into()).chain(
                        self.sub_page_stack
                            .iter()
                            .enumerate()
                            .flat_map(|(index, page)| {
                                (index == 0)
                                    .then(|| page.section_header.clone())
                                    .into_iter()
                                    .chain(std::iter::once(page.link.title.clone()))
                            }),
                    ),
                    "/".into(),
                )
                .map(|item| Label::new(item).color(Color::Muted)),
            )
    }

    fn render_no_results(&self, cx: &App) -> impl IntoElement {
        let search_query = self.search_bar.read(cx).text(cx);

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .child(Label::new("No Results"))
            .child(
                Label::new(format!("No settings match \"{}\"", search_query))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_current_page_items(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement {
        let current_page_index = self.current_page_index();
        let mut page_content = v_flex()
            .id("settings-ui-page")
            .role(Role::Group)
            .aria_label("Settings Content")
            .size_full();

        let has_active_search = !self.search_bar.read(cx).is_empty(cx);
        let has_no_results = self.visible_page_items().next().is_none() && has_active_search;

        if has_no_results {
            page_content = page_content.child(self.render_no_results(cx))
        } else {
            let last_non_header_index = self
                .visible_page_items()
                .filter_map(|(index, item)| {
                    (!matches!(item, SettingsPageItem::SectionHeader(_))).then_some(index)
                })
                .last();

            let root_nav_label = self
                .navbar_entries
                .iter()
                .find(|entry| entry.is_root && entry.page_index == self.current_page_index())
                .map(|entry| entry.title);

            let list_content = list(
                self.list_state.clone(),
                cx.processor(move |this, index, window, cx| {
                    if index == 0 {
                        return div()
                            .px_8()
                            .when(this.sub_page_stack.is_empty(), |this| {
                                this.when_some(root_nav_label, |this, title| {
                                    this.child(
                                        Label::new(title).size(LabelSize::Large).mt_2().mb_3(),
                                    )
                                })
                            })
                            .into_any_element();
                    }

                    let mut visible_items = this.visible_page_items();
                    let Some((actual_item_index, item)) = visible_items.nth(index - 1) else {
                        return gpui::Empty.into_any_element();
                    };

                    let next_is_header = visible_items
                        .next()
                        .map(|(_, item)| matches!(item, SettingsPageItem::SectionHeader(_)))
                        .unwrap_or(false);

                    let is_last = Some(actual_item_index) == last_non_header_index;
                    let is_last_in_section = next_is_header || is_last;

                    let bottom_border = !is_last_in_section;
                    let extra_bottom_padding = is_last_in_section;

                    let item_focus_handle = this.content_handles[current_page_index]
                        [actual_item_index]
                        .focus_handle(cx);

                    v_flex()
                        .id(("settings-page-item", actual_item_index))
                        .track_focus(&item_focus_handle)
                        .w_full()
                        .min_w_0()
                        .child(item.render(
                            this,
                            actual_item_index,
                            bottom_border,
                            extra_bottom_padding,
                            window,
                            cx,
                        ))
                        .into_any_element()
                }),
            );

            page_content = page_content.child(list_content.size_full())
        }
        page_content
    }

    fn render_sub_page_items<'a, Items>(
        &self,
        items: Items,
        scroll_handle: &ScrollHandle,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement
    where
        Items: Iterator<Item = (usize, &'a SettingsPageItem)>,
    {
        let page_content = v_flex()
            .id("settings-ui-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(scroll_handle);
        self.render_sub_page_items_in(page_content, items, false, window, cx)
    }

    fn render_sub_page_items_section<'a, Items>(
        &self,
        items: Items,
        is_inline_section: bool,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement
    where
        Items: Iterator<Item = (usize, &'a SettingsPageItem)>,
    {
        let page_content = v_flex().id("settings-ui-sub-page-section").size_full();
        self.render_sub_page_items_in(page_content, items, is_inline_section, window, cx)
    }

    fn render_sub_page_items_in<'a, Items>(
        &self,
        page_content: Stateful<Div>,
        items: Items,
        is_inline_section: bool,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement
    where
        Items: Iterator<Item = (usize, &'a SettingsPageItem)>,
    {
        let items: Vec<_> = items.collect();
        let items_len = items.len();

        let has_active_search = !self.search_bar.read(cx).is_empty(cx);
        let has_no_results = items_len == 0 && has_active_search;

        if has_no_results {
            page_content.child(self.render_no_results(cx))
        } else {
            let last_non_header_index = items
                .iter()
                .enumerate()
                .rev()
                .find(|(_, (_, item))| !matches!(item, SettingsPageItem::SectionHeader(_)))
                .map(|(index, _)| index);

            let root_nav_label = self
                .navbar_entries
                .iter()
                .find(|entry| entry.is_root && entry.page_index == self.current_page_index())
                .map(|entry| entry.title);

            page_content
                .when(self.sub_page_stack.is_empty(), |this| {
                    this.when_some(root_nav_label, |this, title| {
                        this.child(Label::new(title).size(LabelSize::Large).mt_2().mb_3())
                    })
                })
                .children(items.clone().into_iter().enumerate().map(
                    |(index, (actual_item_index, item))| {
                        let is_last_item = Some(index) == last_non_header_index;
                        let next_is_header = items.get(index + 1).is_some_and(|(_, next_item)| {
                            matches!(next_item, SettingsPageItem::SectionHeader(_))
                        });
                        let bottom_border = !is_inline_section && !next_is_header && !is_last_item;

                        let extra_bottom_padding =
                            !is_inline_section && (next_is_header || is_last_item);

                        v_flex()
                            .w_full()
                            .min_w_0()
                            .id(("settings-page-item", actual_item_index))
                            .child(item.render(
                                self,
                                actual_item_index,
                                bottom_border,
                                extra_bottom_padding,
                                window,
                                cx,
                            ))
                    },
                ))
        }
    }
}
