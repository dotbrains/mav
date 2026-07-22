use super::*;

impl SettingsWindow {
    fn new(
        original_window: Option<WindowHandle<MultiWorkspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let font_family_cache = theme::FontFamilyCache::global(cx);

        cx.spawn(async move |this, cx| {
            font_family_cache.prefetch(cx).await;
            this.update(cx, |_, cx| {
                cx.notify();
            })
        })
        .detach();

        let current_file = SettingsUiFile::User;
        let search_bar = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Search settings…", window, cx);
            editor
        });
        cx.subscribe(&search_bar, |this, _, event: &EditorEvent, cx| {
            let EditorEvent::Edited { transaction_id: _ } = event else {
                return;
            };

            if this.opening_link {
                this.opening_link = false;
                return;
            }
            this.update_matches(cx);
        })
        .detach();

        let mut ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        cx.observe_global_in::<SettingsStore>(window, move |this, window, cx| {
            this.fetch_files(window, cx);

            // Whenever settings are changed, it's possible that the changed
            // settings affects the rendering of the `SettingsWindow`, like is
            // the case with `ui_font_size`. When that happens, we need to
            // instruct the `ListState` to re-measure the list items, as the
            // list item heights may have changed depending on the new font
            // size.
            let new_ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
            if new_ui_font_size != ui_font_size {
                this.list_state.remeasure();
                ui_font_size = new_ui_font_size;
            }

            cx.notify();
        })
        .detach();

        use feature_flags::FeatureFlagAppExt as _;
        let mut last_is_staff = cx.is_staff();
        cx.observe_global_in::<feature_flags::FeatureFlagStore>(window, move |this, window, cx| {
            let is_staff = cx.is_staff();
            if is_staff != last_is_staff {
                last_is_staff = is_staff;
                this.rebuild_pages(window, cx);
            }
        })
        .detach();

        cx.observe_global_in::<SkillIndex>(window, |this, _window, cx| {
            if let Some(skill_index) = cx.try_global::<SkillIndex>() {
                this.hidden_deleted_skill_directory_paths
                    .retain(|directory_path| {
                        skill_index
                            .global_skills
                            .iter()
                            .chain(
                                skill_index
                                    .project_skills
                                    .iter()
                                    .flat_map(|group| group.skills.iter()),
                            )
                            .any(|skill| skill.directory_path.as_path() == directory_path.as_path())
                    });
            } else {
                this.hidden_deleted_skill_directory_paths.clear();
            }
            cx.notify();
        })
        .detach();

        cx.on_window_closed(|cx, _window_id| {
            if let Some(existing_window) = cx
                .windows()
                .into_iter()
                .find_map(|window| window.downcast::<SettingsWindow>())
                && cx.windows().len() == 1
            {
                cx.update_window(*existing_window, |_, window, _| {
                    window.remove_window();
                })
                .ok();

                telemetry::event!("Settings Closed")
            }
        })
        .detach();

        let app_state = AppState::global(cx);
        let workspaces: Vec<Entity<Workspace>> = app_state
            .workspace_store
            .read(cx)
            .workspaces()
            .filter_map(|weak| weak.upgrade())
            .collect();

        for workspace in workspaces {
            let project = workspace.read(cx).project().clone();
            cx.observe_release_in(&project, window, |this, _, window, cx| {
                this.fetch_files(window, cx)
            })
            .detach();
            cx.subscribe_in(&project, window, Self::handle_project_event)
                .detach();
            cx.observe_release_in(&workspace, window, |this, _, window, cx| {
                this.fetch_files(window, cx)
            })
            .detach();
        }

        let this_weak = cx.weak_entity();
        cx.observe_new::<Project>({
            let this_weak = this_weak.clone();

            move |_, window, cx| {
                let project = cx.entity();
                let Some(window) = window else {
                    return;
                };

                this_weak
                    .update(cx, |_, cx| {
                        cx.defer_in(window, |settings_window, window, cx| {
                            settings_window.fetch_files(window, cx)
                        });
                        cx.observe_release_in(&project, window, |_, _, window, cx| {
                            cx.defer_in(window, |this, window, cx| this.fetch_files(window, cx));
                        })
                        .detach();

                        cx.subscribe_in(&project, window, Self::handle_project_event)
                            .detach();
                    })
                    .ok();
            }
        })
        .detach();

        let handle = window.window_handle();
        cx.observe_new::<Workspace>(move |workspace, _, cx| {
            let project = workspace.project().clone();
            let this_weak = this_weak.clone();

            // We defer on the settings window (via `handle`) rather than using
            // the workspace's window from observe_new. When window.defer() runs
            // its callback, it calls handle.update() which temporarily removes
            // that window from cx.windows. If we deferred on the workspace's
            // window, then when fetch_files() tries to read ALL workspaces from
            // the store (including the newly created one), it would fail with
            // "window not found" because that workspace's window would be
            // temporarily removed from cx.windows for the duration of our callback.
            handle
                .update(cx, move |_, window, cx| {
                    window.defer(cx, move |window, cx| {
                        this_weak
                            .update(cx, |this, cx| {
                                this.fetch_files(window, cx);
                                cx.observe_release_in(&project, window, |this, _, window, cx| {
                                    this.fetch_files(window, cx)
                                })
                                .detach();
                            })
                            .ok();
                    });
                })
                .ok();
        })
        .detach();

        let title_bar = if !cfg!(target_os = "macos") {
            Some(cx.new(|cx| PlatformTitleBar::new("settings-title-bar", cx)))
        } else {
            None
        };

        let list_state = gpui::ListState::new(0, gpui::ListAlignment::Top, px(0.0)).measure_all();
        list_state.set_scroll_handler(|_, _, _| {});

        let mut this = Self {
            title_bar,
            original_window,

            worktree_root_dirs: HashMap::default(),
            files: vec![],

            current_file: current_file,
            project_setting_file_buffers: HashMap::default(),
            pages: vec![],
            sub_page_stack: vec![],
            opening_link: false,
            navbar_entries: vec![],
            navbar_entry: 0,
            navbar_scroll_handle: UniformListScrollHandle::default(),
            search_bar,
            search_task: None,
            filter_table: vec![],
            has_query: false,
            content_handles: vec![],
            focus_handle: cx.focus_handle(),
            navbar_focus_handle: NonFocusableHandle::new(
                NAVBAR_CONTAINER_TAB_INDEX,
                false,
                window,
                cx,
            ),
            navbar_focus_subscriptions: vec![],
            content_focus_handle: NonFocusableHandle::new(
                CONTENT_CONTAINER_TAB_INDEX,
                false,
                window,
                cx,
            ),
            files_focus_handle: cx
                .focus_handle()
                .tab_index(HEADER_CONTAINER_TAB_INDEX)
                .tab_stop(false),
            search_index: None,
            shown_errors: HashSet::default(),
            hidden_deleted_skill_directory_paths: HashSet::default(),
            regex_validation_error: None,
            sandbox_host_validation_error: None,
            list_state,
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

        this.fetch_files(window, cx);
        this.build_ui(window, cx);
        this.build_search_index();

        this.search_bar.update(cx, |editor, cx| {
            editor.focus_handle(cx).focus(window, cx);
        });

        this
    }

    fn handle_project_event(
        &mut self,
        _: &Entity<Project>,
        event: &project::Event,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) {
        match event {
            project::Event::WorktreeRemoved(_) | project::Event::WorktreeAdded(_) => {
                cx.defer_in(window, |this, window, cx| {
                    this.fetch_files(window, cx);
                });
            }
            _ => {}
        }
    }
}
