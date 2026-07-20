use super::*;

impl ThreadView {
    pub(super) fn render_permission_granularity_dropdown(
        &self,
        choices: &[PermissionOptionChoice],
        current_label: SharedString,
        entry_ix: usize,
        tool_call_id: acp::ToolCallId,
        selected_index: usize,
        is_first: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let menu_options: Vec<(usize, SharedString)> = choices
            .iter()
            .enumerate()
            .map(|(i, choice)| (i, choice.label()))
            .collect();

        let permission_dropdown_handle = self.permission_dropdown_handle.clone();

        PopoverMenu::new(("permission-granularity", entry_ix))
            .with_handle(permission_dropdown_handle)
            .trigger(
                Button::new(("granularity-trigger", entry_ix), current_label)
                    .end_icon(
                        Icon::new(IconName::ChevronDown)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .label_size(LabelSize::Small)
                    .when(is_first, |this| {
                        this.key_binding(
                            KeyBinding::for_action_in(
                                &crate::OpenPermissionDropdown as &dyn Action,
                                &self.focus_handle(cx),
                                cx,
                            )
                            .map(|kb| kb.size(rems_from_px(12.))),
                        )
                    }),
            )
            .menu(move |window, cx| {
                let tool_call_id = tool_call_id.clone();
                let options = menu_options.clone();

                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    for (index, display_name) in options.iter() {
                        let display_name = display_name.clone();
                        let index = *index;
                        let tool_call_id_for_entry = tool_call_id.clone();
                        let is_selected = index == selected_index;
                        menu = menu.toggleable_entry(
                            display_name,
                            is_selected,
                            IconPosition::End,
                            None,
                            move |window, cx| {
                                window.dispatch_action(
                                    SelectPermissionGranularity {
                                        tool_call_id: tool_call_id_for_entry.0.to_string(),
                                        index,
                                    }
                                    .boxed_clone(),
                                    cx,
                                );
                            },
                        );
                    }

                    menu
                }))
            })
            .into_any_element()
    }

    pub(super) fn render_permission_granularity_dropdown_with_patterns(
        &self,
        choices: &[PermissionOptionChoice],
        patterns: &[PermissionPattern],
        _tool_name: &str,
        current_label: SharedString,
        entry_ix: usize,
        tool_call_id: acp::ToolCallId,
        is_first: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let default_choice_index = choices.len().saturating_sub(1);
        let menu_options: Vec<(usize, SharedString)> = choices
            .iter()
            .enumerate()
            .map(|(i, choice)| (i, choice.label()))
            .collect();

        let pattern_options: Vec<(usize, SharedString)> = patterns
            .iter()
            .enumerate()
            .map(|(i, cp)| {
                (
                    i,
                    SharedString::from(format!("Always for `{}` commands", cp.display_name)),
                )
            })
            .collect();

        let pattern_count = patterns.len();
        let permission_dropdown_handle = self.permission_dropdown_handle.clone();
        let view = cx.entity().downgrade();

        PopoverMenu::new(("permission-granularity", entry_ix))
            .with_handle(permission_dropdown_handle.clone())
            .anchor(gpui::Anchor::TopRight)
            .attach(gpui::Anchor::BottomRight)
            .trigger(
                Button::new(("granularity-trigger", entry_ix), current_label)
                    .end_icon(
                        Icon::new(IconName::ChevronDown)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .label_size(LabelSize::Small)
                    .when(is_first, |this| {
                        this.key_binding(
                            KeyBinding::for_action_in(
                                &crate::OpenPermissionDropdown as &dyn Action,
                                &self.focus_handle(cx),
                                cx,
                            )
                            .map(|kb| kb.size(rems_from_px(12.))),
                        )
                    }),
            )
            .menu(move |window, cx| {
                let tool_call_id = tool_call_id.clone();
                let options = menu_options.clone();
                let patterns = pattern_options.clone();
                let view = view.clone();
                let dropdown_handle = permission_dropdown_handle.clone();

                Some(ContextMenu::build_persistent(
                    window,
                    cx,
                    move |menu, _window, cx| {
                        let mut menu = menu;

                        let selection: Option<PermissionSelection> = view.upgrade().and_then(|v| {
                            let view = v.read(cx);
                            view.permission_selections.get(&tool_call_id).cloned()
                        });

                        let is_pattern_mode =
                            matches!(selection, Some(PermissionSelection::SelectedPatterns(_)));

                        for (index, display_name) in options.iter() {
                            let display_name = display_name.clone();
                            let index = *index;
                            let tool_call_id_for_entry = tool_call_id.clone();
                            let is_selected = !is_pattern_mode
                                && selection
                                    .as_ref()
                                    .and_then(|s| s.choice_index())
                                    .map_or(index == default_choice_index, |ci| ci == index);

                            let view = view.clone();
                            menu = menu.toggleable_entry(
                                display_name,
                                is_selected,
                                IconPosition::End,
                                None,
                                move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.permission_selections.insert(
                                            tool_call_id_for_entry.clone(),
                                            PermissionSelection::Choice(index),
                                        );
                                        cx.notify();
                                    })
                                    .log_err();
                                },
                            );
                        }

                        menu = menu.separator().header("Select Options…");

                        for (pattern_index, label) in patterns.iter() {
                            let label = label.clone();
                            let pattern_index = *pattern_index;
                            let tool_call_id_for_pattern = tool_call_id.clone();
                            let is_checked = selection
                                .as_ref()
                                .is_some_and(|s| s.is_pattern_checked(pattern_index));

                            let view = view.clone();
                            menu = menu.toggleable_entry(
                                label,
                                is_checked,
                                IconPosition::End,
                                None,
                                move |_window, cx| {
                                    view.update(cx, |this, cx| {
                                        let selection = this
                                            .permission_selections
                                            .get_mut(&tool_call_id_for_pattern);

                                        match selection {
                                            Some(PermissionSelection::SelectedPatterns(_)) => {
                                                this.permission_selections
                                                    .get_mut(&tool_call_id_for_pattern)
                                                    .expect("just matched above")
                                                    .toggle_pattern(pattern_index);
                                            }
                                            _ => {
                                                this.permission_selections.insert(
                                                    tool_call_id_for_pattern.clone(),
                                                    PermissionSelection::SelectedPatterns(
                                                        (0..pattern_count).collect(),
                                                    ),
                                                );
                                            }
                                        }
                                        cx.notify();
                                    })
                                    .log_err();
                                },
                            );
                        }

                        let any_patterns_checked = selection
                            .as_ref()
                            .is_some_and(|s| s.has_any_checked_patterns());
                        let dropdown_handle = dropdown_handle.clone();
                        menu = menu.custom_row(move |_window, _cx| {
                            div()
                                .py_1()
                                .w_full()
                                .child(
                                    Button::new("apply-patterns", "Apply")
                                        .full_width()
                                        .style(ButtonStyle::Outlined)
                                        .label_size(LabelSize::Small)
                                        .disabled(!any_patterns_checked)
                                        .on_click({
                                            let dropdown_handle = dropdown_handle.clone();
                                            move |_event, _window, cx| {
                                                dropdown_handle.hide(cx);
                                            }
                                        }),
                                )
                                .into_any_element()
                        });

                        menu
                    },
                ))
            })
            .into_any_element()
    }
}
