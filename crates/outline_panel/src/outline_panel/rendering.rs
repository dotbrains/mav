use super::*;

impl OutlinePanel {
    pub(super) fn render_excerpt(
        &self,
        excerpt: &ExcerptRange<Anchor>,
        depth: usize,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> Option<Stateful<Div>> {
        let item_id = ElementId::from(format!("{excerpt:?}"));
        let is_active = match self.selected_entry() {
            Some(PanelEntry::Outline(OutlineEntry::Excerpt(selected_excerpt))) => {
                selected_excerpt == excerpt
            }
            _ => false,
        };
        let has_outlines = self
            .buffers
            .get(&excerpt.context.start.buffer_id)
            .and_then(|buffer| match &buffer.outlines {
                OutlineState::Outlines(outlines) => Some(outlines),
                OutlineState::Invalidated(outlines) => Some(outlines),
                OutlineState::NotFetched => None,
            })
            .is_some_and(|outlines| !outlines.is_empty());
        let is_expanded = !self
            .collapsed_entries
            .contains(&CollapsedEntry::Excerpt(excerpt.clone()));
        let color = entry_label_color(is_active);
        let icon = if has_outlines {
            FileIcons::get_chevron_icon(is_expanded, cx)
                .map(|icon_path| Icon::from_path(icon_path).color(color).into_any_element())
        } else {
            None
        }
        .unwrap_or_else(empty_icon);

        let label = self.excerpt_label(&excerpt, cx)?;
        let label_element = Label::new(label)
            .single_line()
            .color(color)
            .into_any_element();

        Some(self.entry_element(
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt.clone())),
            item_id,
            depth,
            icon,
            is_active,
            label_element,
            window,
            cx,
        ))
    }

    pub(super) fn excerpt_label(
        &self,
        range: &ExcerptRange<language::Anchor>,
        cx: &App,
    ) -> Option<String> {
        let buffer_snapshot = self.buffer_snapshot_for_id(range.context.start.buffer_id, cx)?;
        let excerpt_range = range.context.to_point(&buffer_snapshot);
        Some(format!(
            "Lines {}- {}",
            excerpt_range.start.row + 1,
            excerpt_range.end.row + 1,
        ))
    }

    pub(super) fn render_outline(
        &self,
        outline: &Outline,
        depth: usize,
        string_match: Option<&StringMatch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let item_id = ElementId::from(SharedString::from(format!(
            "{:?}|{:?}",
            outline.range, &outline.text,
        )));

        let label_element = outline::render_item(
            &outline,
            string_match
                .map(|string_match| string_match.ranges().collect::<Vec<_>>())
                .unwrap_or_default(),
            cx,
        )
        .into_any_element();

        let is_active = match self.selected_entry() {
            Some(PanelEntry::Outline(OutlineEntry::Outline(selected))) => outline == selected,
            _ => false,
        };

        let has_children = self
            .outline_children_cache
            .get(&outline.range.start.buffer_id)
            .and_then(|children_map| {
                let key = (outline.range.clone(), outline.depth);
                children_map.get(&key)
            })
            .copied()
            .unwrap_or(false);
        let is_expanded = !self
            .collapsed_entries
            .contains(&CollapsedEntry::Outline(outline.range.clone()));

        let icon = if has_children {
            FileIcons::get_chevron_icon(is_expanded, cx)
                .map(|icon_path| {
                    Icon::from_path(icon_path)
                        .color(entry_label_color(is_active))
                        .into_any_element()
                })
                .unwrap_or_else(empty_icon)
        } else {
            empty_icon()
        };

        self.entry_element(
            PanelEntry::Outline(OutlineEntry::Outline(outline.clone())),
            item_id,
            depth,
            icon,
            is_active,
            label_element,
            window,
            cx,
        )
    }

    pub(super) fn render_entry(
        &self,
        rendered_entry: &FsEntry,
        depth: usize,
        string_match: Option<&StringMatch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let settings = OutlinePanelSettings::get_global(cx);
        let is_active = match self.selected_entry() {
            Some(PanelEntry::Fs(selected_entry)) => selected_entry == rendered_entry,
            _ => false,
        };
        let (item_id, label_element, icon) = match rendered_entry {
            FsEntry::File(FsEntryFile {
                worktree_id, entry, ..
            }) => {
                let name = self.entry_name(worktree_id, entry, cx);
                let color =
                    entry_git_aware_label_color(entry.git_summary, entry.is_ignored, is_active);
                let icon = if settings.file_icons {
                    FileIcons::get_icon(entry.path.as_std_path(), cx)
                        .map(|icon_path| Icon::from_path(icon_path).color(color).into_any_element())
                } else {
                    None
                };
                (
                    ElementId::from(entry.id.to_proto() as usize),
                    HighlightedLabel::new(
                        name,
                        string_match
                            .map(|string_match| string_match.positions.clone())
                            .unwrap_or_default(),
                    )
                    .color(color)
                    .into_any_element(),
                    icon.unwrap_or_else(empty_icon),
                )
            }
            FsEntry::Directory(directory) => {
                let name = self.entry_name(&directory.worktree_id, &directory.entry, cx);

                let is_expanded = !self.collapsed_entries.contains(&CollapsedEntry::Dir(
                    directory.worktree_id,
                    directory.entry.id,
                ));
                let color = entry_git_aware_label_color(
                    directory.entry.git_summary,
                    directory.entry.is_ignored,
                    is_active,
                );
                let icon = if settings.folder_icons {
                    FileIcons::get_folder_icon(is_expanded, directory.entry.path.as_std_path(), cx)
                } else {
                    FileIcons::get_chevron_icon(is_expanded, cx)
                }
                .map(Icon::from_path)
                .map(|icon| icon.color(color).into_any_element());
                (
                    ElementId::from(directory.entry.id.to_proto() as usize),
                    HighlightedLabel::new(
                        name,
                        string_match
                            .map(|string_match| string_match.positions.clone())
                            .unwrap_or_default(),
                    )
                    .color(color)
                    .into_any_element(),
                    icon.unwrap_or_else(empty_icon),
                )
            }
            FsEntry::ExternalFile(external_file) => {
                let color = entry_label_color(is_active);
                let (icon, name) = match self.buffer_snapshot_for_id(external_file.buffer_id, cx) {
                    Some(buffer_snapshot) => match buffer_snapshot.file() {
                        Some(file) => {
                            let path = file.path();
                            let icon = if settings.file_icons {
                                FileIcons::get_icon(path.as_std_path(), cx)
                            } else {
                                None
                            }
                            .map(Icon::from_path)
                            .map(|icon| icon.color(color).into_any_element());
                            (icon, file_name(path.as_std_path()))
                        }
                        None => (None, "Untitled".to_string()),
                    },
                    None => (None, "Unknown buffer".to_string()),
                };
                (
                    ElementId::from(external_file.buffer_id.to_proto() as usize),
                    HighlightedLabel::new(
                        name,
                        string_match
                            .map(|string_match| string_match.positions.clone())
                            .unwrap_or_default(),
                    )
                    .color(color)
                    .into_any_element(),
                    icon.unwrap_or_else(empty_icon),
                )
            }
        };

        self.entry_element(
            PanelEntry::Fs(rendered_entry.clone()),
            item_id,
            depth,
            icon,
            is_active,
            label_element,
            window,
            cx,
        )
    }

    pub(super) fn render_folded_dirs(
        &self,
        folded_dir: &FoldedDirsEntry,
        depth: usize,
        string_match: Option<&StringMatch>,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> Stateful<Div> {
        let settings = OutlinePanelSettings::get_global(cx);
        let is_active = match self.selected_entry() {
            Some(PanelEntry::FoldedDirs(selected_dirs)) => {
                selected_dirs.worktree_id == folded_dir.worktree_id
                    && selected_dirs.entries == folded_dir.entries
            }
            _ => false,
        };
        let (item_id, label_element, icon) = {
            let name = self.dir_names_string(&folded_dir.entries, folded_dir.worktree_id, cx);

            let is_expanded = folded_dir.entries.iter().all(|dir| {
                !self
                    .collapsed_entries
                    .contains(&CollapsedEntry::Dir(folded_dir.worktree_id, dir.id))
            });
            let is_ignored = folded_dir.entries.iter().any(|entry| entry.is_ignored);
            let git_status = folded_dir
                .entries
                .first()
                .map(|entry| entry.git_summary)
                .unwrap_or_default();
            let color = entry_git_aware_label_color(git_status, is_ignored, is_active);
            let icon = if settings.folder_icons {
                FileIcons::get_folder_icon(is_expanded, &Path::new(&name), cx)
            } else {
                FileIcons::get_chevron_icon(is_expanded, cx)
            }
            .map(Icon::from_path)
            .map(|icon| icon.color(color).into_any_element());
            (
                ElementId::from(
                    folded_dir
                        .entries
                        .last()
                        .map(|entry| entry.id.to_proto())
                        .unwrap_or_else(|| folded_dir.worktree_id.to_proto())
                        as usize,
                ),
                HighlightedLabel::new(
                    name,
                    string_match
                        .map(|string_match| string_match.positions.clone())
                        .unwrap_or_default(),
                )
                .color(color)
                .into_any_element(),
                icon.unwrap_or_else(empty_icon),
            )
        };

        self.entry_element(
            PanelEntry::FoldedDirs(folded_dir.clone()),
            item_id,
            depth,
            icon,
            is_active,
            label_element,
            window,
            cx,
        )
    }

    pub(super) fn render_search_match(
        &mut self,
        multi_buffer_snapshot: Option<&MultiBufferSnapshot>,
        match_range: &Range<editor::Anchor>,
        render_data: &Arc<OnceLock<SearchData>>,
        kind: SearchKind,
        depth: usize,
        string_match: Option<&StringMatch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let search_data = match render_data.get() {
            Some(search_data) => search_data,
            None => {
                if let ItemsDisplayMode::Search(search_state) = &mut self.mode
                    && let Some(multi_buffer_snapshot) = multi_buffer_snapshot
                {
                    search_state
                        .highlight_search_match_tx
                        .try_send(HighlightArguments {
                            multi_buffer_snapshot: multi_buffer_snapshot.clone(),
                            match_range: match_range.clone(),
                            search_data: Arc::clone(render_data),
                        })
                        .ok();
                }
                return None;
            }
        };
        let search_matches = string_match
            .iter()
            .flat_map(|string_match| string_match.ranges())
            .collect::<Vec<_>>();
        let match_ranges = if search_matches.is_empty() {
            &search_data.search_match_indices
        } else {
            &search_matches
        };
        let label_element = outline::render_item(
            &OutlineItem {
                depth,
                annotation_range: None,
                range: search_data.context_range.clone(),
                selection_range: search_data.context_range.clone(),
                text: search_data.context_text.clone().into(),
                source_range_for_text: search_data.context_range.clone(),
                highlight_ranges: search_data
                    .highlights_data
                    .get()
                    .cloned()
                    .unwrap_or_default(),
                name_ranges: search_data.search_match_indices.clone(),
                body_range: Some(search_data.context_range.clone()),
            },
            match_ranges.iter().cloned(),
            cx,
        );
        let truncated_contents_label = || Label::new(TRUNCATED_CONTEXT_MARK);
        let entire_label = h_flex()
            .justify_center()
            .p_0()
            .when(search_data.truncated_left, |parent| {
                parent.child(truncated_contents_label())
            })
            .child(label_element)
            .when(search_data.truncated_right, |parent| {
                parent.child(truncated_contents_label())
            })
            .into_any_element();

        let is_active = match self.selected_entry() {
            Some(PanelEntry::Search(SearchEntry {
                match_range: selected_match_range,
                ..
            })) => match_range == selected_match_range,
            _ => false,
        };
        Some(self.entry_element(
            PanelEntry::Search(SearchEntry {
                kind,
                match_range: match_range.clone(),
                render_data: render_data.clone(),
            }),
            ElementId::from(SharedString::from(format!("search-{match_range:?}"))),
            depth,
            empty_icon(),
            is_active,
            entire_label,
            window,
            cx,
        ))
    }

    fn entry_element(
        &self,
        rendered_entry: PanelEntry,
        item_id: ElementId,
        depth: usize,
        icon_element: AnyElement,
        is_active: bool,
        label_element: gpui::AnyElement,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> Stateful<Div> {
        let settings = OutlinePanelSettings::get_global(cx);
        div()
            .text_ui(cx)
            .id(item_id.clone())
            .on_click({
                let clicked_entry = rendered_entry.clone();
                cx.listener(move |outline_panel, event: &gpui::ClickEvent, window, cx| {
                    if event.is_right_click() || event.first_focus() {
                        return;
                    }

                    let change_focus = event.click_count() > 1;
                    outline_panel.toggle_expanded(&clicked_entry, window, cx);

                    outline_panel.scroll_editor_to_entry(
                        &clicked_entry,
                        true,
                        change_focus,
                        window,
                        cx,
                    );
                })
            })
            .cursor_pointer()
            .child(
                ListItem::new(item_id)
                    .indent_level(depth)
                    .indent_step_size(px(settings.indent_size))
                    .toggle_state(is_active)
                    .child(
                        h_flex()
                            .child(h_flex().w(px(16.)).justify_center().child(icon_element))
                            .child(h_flex().h_6().child(label_element).ml_1()),
                    )
                    .on_secondary_mouse_down(cx.listener(
                        move |outline_panel, event: &MouseDownEvent, window, cx| {
                            // Stop propagation to prevent the catch-all context menu for the project
                            // panel from being deployed.
                            cx.stop_propagation();
                            outline_panel.deploy_context_menu(
                                event.position,
                                rendered_entry.clone(),
                                window,
                                cx,
                            )
                        },
                    )),
            )
            .border_1()
            .border_r_2()
            .rounded_none()
            .hover(|style| {
                if is_active {
                    style
                } else {
                    let hover_color = cx.theme().colors().ghost_element_hover;
                    style.bg(hover_color).border_color(hover_color)
                }
            })
            .when(
                is_active && self.focus_handle.contains_focused(window, cx),
                |div| div.border_color(cx.theme().colors().panel_focused_border),
            )
    }
}
