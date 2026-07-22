use super::*;

impl SettingsWindow {
    pub(super) fn toggle_navbar_entry(&mut self, nav_entry_index: usize) {
        // We can only toggle root entries
        if !self.navbar_entries[nav_entry_index].is_root {
            return;
        }

        let expanded = &mut self.navbar_entries[nav_entry_index].expanded;
        *expanded = !*expanded;
        self.navbar_entry = nav_entry_index;
        self.reset_list_state();
    }

    pub(super) fn toggle_and_focus_navbar_entry(
        &mut self,
        nav_entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_navbar_entry(nav_entry_index);
        window.focus(&self.navbar_entries[nav_entry_index].focus_handle, cx);
        cx.notify();
    }

    pub(super) fn toggle_navbar_entry_on_double_click(
        &mut self,
        nav_entry_index: usize,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entry) = self.navbar_entries.get(nav_entry_index) else {
            return false;
        };
        if !entry.is_root || event.click_count() != 2 {
            return false;
        }

        self.toggle_and_focus_navbar_entry(nav_entry_index, window, cx);
        true
    }

    pub(super) fn build_navbar(&mut self, cx: &App) {
        let mut navbar_entries = Vec::new();

        for (page_index, page) in self.pages.iter().enumerate() {
            navbar_entries.push(NavBarEntry {
                title: page.title,
                is_root: true,
                expanded: false,
                page_index,
                item_index: None,
                focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
            });

            for (item_index, item) in page.items.iter().enumerate() {
                let SettingsPageItem::SectionHeader(title) = item else {
                    continue;
                };
                navbar_entries.push(NavBarEntry {
                    title,
                    is_root: false,
                    expanded: false,
                    page_index,
                    item_index: Some(item_index),
                    focus_handle: cx.focus_handle().tab_index(0).tab_stop(true),
                });
            }
        }

        self.navbar_entries = navbar_entries;
    }

    pub(super) fn setup_navbar_focus_subscriptions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        let mut focus_subscriptions = Vec::new();

        for entry_index in 0..self.navbar_entries.len() {
            let focus_handle = self.navbar_entries[entry_index].focus_handle.clone();

            let subscription = cx.on_focus(
                &focus_handle,
                window,
                move |this: &mut SettingsWindow,
                      window: &mut Window,
                      cx: &mut Context<SettingsWindow>| {
                    if this.sub_page_stack.is_empty() {
                        this.open_and_scroll_to_navbar_entry(entry_index, None, false, window, cx);
                    }
                },
            );
            focus_subscriptions.push(subscription);
        }
        self.navbar_focus_subscriptions = focus_subscriptions;
    }

    pub(super) fn visible_navbar_entries(&self) -> impl Iterator<Item = (usize, &NavBarEntry)> {
        let mut index = 0;
        let entries = &self.navbar_entries;
        let search_matches = &self.filter_table;
        let has_query = self.has_query;
        std::iter::from_fn(move || {
            while index < entries.len() {
                let entry = &entries[index];
                let included_in_search = if let Some(item_index) = entry.item_index {
                    search_matches[entry.page_index][item_index]
                } else {
                    search_matches[entry.page_index].iter().any(|b| *b)
                        || search_matches[entry.page_index].is_empty()
                };
                if included_in_search {
                    break;
                }
                index += 1;
            }
            if index >= self.navbar_entries.len() {
                return None;
            }
            let entry = &entries[index];
            let entry_index = index;

            index += 1;
            if entry.is_root && !entry.expanded && !has_query {
                while index < entries.len() {
                    if entries[index].is_root {
                        break;
                    }
                    index += 1;
                }
            }

            return Some((entry_index, entry));
        })
    }

    pub(super) fn filter_matches_to_file(&mut self) {
        let current_file = self.current_file.mask();
        for (page, page_filter) in std::iter::zip(&self.pages, &mut self.filter_table) {
            let mut header_index = 0;
            let mut any_found_since_last_header = true;

            for (index, item) in page.items.iter().enumerate() {
                match item {
                    SettingsPageItem::SectionHeader(_) => {
                        if !any_found_since_last_header {
                            page_filter[header_index] = false;
                        }
                        header_index = index;
                        any_found_since_last_header = false;
                    }
                    SettingsPageItem::SettingItem(SettingItem { files, .. })
                    | SettingsPageItem::SubPageLink(SubPageLink { files, .. })
                    | SettingsPageItem::DynamicItem(DynamicItem {
                        discriminant: SettingItem { files, .. },
                        ..
                    }) => {
                        if !files.contains(current_file) {
                            page_filter[index] = false;
                        } else {
                            any_found_since_last_header = true;
                        }
                    }
                    SettingsPageItem::ActionLink(ActionLink { files, .. }) => {
                        if !files.contains(current_file) {
                            page_filter[index] = false;
                        } else {
                            any_found_since_last_header = true;
                        }
                    }
                }
            }
            if let Some(last_header) = page_filter.get_mut(header_index)
                && !any_found_since_last_header
            {
                *last_header = false;
            }
        }
    }

    pub(super) fn filter_by_json_path(&self, query: &str) -> Vec<usize> {
        let Some(path) = query.strip_prefix('#') else {
            return vec![];
        };
        let Some(search_index) = self.search_index.as_ref() else {
            return vec![];
        };
        let mut indices = vec![];
        for (index, SearchKeyLUTEntry { json_path, .. }) in search_index.key_lut.iter().enumerate()
        {
            let Some(json_path) = json_path else {
                continue;
            };

            if let Some(post) = json_path.strip_prefix(path)
                && (post.is_empty() || post.starts_with('.'))
            {
                indices.push(index);
            }
        }
        indices
    }

    pub(super) fn apply_match_indices(
        &mut self,
        match_indices: impl Iterator<Item = usize>,
        query: &str,
    ) {
        let Some(search_index) = self.search_index.as_ref() else {
            return;
        };

        for page in &mut self.filter_table {
            page.fill(false);
        }

        for match_index in match_indices {
            let SearchKeyLUTEntry {
                page_index,
                header_index,
                item_index,
                ..
            } = search_index.key_lut[match_index];
            let page = &mut self.filter_table[page_index];
            page[header_index] = true;
            page[item_index] = true;
        }
        self.has_query = true;
        self.filter_matches_to_file();
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        self.open_best_matching_nav_page(&query_words);
        self.reset_list_state();
        self.scroll_content_to_best_match(&query_words);
    }

    pub(super) fn update_matches(&mut self, cx: &mut Context<SettingsWindow>) {
        self.search_task.take();
        let query = self.search_bar.read(cx).text(cx);
        if query.is_empty() || self.search_index.is_none() {
            for page in &mut self.filter_table {
                page.fill(true);
            }
            self.has_query = false;
            self.filter_matches_to_file();
            self.reset_list_state();
            cx.notify();
            return;
        }

        let is_json_link_query = query.starts_with("#");
        if is_json_link_query {
            let indices = self.filter_by_json_path(&query);
            if !indices.is_empty() {
                self.apply_match_indices(indices.into_iter(), &query);
                cx.notify();
                return;
            }
        }

        let search_index = self.search_index.as_ref().unwrap().clone();

        self.search_task = Some(cx.spawn(async move |this, cx| {
            let exact_match_task = cx.background_spawn({
                let search_index = search_index.clone();
                let query = query.clone();
                async move {
                    let query_lower = query.to_lowercase();
                    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                    if query_words.is_empty() {
                        return Vec::new();
                    }
                    search_index
                        .documents
                        .iter()
                        .filter(|doc| {
                            query_words.iter().all(|query_word| {
                                doc.words
                                    .iter()
                                    .any(|doc_word| doc_word.starts_with(query_word))
                            })
                        })
                        .map(|doc| doc.id)
                        .collect::<Vec<usize>>()
                }
            });
            let cancel_flag = std::sync::atomic::AtomicBool::new(false);
            let fuzzy_search_task = fuzzy::match_strings(
                search_index.fuzzy_match_candidates.as_slice(),
                &query,
                false,
                true,
                search_index.fuzzy_match_candidates.len(),
                &cancel_flag,
                cx.background_executor().clone(),
            );

            let fuzzy_matches = fuzzy_search_task.await;
            let exact_matches = exact_match_task.await;

            this.update(cx, |this, cx| {
                let exact_indices = exact_matches.into_iter();
                let fuzzy_indices = fuzzy_matches
                    .into_iter()
                    .take_while(|fuzzy_match| fuzzy_match.score >= 0.5)
                    .map(|fuzzy_match| fuzzy_match.candidate_id);
                let merged_indices = exact_indices.chain(fuzzy_indices);

                this.apply_match_indices(merged_indices, &query);
                cx.notify();
            })
            .ok();

            cx.background_executor().timer(Duration::from_secs(1)).await;
            telemetry::event!("Settings Searched", query = query)
        }));
    }

    pub(super) fn build_filter_table(&mut self) {
        self.filter_table = self
            .pages
            .iter()
            .map(|page| vec![true; page.items.len()])
            .collect::<Vec<_>>();
    }
}
