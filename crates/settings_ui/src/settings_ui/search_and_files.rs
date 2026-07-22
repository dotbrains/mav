use super::*;

impl SettingsWindow {
    fn build_search_index(&mut self) {
        fn split_into_words(parts: &[&str]) -> Vec<String> {
            parts
                .iter()
                .flat_map(|s| {
                    s.split(|c: char| !c.is_alphanumeric())
                        .filter(|w| !w.is_empty())
                        .map(|w| w.to_lowercase())
                })
                .collect()
        }

        let mut key_lut: Vec<SearchKeyLUTEntry> = vec![];
        let mut documents: Vec<SearchDocument> = Vec::default();
        let mut fuzzy_match_candidates = Vec::default();

        fn push_candidates(
            fuzzy_match_candidates: &mut Vec<StringMatchCandidate>,
            key_index: usize,
            input: &str,
        ) {
            for word in input.split_ascii_whitespace() {
                fuzzy_match_candidates.push(StringMatchCandidate::new(key_index, word));
            }
        }

        // PERF: We are currently searching all items even in project files
        // where many settings are filtered out, using the logic in filter_matches_to_file
        // we could only search relevant items based on the current file
        for (page_index, page) in self.pages.iter().enumerate() {
            let mut header_index = 0;
            let mut header_str = "";
            for (item_index, item) in page.items.iter().enumerate() {
                let key_index = key_lut.len();
                let mut json_path = None;
                match item {
                    SettingsPageItem::DynamicItem(DynamicItem {
                        discriminant: item, ..
                    })
                    | SettingsPageItem::SettingItem(item) => {
                        json_path = item
                            .field
                            .json_path()
                            .map(|path| path.trim_end_matches('$'));
                        documents.push(SearchDocument {
                            id: key_index,
                            words: split_into_words(&[
                                page.title,
                                header_str,
                                item.title,
                                item.description,
                            ]),
                        });
                        push_candidates(&mut fuzzy_match_candidates, key_index, item.title);
                        push_candidates(&mut fuzzy_match_candidates, key_index, item.description);
                    }
                    SettingsPageItem::SectionHeader(header) => {
                        documents.push(SearchDocument {
                            id: key_index,
                            words: split_into_words(&[header]),
                        });
                        push_candidates(&mut fuzzy_match_candidates, key_index, header);
                        header_index = item_index;
                        header_str = *header;
                    }
                    SettingsPageItem::SubPageLink(sub_page_link) => {
                        json_path = sub_page_link.json_path;
                        documents.push(SearchDocument {
                            id: key_index,
                            words: split_into_words(&[
                                page.title,
                                header_str,
                                sub_page_link.title.as_ref(),
                            ]),
                        });
                        push_candidates(
                            &mut fuzzy_match_candidates,
                            key_index,
                            sub_page_link.title.as_ref(),
                        );
                    }
                    SettingsPageItem::ActionLink(action_link) => {
                        documents.push(SearchDocument {
                            id: key_index,
                            words: split_into_words(&[
                                page.title,
                                header_str,
                                action_link.title.as_ref(),
                            ]),
                        });
                        push_candidates(
                            &mut fuzzy_match_candidates,
                            key_index,
                            action_link.title.as_ref(),
                        );
                    }
                }
                push_candidates(&mut fuzzy_match_candidates, key_index, page.title);
                push_candidates(&mut fuzzy_match_candidates, key_index, header_str);

                key_lut.push(SearchKeyLUTEntry {
                    page_index,
                    header_index,
                    item_index,
                    json_path,
                });
            }
        }
        self.search_index = Some(Arc::new(SearchIndex {
            documents,
            key_lut,
            fuzzy_match_candidates,
        }));
    }

    fn build_content_handles(&mut self, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        self.content_handles = self
            .pages
            .iter()
            .map(|page| {
                std::iter::repeat_with(|| NonFocusableHandle::new(0, false, window, cx))
                    .take(page.items.len())
                    .collect()
            })
            .collect::<Vec<_>>();
    }

    fn reset_list_state(&mut self) {
        let mut visible_items_count = self.visible_page_items().count();

        if visible_items_count > 0 {
            // show page title if page is non empty
            visible_items_count += 1;
        }

        self.list_state.reset(visible_items_count);
    }

    fn build_ui(&mut self, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        if self.pages.is_empty() {
            self.pages = page_data::settings_data(cx);
            self.build_navbar(cx);
            self.setup_navbar_focus_subscriptions(window, cx);
            self.build_content_handles(window, cx);
        }
        self.sub_page_stack.clear();
        // PERF: doesn't have to be rebuilt, can just be filled with true. pages is constant once it is built
        self.build_filter_table();
        self.reset_list_state();
        self.update_matches(cx);

        cx.notify();
    }

    fn rebuild_pages(&mut self, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        self.pages.clear();
        self.navbar_entries.clear();
        self.navbar_focus_subscriptions.clear();
        self.content_handles.clear();
        self.build_ui(window, cx);
        self.build_search_index();
    }

    #[track_caller]
    fn fetch_files(&mut self, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        self.worktree_root_dirs.clear();
        let prev_files = self.files.clone();
        let settings_store = cx.global::<SettingsStore>();
        let mut ui_files = vec![];
        let mut all_files = settings_store.get_all_files();
        if !all_files.contains(&settings::SettingsFile::User) {
            all_files.push(settings::SettingsFile::User);
        }
        for file in all_files {
            let Some(settings_ui_file) = SettingsUiFile::from_settings(file) else {
                continue;
            };
            if settings_ui_file.is_server() {
                continue;
            }

            if let Some(worktree_id) = settings_ui_file.worktree_id() {
                let directory_name = all_projects(self.original_window.as_ref(), cx)
                    .find_map(|project| project.read(cx).worktree_for_id(worktree_id, cx))
                    .map(|worktree| worktree.read(cx).root_name());

                let Some(directory_name) = directory_name else {
                    log::error!(
                        "No directory name found for settings file at worktree ID: {}",
                        worktree_id
                    );
                    continue;
                };

                self.worktree_root_dirs
                    .insert(worktree_id, directory_name.as_unix_str().to_string());
            }

            let focus_handle = prev_files
                .iter()
                .find_map(|(prev_file, handle)| {
                    (prev_file == &settings_ui_file).then(|| handle.clone())
                })
                .unwrap_or_else(|| cx.focus_handle().tab_index(0).tab_stop(true));
            ui_files.push((settings_ui_file, focus_handle));
        }

        ui_files.reverse();

        if self.original_window.is_some() {
            let mut missing_worktrees = Vec::new();

            for worktree in all_projects(self.original_window.as_ref(), cx)
                .flat_map(|project| project.read(cx).visible_worktrees(cx))
                .filter(|tree| !self.worktree_root_dirs.contains_key(&tree.read(cx).id()))
            {
                let worktree = worktree.read(cx);
                let worktree_id = worktree.id();
                let Some(directory_name) = worktree.root_dir().and_then(|file| {
                    file.file_name()
                        .map(|os_string| os_string.to_string_lossy().to_string())
                }) else {
                    continue;
                };

                missing_worktrees.push((worktree_id, directory_name.clone()));
                let path = RelPath::empty().to_owned().into_arc();

                let settings_ui_file = SettingsUiFile::Project((worktree_id, path));

                let focus_handle = prev_files
                    .iter()
                    .find_map(|(prev_file, handle)| {
                        (prev_file == &settings_ui_file).then(|| handle.clone())
                    })
                    .unwrap_or_else(|| cx.focus_handle().tab_index(0).tab_stop(true));

                ui_files.push((settings_ui_file, focus_handle));
            }

            self.worktree_root_dirs.extend(missing_worktrees);
        }

        self.files = ui_files;
        let current_file_still_exists = self
            .files
            .iter()
            .any(|(file, _)| file == &self.current_file);
        if !current_file_still_exists {
            self.change_file(0, window, cx);
        }
    }
}
