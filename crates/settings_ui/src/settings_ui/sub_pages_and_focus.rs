use super::*;

impl SettingsWindow {
    fn current_page_index(&self) -> usize {
        if self.navbar_entries.is_empty() {
            return 0;
        }

        self.navbar_entries[self.navbar_entry].page_index
    }

    fn current_page(&self) -> &SettingsPage {
        &self.pages[self.current_page_index()]
    }

    fn is_navbar_entry_selected(&self, ix: usize) -> bool {
        ix == self.navbar_entry
    }

    fn push_sub_page(
        &mut self,
        sub_page_link: SubPageLink,
        section_header: SharedString,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        self.sandbox_host_validation_error = None;
        self.sub_page_stack
            .push(SubPage::new(sub_page_link, section_header));
        self.content_focus_handle.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// Push a dynamically-created sub-page with a custom render function.
    /// This is useful for nested sub-pages that aren't defined in the main pages list.
    pub fn push_dynamic_sub_page(
        &mut self,
        title: impl Into<SharedString>,
        section_header: impl Into<SharedString>,
        json_path: Option<&'static str>,
        in_json: bool,
        render: fn(
            &SettingsWindow,
            &ScrollHandle,
            &mut Window,
            &mut Context<SettingsWindow>,
        ) -> AnyElement,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        self.regex_validation_error = None;
        let sub_page_link = SubPageLink {
            title: title.into(),
            r#type: SubPageType::default(),
            description: None,
            json_path,
            in_json,
            files: USER,
            render,
        };
        self.push_sub_page(sub_page_link, section_header.into(), window, cx);
    }

    pub(crate) fn skill_creator_page(&self) -> Option<Entity<pages::SkillCreatorPage>> {
        self.skill_creator_page
            .as_ref()
            .map(|(page, _)| page.clone())
    }

    /// If the creator is already the active sub-page, the open mode is applied
    /// to the existing form instead
    pub fn open_skill_creator_sub_page(
        &mut self,
        open_mode: pages::SkillCreatorOpenMode,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        let creator_is_active_sub_page = self
            .sub_page_stack
            .last()
            .is_some_and(|sub_page| sub_page.link.r#type == SubPageType::SkillCreator);

        if creator_is_active_sub_page && let Some((page, _)) = &self.skill_creator_page {
            let page = page.clone();
            page.update(cx, |page, cx| page.apply_open_mode(open_mode, window, cx));
            return;
        }

        let settings_window = cx.weak_entity();
        let page = cx.new(|cx| pages::SkillCreatorPage::new(settings_window, window, cx));

        let subscription =
            cx.subscribe_in(
                &page,
                window,
                |this, _page, event: &pages::SkillCreatorEvent, window, cx| match event {
                    pages::SkillCreatorEvent::Dismissed | pages::SkillCreatorEvent::Saved => {
                        if this.sub_page_stack.last().is_some_and(|sub_page| {
                            sub_page.link.r#type == SubPageType::SkillCreator
                        }) {
                            this.pop_sub_page(window, cx);
                        }
                    }
                },
            );

        self.skill_creator_page = Some((page.clone(), subscription));

        let sub_page_link = SubPageLink {
            title: "Create Skill".into(),
            r#type: SubPageType::SkillCreator,
            description: None,
            json_path: None,
            in_json: false,
            files: USER | PROJECT,
            render: pages::render_skill_creator_page,
        };

        self.push_sub_page(sub_page_link, "Agent".into(), window, cx);

        let creating_from_url = !matches!(open_mode, pages::SkillCreatorOpenMode::Url { .. });
        page.update(cx, |page, cx| {
            page.apply_open_mode(open_mode, window, cx);
        });
        if creating_from_url {
            let name_editor_focus_handle = page.read(cx).name_editor_focus_handle(cx);
            window.focus(&name_editor_focus_handle, cx);
        }
    }

    pub fn navigate_to_skill_creator(
        &mut self,
        open_mode: pages::SkillCreatorOpenMode,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        self.sub_page_stack.clear();
        let skills_page_index = self.pages.iter().position(|page| {
            page.items.iter().any(|item| {
                matches!(
                    item,
                    SettingsPageItem::SubPageLink(link)
                        if link.json_path == Some(AGENT_SKILLS_SETTINGS_PATH)
                )
            })
        });
        if let Some(page_index) = skills_page_index
            && let Some(navbar_entry_index) = self
                .navbar_entries
                .iter()
                .position(|entry| entry.page_index == page_index && entry.is_root)
        {
            self.open_navbar_entry_page(navbar_entry_index);
        }
        self.navigate_to_sub_page(AGENT_SKILLS_SETTINGS_PATH, window, cx);
        self.open_skill_creator_sub_page(open_mode, window, cx);
    }

    /// Navigate to a sub-page by its json_path.
    /// Returns true if the sub-page was found and pushed, false otherwise.
    pub fn navigate_to_sub_page(
        &mut self,
        json_path: &str,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> bool {
        for page in &self.pages {
            for (item_index, item) in page.items.iter().enumerate() {
                if let SettingsPageItem::SubPageLink(sub_page_link) = item {
                    if sub_page_link.json_path == Some(json_path) {
                        let section_header = page
                            .items
                            .iter()
                            .take(item_index)
                            .rev()
                            .find_map(|item| item.header_text().map(SharedString::new_static))
                            .unwrap_or_else(|| "Settings".into());

                        self.push_sub_page(sub_page_link.clone(), section_header, window, cx);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Navigate to a setting by its json_path.
    /// Clears the sub-page stack and scrolls to the setting item.
    /// Returns true if the setting was found, false otherwise.
    pub fn navigate_to_setting(
        &mut self,
        json_path: &str,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> bool {
        self.sub_page_stack.clear();

        for (page_index, page) in self.pages.iter().enumerate() {
            for (item_index, item) in page.items.iter().enumerate() {
                let item_json_path = match item {
                    SettingsPageItem::SettingItem(setting_item) => setting_item.field.json_path(),
                    SettingsPageItem::DynamicItem(dynamic_item) => {
                        dynamic_item.discriminant.field.json_path()
                    }
                    _ => None,
                };
                if item_json_path == Some(json_path) {
                    if let Some(navbar_entry_index) = self
                        .navbar_entries
                        .iter()
                        .position(|e| e.page_index == page_index && e.is_root)
                    {
                        self.open_and_scroll_to_navbar_entry(
                            navbar_entry_index,
                            None,
                            false,
                            window,
                            cx,
                        );
                        self.scroll_to_content_item(item_index, window, cx);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn pop_sub_page(&mut self, window: &mut Window, cx: &mut Context<SettingsWindow>) {
        self.regex_validation_error = None;
        self.sandbox_host_validation_error = None;
        if let Some(popped) = self.sub_page_stack.pop()
            && popped.link.r#type == SubPageType::SkillCreator
        {
            self.skill_creator_page = None;
        }
        self.content_focus_handle.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn focus_file_at_index(&mut self, index: usize, window: &mut Window, cx: &mut App) {
        if let Some((_, handle)) = self.files.get(index) {
            handle.focus(window, cx);
        }
    }

    fn focused_file_index(&self, window: &Window, cx: &Context<Self>) -> usize {
        if self.files_focus_handle.contains_focused(window, cx)
            && let Some(index) = self
                .files
                .iter()
                .position(|(_, handle)| handle.is_focused(window))
        {
            return index;
        }
        if let Some(current_file_index) = self
            .files
            .iter()
            .position(|(file, _)| file == &self.current_file)
        {
            return current_file_index;
        }
        0
    }

    fn focus_handle_for_content_element(
        &self,
        actual_item_index: usize,
        cx: &Context<Self>,
    ) -> FocusHandle {
        let page_index = self.current_page_index();
        self.content_handles[page_index][actual_item_index].focus_handle(cx)
    }

    fn focused_nav_entry(&self, window: &Window, cx: &App) -> Option<usize> {
        if !self
            .navbar_focus_handle
            .focus_handle(cx)
            .contains_focused(window, cx)
        {
            return None;
        }
        for (index, entry) in self.navbar_entries.iter().enumerate() {
            if entry.focus_handle.is_focused(window) {
                return Some(index);
            }
        }
        None
    }

    fn root_entry_containing(&self, nav_entry_index: usize) -> usize {
        let mut index = Some(nav_entry_index);
        while let Some(prev_index) = index
            && !self.navbar_entries[prev_index].is_root
        {
            index = prev_index.checked_sub(1);
        }
        return index.expect("No root entry found");
    }
}
