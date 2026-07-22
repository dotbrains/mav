use super::*;

fn full_path_budget(
    file_name: &str,
    normal_em: Pixels,
    small_em: Pixels,
    max_width: Pixels,
) -> usize {
    (((max_width / 0.8) - file_name.len() * normal_em) / small_em) as usize
}

impl PickerDelegate for FileFinderDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "file finder"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Search project files...".into()
    }

    fn searchbar_trailer(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        let focus_handle = self.focus_handle.clone();
        let including_ignored = self.include_ignored == Some(true);
        // Clicking includes ignored files unless they're already included, in
        // which case it excludes them again (see `handle_toggle_ignored`).
        let tooltip_label = if including_ignored {
            "Exclude Ignored Files"
        } else {
            "Include Ignored Files"
        };

        let filter_button = IconButton::new("filter-ignored", IconName::Sliders)
            .icon_size(IconSize::Small)
            .toggle_state(including_ignored)
            .when(self.include_ignored.is_some(), |this| {
                this.indicator(Indicator::dot().color(Color::Info))
            })
            .tooltip(move |_window, cx| {
                Tooltip::for_action_in(tooltip_label, &ToggleIncludeIgnored, &focus_handle, cx)
            })
            .on_click(|_, window, cx| {
                window.dispatch_action(ToggleIncludeIgnored.boxed_clone(), cx)
            });
        Some(
            h_flex()
                .gap_1()
                .child(filter_button)
                .children(picker::parts::project_scan_indicator(
                    self.latest_search_query.is_some(),
                    &self.project,
                    cx,
                ))
                .into_any_element(),
        )
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.has_changed_selected_index = true;
        self.selected_index = ix;
        cx.notify();
    }

    fn separators_after_indices(&self) -> Vec<usize> {
        if self.separate_history {
            let first_non_history_index = self
                .matches
                .matches
                .iter()
                .enumerate()
                .find(|(_, m)| !matches!(m, Match::History { .. }))
                .map(|(i, _)| i);
            if let Some(first_non_history_index) = first_non_history_index
                && first_non_history_index > 0
            {
                return vec![first_non_history_index - 1];
            }
        }
        Vec::new()
    }

    fn update_matches(
        &mut self,
        raw_query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let raw_query = raw_query.trim();

        let raw_query = match &raw_query.get(0..2) {
            Some(".\\" | "./") => &raw_query[2..],
            Some(prefix @ ("a\\" | "a/" | "b\\" | "b/")) => {
                if self
                    .workspace
                    .upgrade()
                    .into_iter()
                    .flat_map(|workspace| workspace.read(cx).worktrees(cx))
                    .all(|worktree| {
                        worktree
                            .read(cx)
                            .entry_for_path(RelPath::unix(prefix.split_at(1).0).unwrap())
                            .is_none_or(|entry| !entry.is_dir())
                    })
                {
                    &raw_query[2..]
                } else {
                    raw_query
                }
            }
            _ => raw_query,
        };

        if raw_query.is_empty() {
            // if there was no query before, and we already have some (history) matches
            // there's no need to update anything, since nothing has changed.
            // We also want to populate matches set from history entries on the first update.
            if self.latest_search_query.is_some() || self.first_update {
                let project = self.project.read(cx);

                self.latest_search_id = post_inc(&mut self.search_count);
                self.latest_search_query = None;
                self.matches = Matches {
                    separate_history: self.separate_history,
                    ..Matches::default()
                };
                let path_style = self.project.read(cx).path_style(cx);

                self.matches.push_new_matches(
                    project.worktree_store(),
                    cx,
                    self.history_items.iter().filter(|history_item| {
                        project
                            .worktree_for_id(history_item.project.worktree_id, cx)
                            .is_some()
                            || project.is_local()
                            || project.is_via_remote_server()
                    }),
                    self.currently_opened_path.as_ref(),
                    None,
                    None.into_iter(),
                    false,
                    path_style,
                );

                self.first_update = false;
                self.selected_index = 0;
            }
            cx.notify();
            self.search_in_flight
                .store(false, atomic::Ordering::Release);
            Task::ready(())
        } else {
            let query = parse_file_search_query(raw_query);
            let path = query.path_position.path.clone();

            let search_in_flight = self.search_in_flight.clone();
            let was_in_flight = search_in_flight.swap(true, atomic::Ordering::Relaxed);

            cx.spawn_in(window, async move |this, cx| {
                if was_in_flight {
                    cx.background_executor().timer(SEARCH_DEBOUNCE).await;
                }
                let _ = maybe!(async move {
                    let is_absolute_path = path.is_absolute();
                    let did_resolve_abs_path = is_absolute_path
                        && this
                            .update_in(cx, |this, window, cx| {
                                this.delegate
                                    .lookup_absolute_path(query.clone(), window, cx)
                            })?
                            .await;

                    // Only check for relative paths if no absolute paths were
                    // found.
                    if !did_resolve_abs_path {
                        this.update_in(cx, |this, window, cx| {
                            this.delegate.spawn_search(query, window, cx)
                        })?
                        .await;
                    }
                    anyhow::Ok(())
                })
                .await;
                search_in_flight.store(false, atomic::Ordering::Relaxed);
            })
        }
    }

    fn confirm(
        &mut self,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<FileFinderDelegate>>,
    ) {
        self.open_selected_file(secondary, true, window, cx);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<FileFinderDelegate>>) {
        self.file_finder
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .log_err();
    }

    fn try_get_preview_data_for_match(&self, cx: &App) -> Option<picker::PreviewUpdate> {
        let m = self.matches.get(self.selected_index)?;
        match m {
            Match::CreateNew(project_path) => {
                let path_style = self.project.read(cx).path_style(cx);
                let path_highlight = gpui::HighlightStyle {
                    color: Some(cx.theme().colors().text_accent),
                    ..Default::default()
                };
                let mut message = picker::HighlightedTextBuilder::default();
                message.push_plain("Create file ");
                message.push_styled(project_path.path.display(path_style), path_highlight);
                message.push_plain("?");
                Some(picker::PreviewUpdate::message(message.build()))
            }
            _ => Some(picker::PreviewUpdate::from_path(
                m.abs_path(&self.project, cx)?,
            )),
        }
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let settings = FileFinderSettings::get_global(cx);

        let path_match = self.matches.get(ix)?;

        let end_icon = match path_match {
            Match::History { .. } => Icon::new(IconName::HistoryRerun)
                .color(Color::Muted)
                .size(IconSize::Small)
                .into_any_element(),
            Match::Search(_) => v_flex()
                .flex_none()
                .size(IconSize::Small.rems())
                .into_any_element(),
            Match::Channel { .. } => v_flex()
                .flex_none()
                .size(IconSize::Small.rems())
                .into_any_element(),
            Match::CreateNew(_) => Empty.into_any_element(),
        };

        let is_create_new = matches!(path_match, Match::CreateNew(_));

        let (file_name_label, full_path_label) = self.labels_for_match(path_match, window, cx);

        let file_icon = match path_match {
            Match::Channel { .. } => Some(Icon::new(IconName::Hash).color(Color::Muted)),
            _ => maybe!({
                if !settings.file_icons {
                    return None;
                }
                let abs_path = path_match.abs_path(&self.project, cx)?;
                let file_name = abs_path.file_name()?;
                let icon = FileIcons::get_icon(file_name.as_ref(), cx)?;
                Some(Icon::from_path(icon).color(Color::Muted))
            }),
        };

        Some(
            ListItem::new(ix)
                .spacing(ListItemSpacing::Sparse)
                .inset(true)
                .toggle_state(selected)
                .map(|this| {
                    if is_create_new {
                        this.start_slot(Icon::new(IconName::Plus).size(IconSize::Small))
                    } else {
                        this.start_slot::<Icon>(file_icon)
                    }
                })
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1p5()
                        .child(file_name_label.truncate_middle())
                        .child(full_path_label.truncate_start()),
                )
                .end_slot::<AnyElement>(end_icon),
        )
    }

    fn actions_menu(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Vec<picker::PickerAction> {
        vec![
            picker::PickerAction::header("Split…"),
            picker::PickerAction::button("Left", pane::SplitLeft::default().boxed_clone()),
            picker::PickerAction::button("Right", pane::SplitRight::default().boxed_clone()),
            picker::PickerAction::button("Up", pane::SplitUp::default().boxed_clone()),
            picker::PickerAction::button("Down", pane::SplitDown::default().boxed_clone()),
            picker::PickerAction::separator(),
            picker::PickerAction::button("Open File", menu::Confirm.boxed_clone()),
        ]
    }
}
