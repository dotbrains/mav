use super::*;

impl RecentProjectsDelegate {
    pub(super) fn render_delegate_footer(
        &self,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        let focus_handle = self.focus_handle.clone();
        let popover_style = matches!(self.style, ProjectPickerStyle::Popover);

        let is_already_open_entry = matches!(
            self.filtered_entries.get(self.selected_index),
            Some(ProjectPickerEntry::OpenFolder { .. } | ProjectPickerEntry::ProjectGroup(_))
        );

        let show_move_to_new_window = match self.filtered_entries.get(self.selected_index) {
            Some(ProjectPickerEntry::ProjectGroup(hit)) => {
                self.window_project_groups.len() >= 2
                    && self
                        .window_project_groups
                        .get(hit.candidate_id)
                        .is_some_and(|key| key.host().is_none())
            }
            _ => false,
        };

        if popover_style {
            return Some(
                v_flex()
                    .flex_1()
                    .p_1p5()
                    .gap_1()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child({
                        ButtonLike::new("open_local_folder")
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_1()
                                    .justify_between()
                                    .child(Label::new("Open Local Folders"))
                                    .child(KeyBinding::for_action_in(
                                        &workspace::Open {
                                            create_new_window: Some(self.create_new_window),
                                        },
                                        &focus_handle,
                                        cx,
                                    )),
                            )
                            .on_click({
                                let workspace = self.workspace.clone();
                                let create_new_window = self.create_new_window;
                                move |_, window, cx| {
                                    open_local_project(
                                        workspace.clone(),
                                        create_new_window,
                                        window,
                                        cx,
                                    );
                                }
                            })
                    })
                    .child(
                        ButtonLike::new("open_remote_folder")
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_1()
                                    .justify_between()
                                    .child(Label::new("Open Remote Folder"))
                                    .child(KeyBinding::for_action(
                                        &OpenRemote {
                                            from_existing_connection: false,
                                            create_new_window: Some(self.create_new_window),
                                        },
                                        cx,
                                    )),
                            )
                            .on_click({
                                let create_new_window = self.create_new_window;
                                move |_, window, cx| {
                                    window.dispatch_action(
                                        OpenRemote {
                                            from_existing_connection: false,
                                            create_new_window: Some(create_new_window),
                                        }
                                        .boxed_clone(),
                                        cx,
                                    )
                                }
                            }),
                    )
                    .into_any(),
            );
        }

        let selected_entry = self.filtered_entries.get(self.selected_index);

        let is_current_workspace_entry =
            if let Some(ProjectPickerEntry::ProjectGroup(hit)) = selected_entry {
                self.window_project_groups
                    .get(hit.candidate_id)
                    .is_some_and(|key| self.is_active_project_group(key, cx))
            } else {
                false
            };

        let secondary_footer_actions: Option<AnyElement> = match selected_entry {
            Some(ProjectPickerEntry::OpenFolder { .. }) => Some(
                Button::new("remove_selected", "Remove Folder")
                    .key_binding(KeyBinding::for_action_in(
                        &RemoveSelected,
                        &focus_handle,
                        cx,
                    ))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(RemoveSelected.boxed_clone(), cx)
                    })
                    .into_any_element(),
            ),
            Some(ProjectPickerEntry::ProjectGroup(_)) if !is_current_workspace_entry => Some(
                Button::new("remove_selected", "Remove from Window")
                    .key_binding(KeyBinding::for_action_in(
                        &RemoveSelected,
                        &focus_handle,
                        cx,
                    ))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(RemoveSelected.boxed_clone(), cx)
                    })
                    .into_any_element(),
            ),
            Some(ProjectPickerEntry::RecentProject(_)) => Some(
                Button::new("delete_recent", "Remove")
                    .key_binding(KeyBinding::for_action_in(
                        &RemoveSelected,
                        &focus_handle,
                        cx,
                    ))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(RemoveSelected.boxed_clone(), cx)
                    })
                    .into_any_element(),
            ),
            _ => None,
        };

        Some(
            h_flex()
                .flex_1()
                .p_1p5()
                .gap_1()
                .justify_end()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .when_some(secondary_footer_actions, |this, actions| {
                    this.child(actions)
                })
                .map(|this| {
                    if is_already_open_entry {
                        this.when(show_move_to_new_window, |this| {
                            this.child({
                                let window_project_groups = self.window_project_groups.clone();
                                let selected_index = self.selected_index;
                                let filtered_entries = self.filtered_entries.clone();
                                Button::new("move_to_new_window", "New Window")
                                    .key_binding(KeyBinding::for_action_in(
                                        &menu::SecondaryConfirm,
                                        &focus_handle,
                                        cx,
                                    ))
                                    .on_click(move |_, window, cx| {
                                        let key = match filtered_entries.get(selected_index) {
                                            Some(ProjectPickerEntry::ProjectGroup(hit)) => {
                                                window_project_groups.get(hit.candidate_id).cloned()
                                            }
                                            _ => None,
                                        };
                                        if let Some(key) = key {
                                            move_project_group_to_new_window(&key, window, cx);
                                        }
                                    })
                            })
                        })
                        .child(
                            Button::new("activate", "Activate")
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::Confirm,
                                    &focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                                }),
                        )
                    } else if self.create_new_window {
                        this.child(
                            Button::new("open_here", "This Window")
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::SecondaryConfirm,
                                    &focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                                }),
                        )
                        .child(
                            Button::new("open_new_window", "Open")
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::Confirm,
                                    &focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                                }),
                        )
                    } else {
                        this.child(
                            Button::new("open_new_window", "New Window")
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::SecondaryConfirm,
                                    &focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                                }),
                        )
                        .child(
                            Button::new("open_here", "Open")
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::Confirm,
                                    &focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                                }),
                        )
                    }
                })
                .child(Divider::vertical())
                .child(
                    PopoverMenu::new("actions-menu-popover")
                        .with_handle(self.actions_menu_handle.clone())
                        .anchor(gpui::Anchor::BottomRight)
                        .offset(gpui::Point {
                            x: px(0.0),
                            y: px(-2.0),
                        })
                        .trigger(
                            Button::new("actions-trigger", "Actions")
                                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                .key_binding(KeyBinding::for_action_in(
                                    &ToggleActionsMenu,
                                    &focus_handle,
                                    cx,
                                )),
                        )
                        .menu({
                            let focus_handle = focus_handle.clone();
                            let workspace_handle = self.workspace.clone();
                            let create_new_window = self.create_new_window;
                            let open_action = workspace::Open {
                                create_new_window: Some(create_new_window),
                            };
                            let show_add_to_workspace = match selected_entry {
                                Some(ProjectPickerEntry::RecentProject(hit)) => self
                                    .workspaces
                                    .get(hit.candidate_id)
                                    .map(|workspace| {
                                        matches!(
                                            workspace.location,
                                            SerializedWorkspaceLocation::Local
                                        )
                                    })
                                    .unwrap_or(false),
                                _ => false,
                            };

                            move |window, cx| {
                                Some(ContextMenu::build(window, cx, {
                                    let focus_handle = focus_handle.clone();
                                    let workspace_handle = workspace_handle.clone();
                                    let open_action = open_action.clone();
                                    move |menu, _, _| {
                                        menu.context(focus_handle)
                                            .when(show_add_to_workspace, |menu| {
                                                menu.action(
                                                    "Add Folder to this Project",
                                                    AddToWorkspace.boxed_clone(),
                                                )
                                                .separator()
                                            })
                                            .entry(
                                                "Open Local Folders",
                                                Some(open_action.boxed_clone()),
                                                {
                                                    let workspace_handle = workspace_handle.clone();
                                                    move |window, cx| {
                                                        open_local_project(
                                                            workspace_handle.clone(),
                                                            create_new_window,
                                                            window,
                                                            cx,
                                                        );
                                                    }
                                                },
                                            )
                                            .action(
                                                "Open Remote Folder",
                                                OpenRemote {
                                                    from_existing_connection: false,
                                                    create_new_window: Some(create_new_window),
                                                }
                                                .boxed_clone(),
                                            )
                                    }
                                }))
                            }
                        }),
                )
                .into_any(),
        )
    }
}
