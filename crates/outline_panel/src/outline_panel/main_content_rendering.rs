use super::*;

impl OutlinePanel {
    pub(super) fn render_main_contents(
        &mut self,
        query: Option<String>,
        show_indent_guides: bool,
        indent_size: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let contents = if self.cached_entries.is_empty() {
            let header = if query.is_some() {
                "No matches for query"
            } else {
                "No outlines available"
            };

            v_flex()
                .id("empty-outline-state")
                .gap_0p5()
                .flex_1()
                .justify_center()
                .size_full()
                .child(h_flex().justify_center().child(Label::new(header)))
                .when_some(query, |panel, query| {
                    panel.child(
                        h_flex()
                            .px_0p5()
                            .justify_center()
                            .bg(cx.theme().colors().element_selected.opacity(0.2))
                            .child(Label::new(query)),
                    )
                })
                .child(
                    h_flex()
                        .gap_1()
                        .justify_center()
                        .child(Label::new("Toggle Panel With").color(Color::Muted))
                        .when_some(
                            match self.position(window, cx) {
                                DockPosition::Left => Some(
                                    KeyBinding::for_action(&workspace::ToggleSidebar, cx)
                                        .into_any_element(),
                                ),
                                DockPosition::Bottom => None,
                                DockPosition::Right => Some(
                                    KeyBinding::for_action(&workspace::ToggleProjectPane, cx)
                                        .into_any_element(),
                                ),
                            },
                            |this, key_binding| this.child(key_binding),
                        ),
                )
        } else {
            let list_contents = {
                let items_len = self.cached_entries.len();
                let multi_buffer_snapshot = self
                    .active_editor()
                    .map(|editor| editor.read(cx).buffer().read(cx).snapshot(cx));
                uniform_list(
                    "entries",
                    items_len,
                    cx.processor(move |outline_panel, range: Range<usize>, window, cx| {
                        outline_panel.rendered_entries_len = range.end - range.start;
                        let entries = outline_panel.cached_entries.get(range);
                        entries
                            .map(|entries| entries.to_vec())
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|cached_entry| match cached_entry.entry {
                                PanelEntry::Fs(entry) => Some(outline_panel.render_entry(
                                    &entry,
                                    cached_entry.depth,
                                    cached_entry.string_match.as_ref(),
                                    window,
                                    cx,
                                )),
                                PanelEntry::FoldedDirs(folded_dirs_entry) => {
                                    Some(outline_panel.render_folded_dirs(
                                        &folded_dirs_entry,
                                        cached_entry.depth,
                                        cached_entry.string_match.as_ref(),
                                        window,
                                        cx,
                                    ))
                                }
                                PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                                    outline_panel.render_excerpt(
                                        &excerpt,
                                        cached_entry.depth,
                                        window,
                                        cx,
                                    )
                                }
                                PanelEntry::Outline(OutlineEntry::Outline(entry)) => {
                                    Some(outline_panel.render_outline(
                                        &entry,
                                        cached_entry.depth,
                                        cached_entry.string_match.as_ref(),
                                        window,
                                        cx,
                                    ))
                                }
                                PanelEntry::Search(SearchEntry {
                                    match_range,
                                    render_data,
                                    kind,
                                    ..
                                }) => outline_panel.render_search_match(
                                    multi_buffer_snapshot.as_ref(),
                                    &match_range,
                                    &render_data,
                                    kind,
                                    cached_entry.depth,
                                    cached_entry.string_match.as_ref(),
                                    window,
                                    cx,
                                ),
                            })
                            .collect()
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(self.max_width_item_index)
                .track_scroll(&self.scroll_handle)
                .when(show_indent_guides, |list| {
                    list.with_decoration(
                        ui::indent_guides(px(indent_size), IndentGuideColors::panel(cx))
                            .with_compute_indents_fn(cx.entity(), |outline_panel, range, _, _| {
                                let entries = outline_panel.cached_entries.get(range);
                                if let Some(entries) = entries {
                                    entries.iter().map(|item| item.depth).collect()
                                } else {
                                    smallvec::SmallVec::new()
                                }
                            })
                            .with_render_fn(cx.entity(), move |outline_panel, params, _, _| {
                                const LEFT_OFFSET: Pixels = px(14.);

                                let indent_size = params.indent_size;
                                let item_height = params.item_height;
                                let active_indent_guide_ix = find_active_indent_guide_ix(
                                    outline_panel,
                                    &params.indent_guides,
                                );

                                params
                                    .indent_guides
                                    .into_iter()
                                    .enumerate()
                                    .map(|(ix, layout)| {
                                        let bounds = Bounds::new(
                                            point(
                                                layout.offset.x * indent_size + LEFT_OFFSET,
                                                layout.offset.y * item_height,
                                            ),
                                            size(px(1.), layout.length * item_height),
                                        );
                                        ui::RenderedIndentGuide {
                                            bounds,
                                            layout,
                                            is_active: active_indent_guide_ix == Some(ix),
                                            hitbox: None,
                                        }
                                    })
                                    .collect()
                            }),
                    )
                })
            };

            v_flex()
                .flex_shrink_1()
                .size_full()
                .child(list_contents.size_full().flex_shrink_1())
                .custom_scrollbars(
                    Scrollbars::for_settings::<OutlinePanelSettingsScrollbarProxy>()
                        .tracked_scroll_handle(&self.scroll_handle.clone())
                        .with_track_along(
                            ScrollAxes::Horizontal,
                            cx.theme().colors().editor_background,
                        )
                        .tracked_entity(cx.entity_id()),
                    window,
                    cx,
                )
        }
        .children(self.context_menu.as_ref().map(|(menu, position, _)| {
            deferred(
                anchored()
                    .position(*position)
                    .anchor(gpui::Anchor::TopLeft)
                    .child(menu.clone()),
            )
            .with_priority(1)
        }));

        v_flex().w_full().flex_1().overflow_hidden().child(contents)
    }

    pub(super) fn render_filter_footer(&mut self, pinned: bool, cx: &mut Context<Self>) -> Div {
        let (pin_button_id, icon, icon_tooltip) = if pinned {
            ("unpin_button", IconName::Unpin, "Unpin Outline")
        } else {
            ("pin_button", IconName::Pin, "Pin Active Outline")
        };

        let has_query = self.query(cx).is_some();

        h_flex()
            .p_2()
            .h(Tab::container_height(cx))
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(self.filter_editor.clone()),
            )
            .child(
                h_flex()
                    .when(has_query, |this| {
                        this.child(
                            IconButton::new("clear_filter", IconName::Close)
                                .shape(IconButtonShape::Square)
                                .tooltip(Tooltip::text("Clear Filter"))
                                .on_click(cx.listener(|outline_panel, _, window, cx| {
                                    outline_panel.filter_editor.update(cx, |editor, cx| {
                                        editor.set_text("", window, cx);
                                    });
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        IconButton::new(pin_button_id, icon)
                            .tooltip(Tooltip::text(icon_tooltip))
                            .shape(IconButtonShape::Square)
                            .on_click(cx.listener(|outline_panel, _, window, cx| {
                                outline_panel.toggle_active_editor_pin(
                                    &ToggleActiveEditorPin,
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
    }
}
