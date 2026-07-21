use super::*;

impl GitPanel {
    pub(super) fn render_entries(
        &self,
        has_write_access: bool,
        repo: Entity<Repository>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (is_tree_view, entry_count) = match &self.view_mode {
            GitPanelViewMode::Tree(state) => (true, state.logical_indices.len()),
            GitPanelViewMode::Flat => (false, self.entries.len()),
        };
        let repo = repo.downgrade();

        v_flex()
            .flex_1()
            .size_full()
            .overflow_hidden()
            .relative()
            .child(
                h_flex()
                    .flex_1()
                    .size_full()
                    .relative()
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "entries",
                            entry_count,
                            cx.processor(move |this, range: Range<usize>, window, cx| {
                                let Some(repo) = repo.upgrade() else {
                                    return Vec::new();
                                };
                                let repo = repo.read(cx);

                                let mut items = Vec::with_capacity(range.end - range.start);

                                for ix in range.into_iter().map(|ix| match &this.view_mode {
                                    GitPanelViewMode::Tree(state) => state.logical_indices[ix],
                                    GitPanelViewMode::Flat => ix,
                                }) {
                                    match &this.entries.get(ix) {
                                        Some(GitListEntry::Status(entry)) => {
                                            items.push(this.render_status_entry(
                                                ix,
                                                entry,
                                                0,
                                                has_write_access,
                                                repo,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::TreeStatus(entry)) => {
                                            items.push(this.render_status_entry(
                                                ix,
                                                &entry.entry,
                                                entry.depth,
                                                has_write_access,
                                                repo,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::Directory(entry)) => {
                                            items.push(this.render_directory_entry(
                                                ix,
                                                entry,
                                                has_write_access,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::Header(header)) => {
                                            items.push(this.render_list_header(
                                                ix,
                                                header,
                                                has_write_access,
                                                window,
                                                cx,
                                            ));
                                        }
                                        None => {}
                                    }
                                }

                                items
                            }),
                        )
                        .when(is_tree_view, |list| {
                            let indent_size = px(TREE_INDENT);
                            list.with_decoration(
                                ui::indent_guides(indent_size, IndentGuideColors::panel(cx))
                                    .with_compute_indents_fn(
                                        cx.entity(),
                                        |this, range, _window, _cx| {
                                            this.compute_visible_depths(range)
                                        },
                                    )
                                    .with_render_fn(cx.entity(), |_, params, _, _| {
                                        // Magic number to align the tree item is 3 here
                                        // because we're using 12px as the left-side padding
                                        // and 3 makes the alignment work with the bounding box of the icon
                                        let left_offset = px(TREE_INDENT + 3_f32);
                                        let indent_size = params.indent_size;
                                        let item_height = params.item_height;

                                        params
                                            .indent_guides
                                            .into_iter()
                                            .map(|layout| {
                                                let bounds = Bounds::new(
                                                    point(
                                                        layout.offset.x * indent_size + left_offset,
                                                        layout.offset.y * item_height,
                                                    ),
                                                    size(px(1.), layout.length * item_height),
                                                );
                                                RenderedIndentGuide {
                                                    bounds,
                                                    layout,
                                                    is_active: false,
                                                    hitbox: None,
                                                }
                                            })
                                            .collect()
                                    }),
                            )
                        })
                        .group("entries")
                        .size_full()
                        .flex_grow_1()
                        .with_width_from_item(self.max_width_item_index)
                        .track_scroll(&self.scroll_handle),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.deploy_panel_context_menu(event.position, window, cx)
                        }),
                    )
                    .custom_scrollbars(
                        Scrollbars::for_settings::<GitPanelScrollbarAccessor>()
                            .tracked_scroll_handle(&self.scroll_handle)
                            .with_track_along(
                                ScrollAxes::Horizontal,
                                cx.theme().colors().editor_background,
                            ),
                        window,
                        cx,
                    ),
            )
    }

    pub(super) fn entry_label(&self, label: impl Into<SharedString>, color: Color) -> Label {
        Label::new(label.into()).color(color)
    }

    pub(super) fn list_item_height(&self) -> Rems {
        rems(1.75)
    }

    pub(super) fn render_list_header(
        &self,
        ix: usize,
        header: &GitHeaderEntry,
        has_write_access: bool,
        _window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id: ElementId = ElementId::Name(format!("header_{}", ix).into());
        let checkbox_id: ElementId = ElementId::Name(format!("header_{}_checkbox", ix).into());
        let group_name: SharedString = format!("header_{}", ix).into();
        let toggle_state = self.header_state(header.header);
        let section = header.header;
        let weak = cx.weak_entity();

        h_flex()
            .id(id)
            .cursor_pointer()
            .group(group_name)
            .h(self.list_item_height())
            .w_full()
            .pl_3()
            .pr_1()
            .gap_2()
            .justify_between()
            .hover(|s| s.bg(cx.theme().colors().ghost_element_hover))
            .border_1()
            .border_r_2()
            .child(
                Label::new(header.title())
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            )
            .child(
                Checkbox::new(checkbox_id, toggle_state)
                    .disabled(!has_write_access)
                    .fill()
                    .elevation(ElevationIndex::Surface),
            )
            .on_click(move |_, window, cx| {
                if !has_write_access {
                    return;
                }

                weak.update(cx, |this, cx| {
                    this.toggle_staged_for_entry(
                        &GitListEntry::Header(GitHeaderEntry { header: section }),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .ok();
            })
            .into_any_element()
    }
}
