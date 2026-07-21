
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

#[gpui::test]
async fn test_settings_window_shows_worktrees_from_multiple_workspaces(
    cx: &mut gpui::TestAppContext,
) {
    use project::Project;
    use serde_json::json;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
        .insert_tree(
            "/workspace1",
            json!({
                "worktree_a": {
                    "file1.rs": "fn main() {}"
                },
                "worktree_b": {
                    "file2.rs": "fn test() {}"
                }
            }),
        )
        .await;

    fake_fs
        .insert_tree(
            "/workspace2",
            json!({
                "worktree_c": {
                    "file3.rs": "fn foo() {}"
                }
            }),
        )
        .await;

    let project1 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_a", true, cx)
        })
        .await
        .expect("Failed to create worktree_a");
    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_b", true, cx)
        })
        .await
        .expect("Failed to create worktree_b");

    let project2 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project2
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace2/worktree_c", true, cx)
        })
        .await
        .expect("Failed to create worktree_c");

    let (_multi_workspace1, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project1.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let (_multi_workspace2, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project2.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let workspace2_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace2_handle), window, cx));

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let worktree_names: Vec<_> = settings_window
            .worktree_root_dirs
            .values()
            .cloned()
            .collect();

        assert!(
            worktree_names.iter().any(|name| name == "worktree_a"),
            "Should contain worktree_a from workspace1, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_b"),
            "Should contain worktree_b from workspace1, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_c"),
            "Should contain worktree_c from workspace2, but found: {:?}",
            worktree_names
        );

        assert_eq!(
            worktree_names.len(),
            3,
            "Should have exactly 3 worktrees from both workspaces, but found: {:?}",
            worktree_names
        );

        let project_files: Vec<_> = settings_window
            .files
            .iter()
            .filter_map(|(f, _)| match f {
                SettingsUiFile::Project((worktree_id, _)) => Some(*worktree_id),
                _ => None,
            })
            .collect();

        let unique_project_files: std::collections::HashSet<_> = project_files.iter().collect();
        assert_eq!(
            project_files.len(),
            unique_project_files.len(),
            "Should have no duplicate project files, but found duplicates. All files: {:?}",
            project_files
        );
    });
}

#[gpui::test]
async fn test_settings_window_updates_when_new_workspace_created(cx: &mut gpui::TestAppContext) {
    use project::Project;
    use serde_json::json;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
        .insert_tree(
            "/workspace1",
            json!({
                "worktree_a": {
                    "file1.rs": "fn main() {}"
                }
            }),
        )
        .await;

    fake_fs
        .insert_tree(
            "/workspace2",
            json!({
                "worktree_b": {
                    "file2.rs": "fn test() {}"
                }
            }),
        )
        .await;

    let project1 = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project1
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/workspace1/worktree_a", true, cx)
        })
        .await
        .expect("Failed to create worktree_a");

    let (_multi_workspace1, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project1.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    let workspace1_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace1_handle), window, cx));

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        assert_eq!(
            settings_window.worktree_root_dirs.len(),
            1,
            "Should have 1 worktree initially"
        );
    });

    let project2 = cx.update(|_, cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    project2
        .update(&mut cx.cx, |project, cx| {
            project.find_or_create_worktree("/workspace2/worktree_b", true, cx)
        })
        .await
        .expect("Failed to create worktree_b");

    let (_multi_workspace2, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project2.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let worktree_names: Vec<_> = settings_window
            .worktree_root_dirs
            .values()
            .cloned()
            .collect();

        assert!(
            worktree_names.iter().any(|name| name == "worktree_a"),
            "Should contain worktree_a, but found: {:?}",
            worktree_names
        );
        assert!(
            worktree_names.iter().any(|name| name == "worktree_b"),
            "Should contain worktree_b from newly created workspace, but found: {:?}",
            worktree_names
        );

        assert_eq!(
            worktree_names.len(),
            2,
            "Should have 2 worktrees after new workspace created, but found: {:?}",
            worktree_names
        );

        let project_files: Vec<_> = settings_window
            .files
            .iter()
            .filter_map(|(f, _)| match f {
                SettingsUiFile::Project((worktree_id, _)) => Some(*worktree_id),
                _ => None,
            })
            .collect();

        let unique_project_files: std::collections::HashSet<_> = project_files.iter().collect();
        assert_eq!(
            project_files.len(),
            unique_project_files.len(),
            "Should have no duplicate project files, but found duplicates. All files: {:?}",
            project_files
        );
    });
}

#[gpui::test]
async fn test_skills_page_scope_switch_updates_displayed_skills(cx: &mut gpui::TestAppContext) {
    use agent_skills::{
        ProjectSkillGroup, Skill, SkillScopeId, SkillSource, load_skills_from_directory,
    };
    use project::Project;
    use serde_json::json;
    use std::path::Path;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
            .insert_tree(
                "/global-skills",
                json!({
                    "global-skill": {
                        "SKILL.md": "---\nname: global-skill\ndescription: A user level skill\n---\n\nGlobal instructions."
                    }
                }),
            )
            .await;

    fake_fs
            .insert_tree(
                "/project",
                json!({
                    ".agents": {
                        "skills": {
                            "project-skill": {
                                "SKILL.md": "---\nname: project-skill\ndescription: A project level skill\n---\n\nProject instructions."
                            }
                        }
                    },
                    "main.rs": "fn main() {}"
                }),
            )
            .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    // Load both skills from the fake filesystem the same way the agent
    // does, then publish them as the global skill index.
    let fs = app_state.fs.clone();
    let global_skills: Vec<Skill> =
        load_skills_from_directory(&fs, Path::new("/global-skills"), SkillSource::Global)
            .await
            .into_iter()
            .map(|result| result.expect("global skill should load"))
            .collect();
    let project_skills: Vec<Skill> = load_skills_from_directory(
        &fs,
        Path::new("/project/.agents/skills"),
        SkillSource::ProjectLocal {
            worktree_id: SkillScopeId(worktree_id.to_usize()),
            worktree_root_name: "project".into(),
        },
    )
    .await
    .into_iter()
    .map(|result| result.expect("project skill should load"))
    .collect();
    assert_eq!(global_skills.len(), 1);
    assert_eq!(project_skills.len(), 1);

    cx.update(|cx| {
        cx.set_global(SkillIndex {
            global_skills,
            project_skills: vec![ProjectSkillGroup {
                worktree_id: SkillScopeId(worktree_id.to_usize()),
                worktree_root_name: "project".into(),
                skills: project_skills,
            }],
        });
    });

    let (_multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });
    let workspace_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace_handle), window, cx));

    cx.run_until_parked();

    settings_window.update_in(cx, |settings_window, window, cx| {
        fn displayed_skill_names(settings_window: &SettingsWindow, cx: &App) -> Vec<String> {
            crate::pages::displayed_skills(settings_window, cx)
                .iter()
                .map(|skill| skill.name.to_string())
                .collect()
        }

        assert_eq!(settings_window.current_file, SettingsUiFile::User);
        assert!(
            settings_window.navigate_to_sub_page(AGENT_SKILLS_SETTINGS_PATH, window, cx),
            "Skills sub-page should exist"
        );
        assert_eq!(displayed_skill_names(settings_window, cx), ["global-skill"]);

        let project_file_index = settings_window
            .files
            .iter()
            .position(|(file, _)| file.worktree_id() == Some(worktree_id))
            .expect("project settings file should be listed");
        settings_window.change_file_in_sub_page(project_file_index, window, cx);

        assert_eq!(
            settings_window.current_file.worktree_id(),
            Some(worktree_id)
        );
        assert_eq!(
            settings_window.sub_page_stack.len(),
            1,
            "Skills sub-page should stay open when switching scope"
        );
        assert_eq!(settings_window.sub_page_stack[0].link.title, "Skills");
        assert_eq!(
            displayed_skill_names(settings_window, cx),
            ["project-skill"]
        );

        let user_file_index = settings_window
            .files
            .iter()
            .position(|(file, _)| file == &SettingsUiFile::User)
            .expect("user settings file should be listed");
        settings_window.change_file_in_sub_page(user_file_index, window, cx);

        assert_eq!(settings_window.current_file, SettingsUiFile::User);
        assert_eq!(settings_window.sub_page_stack.len(), 1);
        assert_eq!(displayed_skill_names(settings_window, cx), ["global-skill"]);
    });
}

#[gpui::test]
async fn test_open_skill_creator_navigates_to_sub_page(cx: &mut gpui::TestAppContext) {
    use project::Project;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    app_state
        .fs
        .as_fake()
        .insert_tree("/project", serde_json::json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");

    let (_multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });
    let workspace_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace_handle), window, cx));

    cx.run_until_parked();

    settings_window.update_in(cx, |settings_window, window, cx| {
        settings_window.navigate_to_skill_creator(pages::SkillCreatorOpenMode::Form, window, cx);
    });

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let titles: Vec<_> = settings_window
            .sub_page_stack
            .iter()
            .map(|sub_page| sub_page.link.title.to_string())
            .collect();
        assert_eq!(
            titles,
            ["Skills", "Create Skill"],
            "skill creator should be pushed on top of the skills page"
        );
        assert!(
            settings_window.skill_creator_page().is_some(),
            "skill creator page state should exist"
        );
    });
}

#[gpui::test]
async fn test_open_skill_creator_action_opens_settings_window_at_sub_page(
    cx: &mut gpui::TestAppContext,
) {
    use project::Project;

    cx.update(|cx| {
        register_settings(cx);
        release_channel::init("0.0.0".parse().unwrap(), cx);
        crate::init(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    app_state
        .fs
        .as_fake()
        .insert_tree("/project", serde_json::json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    cx.run_until_parked();

    // Dispatch the action the way the command palette does: on the
    // workspace window.
    multi_workspace.update_in(cx, |_multi_workspace, window, cx| {
        window.dispatch_action(Box::new(mav_actions::assistant::OpenSkillCreator), cx);
    });

    cx.run_until_parked();

    let settings_window = cx
        .update(|_, cx| {
            cx.windows()
                .into_iter()
                .find_map(|window| window.downcast::<SettingsWindow>())
        })
        .expect("dispatching agent::OpenSkillCreator should open the settings window");

    settings_window
        .read_with(cx, |settings_window, _| {
            let titles: Vec<_> = settings_window
                .sub_page_stack
                .iter()
                .map(|sub_page| sub_page.link.title.to_string())
                .collect();
            assert_eq!(
                titles,
                ["Skills", "Create Skill"],
                "skill creator should be pushed on top of the skills page"
            );
        })
        .unwrap();
}
