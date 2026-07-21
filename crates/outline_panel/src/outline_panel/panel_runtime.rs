use super::*;

pub(super) fn workspace_active_editor(
    workspace: &Workspace,
    cx: &App,
) -> Option<(Box<dyn ItemHandle>, Entity<Editor>)> {
    let active_item = workspace.active_item(cx)?;
    let active_editor = active_item
        .act_as::<Editor>(cx)
        .filter(|editor| editor.read(cx).mode().is_full())?;
    Some((active_item, active_editor))
}

impl Panel for OutlinePanel {
    fn persistent_name() -> &'static str {
        "Outline Panel"
    }

    fn panel_key() -> &'static str {
        OUTLINE_PANEL_KEY
    }

    fn position(&self, _: &Window, cx: &App) -> DockPosition {
        match OutlinePanelSettings::get_global(cx).dock {
            DockSide::Left => DockPosition::Left,
            DockSide::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            let dock = match position {
                DockPosition::Left | DockPosition::Bottom => DockSide::Left,
                DockPosition::Right => DockSide::Right,
            };
            settings.outline_panel.get_or_insert_default().dock = Some(dock);
        });
    }

    fn default_size(&self, _: &Window, cx: &App) -> Pixels {
        OutlinePanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::ToC)
    }

    fn button_visible(&self, cx: &App) -> bool {
        OutlinePanelSettings::get_global(cx).button
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Outline Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _window: &Window, _: &App) -> bool {
        self.active
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |outline_panel, cx| {
            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    let old_active = outline_panel.active;
                    outline_panel.active = active;
                    if old_active != active {
                        if active
                            && let Some((active_item, active_editor)) =
                                outline_panel.workspace.upgrade().and_then(|workspace| {
                                    workspace_active_editor(workspace.read(cx), cx)
                                })
                        {
                            if outline_panel.should_replace_active_item(active_item.as_ref()) {
                                outline_panel.replace_active_editor(
                                    active_item,
                                    active_editor,
                                    window,
                                    cx,
                                );
                            } else {
                                outline_panel.update_fs_entries(active_editor, None, window, cx)
                            }
                            return;
                        }

                        if !outline_panel.pinned {
                            outline_panel.clear_previous(window, cx);
                        }
                    }
                    outline_panel.serialize(cx);
                })
                .ok();
        })
        .detach()
    }

    fn activation_priority(&self) -> u32 {
        6
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.outline_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

impl Focusable for OutlinePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter_editor.focus_handle(cx)
    }
}

impl EventEmitter<Event> for OutlinePanel {}

impl EventEmitter<PanelEvent> for OutlinePanel {}

impl Render for OutlinePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_local, is_via_ssh) = self.project.read_with(cx, |project, _| {
            (project.is_local(), project.is_via_remote_server())
        });
        let query = self.query(cx);
        let pinned = self.pinned;
        let settings = OutlinePanelSettings::get_global(cx);
        let indent_size = settings.indent_size;
        let show_indent_guides = settings.indent_guides.show == ShowIndentGuides::Always;

        let search_query = match &self.mode {
            ItemsDisplayMode::Search(search_query) => Some(search_query),
            _ => None,
        };

        let search_query_text = search_query.map(|sq| sq.query.to_string());

        v_flex()
            .id("outline-panel")
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .overflow_hidden()
            .relative()
            .key_context(self.dispatch_context(window, cx))
            .on_action(cx.listener(Self::open_selected_entry))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::scroll_cursor_center))
            .on_action(cx.listener(Self::scroll_cursor_top))
            .on_action(cx.listener(Self::scroll_cursor_bottom))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::select_parent))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::expand_all_entries))
            .on_action(cx.listener(Self::collapse_all_entries))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::copy_relative_path))
            .on_action(cx.listener(Self::toggle_active_editor_pin))
            .on_action(cx.listener(Self::unfold_directory))
            .on_action(cx.listener(Self::fold_directory))
            .on_action(cx.listener(Self::open_excerpts))
            .on_action(cx.listener(Self::open_excerpts_split))
            .when(is_local, |el| {
                el.on_action(cx.listener(Self::reveal_in_finder))
            })
            .when(is_local || is_via_ssh, |el| {
                el.on_action(cx.listener(Self::open_in_terminal))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |outline_panel, event: &MouseDownEvent, window, cx| {
                    if let Some(entry) = outline_panel.selected_entry().cloned() {
                        outline_panel.deploy_context_menu(event.position, entry, window, cx)
                    } else if let Some(entry) = outline_panel.fs_entries.first().cloned() {
                        outline_panel.deploy_context_menu(
                            event.position,
                            PanelEntry::Fs(entry),
                            window,
                            cx,
                        )
                    }
                }),
            )
            .track_focus(&self.focus_handle)
            .child(self.render_filter_footer(pinned, cx))
            .when_some(search_query_text, |outline_panel, query_text| {
                outline_panel.child(
                    h_flex()
                        .py_1p5()
                        .px_2()
                        .h(Tab::container_height(cx))
                        .gap_0p5()
                        .border_b_1()
                        .border_color(cx.theme().colors().border_variant)
                        .child(Label::new("Searching:").color(Color::Muted))
                        .child(Label::new(query_text)),
                )
            })
            .child(self.render_main_contents(query, show_indent_guides, indent_size, window, cx))
    }
}
