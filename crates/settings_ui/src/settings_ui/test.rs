use super::*;

impl SettingsWindow {
    fn navbar_entry(&self) -> usize {
        self.navbar_entry
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_bar = cx.new(|cx| Editor::single_line(window, cx));
        let dummy_page = SettingsPage {
            title: "Test",
            items: Box::new([]),
        };
        Self {
            title_bar: None,
            original_window: None,
            worktree_root_dirs: HashMap::default(),
            files: Vec::default(),
            current_file: SettingsUiFile::User,
            project_setting_file_buffers: HashMap::default(),
            pages: vec![dummy_page],
            search_bar,
            navbar_entry: 0,
            navbar_entries: Vec::default(),
            navbar_scroll_handle: UniformListScrollHandle::default(),
            navbar_focus_subscriptions: Vec::default(),
            filter_table: Vec::default(),
            has_query: false,
            content_handles: Vec::default(),
            search_task: None,
            sub_page_stack: Vec::default(),
            opening_link: false,
            focus_handle: cx.focus_handle(),
            navbar_focus_handle: NonFocusableHandle::new(
                NAVBAR_CONTAINER_TAB_INDEX,
                false,
                window,
                cx,
            ),
            content_focus_handle: NonFocusableHandle::new(
                CONTENT_CONTAINER_TAB_INDEX,
                false,
                window,
                cx,
            ),
            files_focus_handle: cx.focus_handle(),
            search_index: None,
            list_state: ListState::new(0, gpui::ListAlignment::Top, px(0.0)),
            shown_errors: HashSet::default(),
            hidden_deleted_skill_directory_paths: HashSet::default(),
            regex_validation_error: None,
            sandbox_host_validation_error: None,
            last_copied_link_path: None,
            provider_configuration_views: HashMap::default(),
            configuring_provider: None,
            last_copied_skill_directory_path: None,
            mcp_server_form: None,
            mcp_add_server_focus_handle: cx.focus_handle(),
            custom_agent_form: None,
            external_agent_add_focus_handle: cx.focus_handle(),
            skill_creator_page: None,
        }
    }
}

impl PartialEq for NavBarEntry {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.is_root == other.is_root
            && self.expanded == other.expanded
            && self.page_index == other.page_index
            && self.item_index == other.item_index
        // ignoring focus_handle
    }
}

pub fn register_settings(cx: &mut App) {
    settings::init(cx);
    theme_settings::init(theme::LoadThemes::JustBase, cx);
    editor::init(cx);
    menu::init();
}

fn parse(input: &'static str, window: &mut Window, cx: &mut App) -> SettingsWindow {
    struct PageBuilder {
        title: &'static str,
        items: Vec<SettingsPageItem>,
    }
    let mut page_builders: Vec<PageBuilder> = Vec::new();
    let mut expanded_pages = Vec::new();
    let mut selected_idx = None;
    let mut index = 0;
    let mut in_expanded_section = false;

    for mut line in input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
    {
        if let Some(pre) = line.strip_suffix('*') {
            assert!(selected_idx.is_none(), "Only one selected entry allowed");
            selected_idx = Some(index);
            line = pre;
        }
        let (kind, title) = line.split_once(" ").unwrap();
        assert_eq!(kind.len(), 1);
        let kind = kind.chars().next().unwrap();
        if kind == 'v' {
            let page_idx = page_builders.len();
            expanded_pages.push(page_idx);
            page_builders.push(PageBuilder {
                title,
                items: vec![],
            });
            index += 1;
            in_expanded_section = true;
        } else if kind == '>' {
            page_builders.push(PageBuilder {
                title,
                items: vec![],
            });
            index += 1;
            in_expanded_section = false;
        } else if kind == '-' {
            page_builders
                .last_mut()
                .unwrap()
                .items
                .push(SettingsPageItem::SectionHeader(title));
            if selected_idx == Some(index) && !in_expanded_section {
                panic!("Items in unexpanded sections cannot be selected");
            }
            index += 1;
        } else {
            panic!(
                "Entries must start with one of 'v', '>', or '-'\n line: {}",
                line
            );
        }
    }

    let pages: Vec<SettingsPage> = page_builders
        .into_iter()
        .map(|builder| SettingsPage {
            title: builder.title,
            items: builder.items.into_boxed_slice(),
        })
        .collect();

    let mut settings_window = SettingsWindow {
        title_bar: None,
        original_window: None,
        worktree_root_dirs: HashMap::default(),
        files: Vec::default(),
        current_file: crate::SettingsUiFile::User,
        project_setting_file_buffers: HashMap::default(),
        pages,
        search_bar: cx.new(|cx| Editor::single_line(window, cx)),
        navbar_entry: selected_idx.expect("Must have a selected navbar entry"),
        navbar_entries: Vec::default(),
        navbar_scroll_handle: UniformListScrollHandle::default(),
        navbar_focus_subscriptions: vec![],
        filter_table: vec![],
        sub_page_stack: vec![],
        opening_link: false,
        has_query: false,
        content_handles: vec![],
        search_task: None,
        focus_handle: cx.focus_handle(),
        navbar_focus_handle: NonFocusableHandle::new(NAVBAR_CONTAINER_TAB_INDEX, false, window, cx),
        content_focus_handle: NonFocusableHandle::new(
            CONTENT_CONTAINER_TAB_INDEX,
            false,
            window,
            cx,
        ),
        files_focus_handle: cx.focus_handle(),
        search_index: None,
        list_state: ListState::new(0, gpui::ListAlignment::Top, px(0.0)),
        shown_errors: HashSet::default(),
        hidden_deleted_skill_directory_paths: HashSet::default(),
        regex_validation_error: None,
        sandbox_host_validation_error: None,
        last_copied_link_path: None,
        provider_configuration_views: HashMap::default(),
        configuring_provider: None,
        last_copied_skill_directory_path: None,
        mcp_server_form: None,
        mcp_add_server_focus_handle: cx.focus_handle(),
        custom_agent_form: None,
        external_agent_add_focus_handle: cx.focus_handle(),
        skill_creator_page: None,
    };

    settings_window.build_filter_table();
    settings_window.build_navbar(cx);
    for expanded_page_index in expanded_pages {
        for entry in &mut settings_window.navbar_entries {
            if entry.page_index == expanded_page_index && entry.is_root {
                entry.expanded = true;
            }
        }
    }
    settings_window
}

#[track_caller]
fn check_navbar_toggle(
    before: &'static str,
    toggle_page: &'static str,
    after: &'static str,
    window: &mut Window,
    cx: &mut App,
) {
    let mut settings_window = parse(before, window, cx);
    let toggle_page_idx = settings_window
        .pages
        .iter()
        .position(|page| page.title == toggle_page)
        .expect("page not found");
    let toggle_idx = settings_window
        .navbar_entries
        .iter()
        .position(|entry| entry.page_index == toggle_page_idx)
        .expect("page not found");
    settings_window.toggle_navbar_entry(toggle_idx);

    let expected_settings_window = parse(after, window, cx);

    pretty_assertions::assert_eq!(
        settings_window
            .visible_navbar_entries()
            .map(|(_, entry)| entry)
            .collect::<Vec<_>>(),
        expected_settings_window
            .visible_navbar_entries()
            .map(|(_, entry)| entry)
            .collect::<Vec<_>>(),
    );
    pretty_assertions::assert_eq!(
        settings_window.navbar_entries[settings_window.navbar_entry()],
        expected_settings_window.navbar_entries[expected_settings_window.navbar_entry()],
    );
}

macro_rules! check_navbar_toggle {
    ($name:ident, before: $before:expr, toggle_page: $toggle_page:expr, after: $after:expr) => {
        #[gpui::test]
        fn $name(cx: &mut gpui::TestAppContext) {
            let window = cx.add_empty_window();
            window.update(|window, cx| {
                register_settings(cx);
                check_navbar_toggle($before, $toggle_page, $after, window, cx);
            });
        }
    };
}

check_navbar_toggle!(
    navbar_basic_open,
    before: r"
        v General
        - General
        - Privacy*
        v Project
        - Project Settings
        ",
    toggle_page: "General",
    after: r"
        > General*
        v Project
        - Project Settings
        "
);

check_navbar_toggle!(
    navbar_basic_close,
    before: r"
        > General*
        - General
        - Privacy
        v Project
        - Project Settings
        ",
    toggle_page: "General",
    after: r"
        v General*
        - General
        - Privacy
        v Project
        - Project Settings
        "
);

check_navbar_toggle!(
    navbar_basic_second_root_entry_close,
    before: r"
        > General
        - General
        - Privacy
        v Project
        - Project Settings*
        ",
    toggle_page: "Project",
    after: r"
        > General
        > Project*
        "
);

check_navbar_toggle!(
    navbar_toggle_subroot,
    before: r"
        v General Page
        - General
        - Privacy
        v Project
        - Worktree Settings Content*
        v AI
        - General
        > Appearance & Behavior
        ",
    toggle_page: "Project",
    after: r"
        v General Page
        - General
        - Privacy
        > Project*
        v AI
        - General
        > Appearance & Behavior
        "
);

check_navbar_toggle!(
    navbar_toggle_close_propagates_selected_index,
    before: r"
        v General Page
        - General
        - Privacy
        v Project
        - Worktree Settings Content
        v AI
        - General*
        > Appearance & Behavior
        ",
    toggle_page: "General Page",
    after: r"
        > General Page*
        v Project
        - Worktree Settings Content
        v AI
        - General
        > Appearance & Behavior
        "
);

check_navbar_toggle!(
    navbar_toggle_expand_propagates_selected_index,
    before: r"
        > General Page
        - General
        - Privacy
        v Project
        - Worktree Settings Content
        v AI
        - General*
        > Appearance & Behavior
        ",
    toggle_page: "General Page",
    after: r"
        v General Page*
        - General
        - Privacy
        v Project
        - Worktree Settings Content
        v AI
        - General
        > Appearance & Behavior
        "
);

#[gpui::test]
fn navbar_double_click_toggle(cx: &mut gpui::TestAppContext) {
    let (settings_window, cx) = cx.add_window_view(|window, cx| {
        register_settings(cx);
        let mut settings_window = parse(
            r"
                > General*
                - General
                - Privacy
                v Project
                - Project Settings
                ",
            window,
            cx,
        );
        settings_window.build_content_handles(window, cx);
        settings_window
    });

    settings_window.update_in(cx, |settings_window, window, cx| {
        let general_idx = settings_window
            .navbar_entries
            .iter()
            .position(|entry| entry.title == "General" && entry.is_root)
            .expect("General root entry should exist");
        let privacy_idx = settings_window
            .navbar_entries
            .iter()
            .position(|entry| entry.title == "Privacy" && !entry.is_root)
            .expect("Privacy nested entry should exist");

        let click_event = |click_count| {
            gpui::ClickEvent::Mouse(gpui::MouseClickEvent {
                down: gpui::MouseDownEvent {
                    button: gpui::MouseButton::Left,
                    click_count,
                    ..Default::default()
                },
                up: gpui::MouseUpEvent {
                    button: gpui::MouseButton::Left,
                    click_count,
                    ..Default::default()
                },
            })
        };

        assert!(
            !settings_window.toggle_navbar_entry_on_double_click(
                general_idx,
                &click_event(1),
                window,
                cx,
            ),
            "single-clicks should use the normal navigation path"
        );
        assert!(!settings_window.navbar_entries[general_idx].expanded);

        assert!(settings_window.toggle_navbar_entry_on_double_click(
            general_idx,
            &click_event(2),
            window,
            cx,
        ));
        assert!(settings_window.navbar_entries[general_idx].expanded);

        assert!(
            !settings_window.toggle_navbar_entry_on_double_click(
                general_idx,
                &click_event(3),
                window,
                cx,
            ),
            "triple-clicks should not toggle the entry again"
        );
        assert!(settings_window.navbar_entries[general_idx].expanded);

        assert!(!settings_window.toggle_navbar_entry_on_double_click(
            privacy_idx,
            &click_event(2),
            window,
            cx,
        ));
    });
}

mod skill_tests;
mod workspace_tests;
