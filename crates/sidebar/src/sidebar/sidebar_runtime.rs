use super::*;

impl WorkspaceSidebar for Sidebar {
    fn width(&self, _cx: &App) -> Pixels {
        self.width
    }

    fn set_width(&mut self, width: Option<Pixels>, cx: &mut Context<Self>) {
        let width = width.unwrap_or(DEFAULT_WIDTH).clamp(MIN_WIDTH, MAX_WIDTH);
        if self.width == width {
            return;
        }
        self.width = width;
        self.serialize(cx);
        cx.notify();
    }

    fn has_notifications(&self, _cx: &App) -> bool {
        !self.contents.notified_threads.is_empty() || !self.contents.notified_terminals.is_empty()
    }

    fn is_threads_list_view_active(&self) -> bool {
        matches!(self.view, SidebarView::ThreadList)
    }

    fn side(&self, cx: &App) -> SidebarSide {
        SidebarSettings::get_global(cx).side()
    }

    fn prepare_for_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.selection = None;
        cx.notify();
    }

    fn toggle_thread_switcher(
        &mut self,
        select_last: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_thread_switcher_impl(select_last, window, cx);
    }

    fn cycle_project(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_project_impl(forward, window, cx);
    }

    fn cycle_thread(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_thread_impl(forward, window, cx);
    }

    fn toggle_options_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.agent_options_menu_handle.toggle(window, cx);
    }

    fn simulate_update_available(&mut self, cx: &mut Context<Self>) {
        self.sidebar_chrome.update(cx, |sidebar_chrome, cx| {
            sidebar_chrome.toggle_update_simulation(cx);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn open_application_menu(
        &mut self,
        menu_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_chrome.update(cx, |sidebar_chrome, cx| {
            sidebar_chrome.open_application_menu(menu_name, cx);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn activate_application_menu(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_chrome.update(cx, |sidebar_chrome, cx| {
            sidebar_chrome.activate_application_menu(right, window, cx);
        });
    }

    fn serialized_state(&self, _cx: &App) -> Option<String> {
        let serialized = SerializedSidebar {
            width: Some(f32::from(self.width)),
            active_view: match self.view {
                SidebarView::ThreadList => SerializedSidebarView::ThreadList,
                SidebarView::Archive(_) => SerializedSidebarView::History,
            },
        };
        serde_json::to_string(&serialized).ok()
    }

    fn restore_serialized_state(
        &mut self,
        state: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(serialized) = serde_json::from_str::<SerializedSidebar>(state).log_err() {
            if let Some(width) = serialized.width {
                self.width = px(width).clamp(MIN_WIDTH, MAX_WIDTH);
            }
            if serialized.active_view == SerializedSidebarView::History {
                cx.defer_in(window, |this, window, cx| {
                    this.show_archive(window, cx);
                });
            }
        }
        cx.notify();
    }
}

impl gpui::EventEmitter<workspace::SidebarEvent> for Sidebar {}

impl Focusable for Sidebar {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme_settings::setup_ui_font(window, cx);
        let sticky_header = self.render_sticky_header(window, cx);

        let color = cx.theme().colors();
        let bg = color.editor_background;

        let no_open_projects = !self.contents.has_open_projects;
        let no_search_results = self.contents.entries.is_empty();

        v_flex()
            .id("workspace-sidebar")
            .key_context(self.dispatch_context(window, cx))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::editor_move_down))
            .on_action(cx.listener(Self::editor_move_up))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::toggle_selected_fold))
            .on_action(cx.listener(Self::fold_all))
            .on_action(cx.listener(Self::unfold_all))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::archive_selected_thread))
            .on_action(cx.listener(Self::rename_selected_thread))
            .on_action(cx.listener(Self::new_thread_in_group))
            .on_action(cx.listener(Self::new_terminal_thread))
            .on_action(cx.listener(Self::toggle_archive))
            .on_action(cx.listener(Self::focus_sidebar_filter))
            .on_action(cx.listener(Self::on_toggle_thread_switcher))
            .on_action(cx.listener(Self::on_next_project))
            .on_action(cx.listener(Self::on_previous_project))
            .on_action(cx.listener(Self::on_next_thread))
            .on_action(cx.listener(Self::on_previous_thread))
            .on_action(cx.listener(Self::toggle_agent_options_menu))
            .on_action(cx.listener(|this, _: &OpenRecent, window, cx| {
                this.recent_projects_popover_handle.toggle(window, cx);
            }))
            .font(ui_font)
            .h_full()
            .w(self.width)
            .bg(bg)
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(color.border)
            .child(self.render_sidebar_header(window, cx))
            .map(|this| match &self.view {
                SidebarView::ThreadList => this.map(|this| {
                    if no_open_projects {
                        this.child(self.render_empty_state(cx))
                    } else {
                        this.child(
                            v_flex()
                                .relative()
                                .flex_1()
                                .overflow_hidden()
                                .child(
                                    list(
                                        self.list_state.clone(),
                                        cx.processor(Self::render_list_entry),
                                    )
                                    .flex_1()
                                    .size_full(),
                                )
                                .when(no_search_results, |this| {
                                    this.child(self.render_no_results(cx))
                                })
                                .when_some(sticky_header, |this, header| this.child(header))
                                .custom_scrollbars(
                                    Scrollbars::new(ScrollAxes::Vertical)
                                        .tracked_scroll_handle(&self.list_state),
                                    window,
                                    cx,
                                ),
                        )
                    }
                }),
                SidebarView::Archive(archive_view) => this.child(archive_view.clone()),
            })
            .map(|this| {
                let show_acp = self.should_render_acp_import_onboarding(cx);
                let show_cross_channel = self.should_render_cross_channel_import_onboarding(cx);

                let verbose = *self
                    .import_banners_use_verbose_labels
                    .get_or_insert(show_acp && show_cross_channel);

                this.when(show_acp, |this| {
                    this.child(self.render_acp_import_onboarding(verbose, cx))
                })
                .when(show_cross_channel, |this| {
                    this.child(self.render_cross_channel_import_onboarding(verbose, cx))
                })
            })
            .child(self.render_sidebar_bottom_bar(cx))
    }
}
