use super::*;

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        next_timestamp: Arc<AtomicUsize>,
        can_drop_predicate: Option<Arc<dyn Fn(&dyn Any, &mut Window, &mut App) -> bool + 'static>>,
        double_click_dispatch_action: Box<dyn Action>,
        use_max_tabs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let max_tabs = if use_max_tabs {
            WorkspaceSettings::get_global(cx).max_tabs
        } else {
            None
        };

        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Pane::focus_in),
            cx.on_focus_in(&focus_handle, window, Pane::focus_in),
            cx.on_focus_out(&focus_handle, window, Pane::focus_out),
            cx.observe_global_in::<SettingsStore>(window, Self::settings_changed),
            cx.subscribe(&project, Self::project_events),
        ];

        let handle = cx.entity().downgrade();

        Self {
            alternate_file_items: (None, None),
            focus_handle,
            items: Vec::new(),
            activation_history: Vec::new(),
            next_activation_timestamp: next_timestamp.clone(),
            was_focused: false,
            zoomed: false,
            active_item_index: 0,
            preview_item_id: None,
            max_tabs,
            use_max_tabs,
            last_focus_handle_by_item: Default::default(),
            nav_history: NavHistory::new(handle, next_timestamp),
            toolbar: cx.new(|_| Toolbar::new()),
            tab_bar_scroll_handle: ScrollHandle::new(),
            suppress_scroll: false,
            drag_split_direction: None,
            drag_swap_target: false,
            drag_tab_target: false,
            drag_tab_insertion_target: None,
            workspace,
            project: project.downgrade(),
            can_drop_predicate,
            can_split_predicate: None,
            can_toggle_zoom: true,
            should_display_tab_bar: Rc::new(|_, cx| TabBarSettings::get_global(cx).show),
            should_display_welcome_page: false,
            render_tab_bar_buttons: Rc::new(pane_render::default_render_tab_bar_buttons),
            render_tab_bar: Rc::new(Self::render_tab_bar),
            show_tab_bar_buttons: TabBarSettings::get_global(cx).show_tab_bar_buttons,
            display_nav_history_buttons: Some(
                TabBarSettings::get_global(cx).show_nav_history_buttons,
            ),
            _subscriptions: subscriptions,
            double_click_dispatch_action,
            save_modals_spawned: HashSet::default(),
            close_pane_if_empty: true,
            split_item_context_menu_handle: Default::default(),
            new_item_context_menu_handle: Default::default(),
            pinned_tab_count: 0,
            diagnostics: Default::default(),
            zoom_out_on_close: true,
            focus_follows_mouse: WorkspaceSettings::get_global(cx).focus_follows_mouse,
            diagnostic_summary_update: Task::ready(()),
            project_item_restoration_data: HashMap::default(),
            welcome_page: None,
            in_center_group: false,
            reserve_traffic_light_space: false,
            pane_kind: PaneKind::Tabs,
            visible: true,
            preferred_horizontal_split_size: None,
        }
    }

    pub(super) fn alternate_file(
        &mut self,
        _: &AlternateFile,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) {
        let (_, alternative) = &self.alternate_file_items;
        if let Some(alternative) = alternative {
            let existing = self
                .items()
                .find_position(|item| item.item_id() == alternative.id());
            if let Some((ix, _)) = existing {
                self.activate_item(ix, true, true, window, cx);
            } else if let Some(upgraded) = alternative.upgrade() {
                self.add_item(upgraded, true, true, None, window, cx);
            }
        }
    }

    pub fn track_alternate_file_items(&mut self) {
        if let Some(item) = self.active_item().map(|item| item.downgrade_item()) {
            let (current, _) = &self.alternate_file_items;
            match current {
                Some(current) => {
                    if current.id() != item.id() {
                        self.alternate_file_items =
                            (Some(item), self.alternate_file_items.0.take());
                    }
                }
                None => {
                    self.alternate_file_items = (Some(item), None);
                }
            }
        }
    }
}
