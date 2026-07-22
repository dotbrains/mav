use super::*;

impl SettingsWindow {
    fn open_navbar_entry_page(&mut self, navbar_entry: usize) {
        // Navigating to another page dismisses the transient "copied share
        // link" checkmark shown on a Skills page row.
        self.last_copied_skill_directory_path = None;

        if !self.is_nav_entry_visible(navbar_entry) {
            self.open_first_nav_page();
        }

        let is_new_page = self.navbar_entries[self.navbar_entry].page_index
            != self.navbar_entries[navbar_entry].page_index;

        self.navbar_entry = navbar_entry;

        // We only need to reset visible items when updating matches
        // and selecting a new page
        if is_new_page {
            self.reset_list_state();
        }

        self.sub_page_stack.clear();
    }

    fn open_best_matching_nav_page(&mut self, query_words: &[&str]) {
        let mut entries = self.visible_navbar_entries().peekable();
        let first_entry = entries.peek().map(|(index, _)| (0, *index));
        let best_match = entries
            .enumerate()
            .filter(|(_, (_, entry))| !entry.is_root)
            .map(|(logical_index, (index, entry))| {
                let title_lower = entry.title.to_lowercase();
                let matching_words = query_words
                    .iter()
                    .filter(|query_word| {
                        title_lower
                            .split_whitespace()
                            .any(|title_word| title_word.starts_with(*query_word))
                    })
                    .count();
                (logical_index, index, matching_words)
            })
            .filter(|(_, _, count)| *count > 0)
            .max_by_key(|(_, _, count)| *count)
            .map(|(logical_index, index, _)| (logical_index, index));
        if let Some((logical_index, navbar_entry_index)) = best_match.or(first_entry) {
            self.open_navbar_entry_page(navbar_entry_index);
            self.navbar_scroll_handle
                .scroll_to_item(logical_index + 1, gpui::ScrollStrategy::Top);
        }
    }

    fn scroll_content_to_best_match(&self, query_words: &[&str]) {
        let position = self
            .visible_page_items()
            .enumerate()
            .find(|(_, (_, item))| match item {
                SettingsPageItem::SectionHeader(title) => {
                    let title_lower = title.to_lowercase();
                    query_words.iter().all(|query_word| {
                        title_lower
                            .split_whitespace()
                            .any(|title_word| title_word.starts_with(query_word))
                    })
                }
                _ => false,
            })
            .map(|(position, _)| position);
        if let Some(position) = position {
            self.list_state.scroll_to(gpui::ListOffset {
                item_ix: position + 1,
                offset_in_item: px(0.),
            });
        }
    }

    fn open_first_nav_page(&mut self) {
        let Some(first_navbar_entry_index) = self.visible_navbar_entries().next().map(|e| e.0)
        else {
            return;
        };
        self.open_navbar_entry_page(first_navbar_entry_index);
    }

    fn change_file(&mut self, ix: usize, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        if ix >= self.files.len() {
            self.current_file = SettingsUiFile::User;
            self.build_ui(window, cx);
            return;
        }

        if self.files[ix].0 == self.current_file {
            return;
        }
        self.current_file = self.files[ix].0.clone();

        if let SettingsUiFile::Project((_, _)) = &self.current_file {
            telemetry::event!("Setting Project Clicked");
        }

        self.build_ui(window, cx);

        if self
            .visible_navbar_entries()
            .any(|(index, _)| index == self.navbar_entry)
        {
            self.open_and_scroll_to_navbar_entry(self.navbar_entry, None, true, window, cx);
        } else {
            self.open_first_nav_page();
        };
    }

    /// Changes the current settings file like [`Self::change_file`], but keeps
    /// the currently open sub-page stack when every sub-page in it is
    /// available in the new file's scope (e.g. switching a Skills sub-page
    /// between the user scope and a project scope).
    fn change_file_in_sub_page(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        if ix >= self.files.len() || self.files[ix].0 == self.current_file {
            return;
        }
        self.current_file = self.files[ix].0.clone();

        if let SettingsUiFile::Project((_, _)) = &self.current_file {
            telemetry::event!("Setting Project Clicked");
        }

        self.last_copied_skill_directory_path = None;

        let sub_page_stack = std::mem::take(&mut self.sub_page_stack);
        self.build_ui(window, cx);

        let file_mask = self.current_file.mask();
        if let Some(first_sub_page) = sub_page_stack.first()
            && sub_page_stack
                .iter()
                .all(|sub_page| sub_page.link.files.contains(file_mask))
        {
            if !self.is_nav_entry_visible(self.navbar_entry) {
                // The previously selected page may be filtered out in the new
                // scope (e.g. after deep-linking into a sub-page). Re-anchor
                // the navbar to the page containing the open sub-page, which
                // is visible because its sub-page link supports this scope.
                let anchor_entry = self
                    .pages
                    .iter()
                    .position(|page| {
                        page.items.iter().any(|item| {
                            matches!(item, SettingsPageItem::SubPageLink(link) if link == &first_sub_page.link)
                        })
                    })
                    .and_then(|page_index| {
                        self.navbar_entries
                            .iter()
                            .position(|entry| entry.is_root && entry.page_index == page_index)
                    });
                if let Some(anchor_entry) = anchor_entry
                    && self.is_nav_entry_visible(anchor_entry)
                {
                    self.open_navbar_entry_page(anchor_entry);
                }
            }
            if self.is_nav_entry_visible(self.navbar_entry) {
                self.sub_page_stack = sub_page_stack;
                cx.notify();
                return;
            }
        }

        if self.is_nav_entry_visible(self.navbar_entry) {
            self.open_and_scroll_to_navbar_entry(self.navbar_entry, None, true, window, cx);
        } else {
            self.open_first_nav_page();
        }
    }

    fn render_files_header(
        &self,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement {
        static OVERFLOW_LIMIT: usize = 1;

        let file_button =
            |ix, file: &SettingsUiFile, focus_handle, cx: &mut Context<SettingsWindow>| {
                Button::new(
                    ix,
                    self.display_name(&file)
                        .expect("Files should always have a name"),
                )
                .toggle_state(file == &self.current_file)
                .selected_style(ButtonStyle::Tinted(ui::TintColor::Accent))
                .track_focus(focus_handle)
                .on_click(cx.listener({
                    let focus_handle = focus_handle.clone();
                    move |this, _: &gpui::ClickEvent, window, cx| {
                        this.change_file(ix, window, cx);
                        focus_handle.focus(window, cx);
                    }
                }))
            };

        let this = cx.entity();

        let selected_file_ix = self
            .files
            .iter()
            .enumerate()
            .skip(OVERFLOW_LIMIT)
            .find_map(|(ix, (file, _))| {
                if file == &self.current_file {
                    Some(ix)
                } else {
                    None
                }
            })
            .unwrap_or(OVERFLOW_LIMIT);
        let edit_in_json_id = SharedString::new(format!("edit-in-json-{}", selected_file_ix));

        h_flex()
            .id("settings-ui-files-header")
            .role(Role::Group)
            .aria_label("Settings File")
            .w_full()
            .gap_1()
            .justify_between()
            .track_focus(&self.files_focus_handle)
            .tab_group()
            .tab_index(HEADER_GROUP_TAB_INDEX)
            .child(
                h_flex()
                    .gap_1()
                    .children(
                        self.files.iter().enumerate().take(OVERFLOW_LIMIT).map(
                            |(ix, (file, focus_handle))| file_button(ix, file, focus_handle, cx),
                        ),
                    )
                    .when(self.files.len() > OVERFLOW_LIMIT, |div| {
                        let (file, focus_handle) = &self.files[selected_file_ix];

                        div.child(file_button(selected_file_ix, file, focus_handle, cx))
                            .when(self.files.len() > OVERFLOW_LIMIT + 1, |div| {
                                div.child(
                                    DropdownMenu::new(
                                        "more-files",
                                        format!("+{}", self.files.len() - (OVERFLOW_LIMIT + 1)),
                                        ContextMenu::build(window, cx, move |mut menu, _, _| {
                                            for (mut ix, (file, focus_handle)) in self
                                                .files
                                                .iter()
                                                .enumerate()
                                                .skip(OVERFLOW_LIMIT + 1)
                                            {
                                                let (display_name, focus_handle) =
                                                    if selected_file_ix == ix {
                                                        ix = OVERFLOW_LIMIT;
                                                        (
                                                            self.display_name(&self.files[ix].0),
                                                            self.files[ix].1.clone(),
                                                        )
                                                    } else {
                                                        (
                                                            self.display_name(&file),
                                                            focus_handle.clone(),
                                                        )
                                                    };

                                                menu = menu.entry(
                                                    display_name
                                                        .expect("Files should always have a name"),
                                                    None,
                                                    {
                                                        let this = this.clone();
                                                        move |window, cx| {
                                                            this.update(cx, |this, cx| {
                                                                this.change_file(ix, window, cx);
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
                                    .trigger_tooltip(Tooltip::text("View Other Projects"))
                                    .trigger_icon(IconName::ChevronDown)
                                    .attach(gpui::Anchor::BottomLeft)
                                    .offset(gpui::Point {
                                        x: px(0.0),
                                        y: px(2.0),
                                    })
                                    .tab_index(0),
                                )
                            })
                    }),
            )
            .child(
                Button::new(edit_in_json_id, "Edit in settings.json")
                    .tab_index(0_isize)
                    .style(ButtonStyle::OutlinedGhost)
                    .tooltip(Tooltip::for_action_title_in(
                        "Edit in settings.json",
                        &OpenCurrentFile,
                        &self.focus_handle,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_current_settings_file(window, cx);
                    })),
            )
    }

    pub(crate) fn display_name(&self, file: &SettingsUiFile) -> Option<String> {
        match file {
            SettingsUiFile::User => Some("User".to_string()),
            SettingsUiFile::Project((worktree_id, path)) => self
                .worktree_root_dirs
                .get(&worktree_id)
                .map(|directory_name| {
                    let path_style = PathStyle::local();
                    if path.is_empty() {
                        directory_name.clone()
                    } else {
                        format!(
                            "{}{}{}",
                            directory_name,
                            path_style.primary_separator(),
                            path.display(path_style)
                        )
                    }
                }),
            SettingsUiFile::Server(file) => Some(file.to_string()),
        }
    }

    // TODO:
    //  Reconsider this after preview launch
    // fn file_location_str(&self) -> String {
    //     match &self.current_file {
    //         SettingsUiFile::User => "settings.json".to_string(),
    //         SettingsUiFile::Project((worktree_id, path)) => self
    //             .worktree_root_dirs
    //             .get(&worktree_id)
    //             .map(|directory_name| {
    //                 let path_style = PathStyle::local();
    //                 let file_path = path.join(paths::local_settings_file_relative_path());
    //                 format!(
    //                     "{}{}{}",
    //                     directory_name,
    //                     path_style.separator(),
    //                     file_path.display(path_style)
    //                 )
    //             })
    //             .expect("Current file should always be present in root dir map"),
    //         SettingsUiFile::Server(file) => file.to_string(),
    //     }
    // }

    fn render_search(&self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (a11y_value, a11y_text_runs) =
            text_field_a11y_state("settings-ui-search", &self.search_bar, window, cx);

        h_flex()
            .id("settings-ui-search")
            .role(Role::SearchInput)
            .aria_label("Search Settings")
            .aria_value(a11y_value)
            .track_focus(&self.search_bar.focus_handle(cx))
            .a11y_synthetic_children(a11y_text_runs)
            .py_1()
            .px_1p5()
            .mb_3()
            .gap_1p5()
            .rounded_sm()
            .bg(cx.theme().colors().editor_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(Icon::new(IconName::MagnifyingGlass).color(Color::Muted))
            .child(self.search_bar.clone())
    }
}
