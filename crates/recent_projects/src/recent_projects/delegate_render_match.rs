use super::*;

impl RecentProjectsDelegate {
    pub(super) fn render_delegate_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        match self.filtered_entries.get(ix)? {
            ProjectPickerEntry::Header(title) => Some(
                v_flex()
                    .w_full()
                    .gap_1()
                    .when(ix > 0, |this| this.mt_1().child(Divider::horizontal()))
                    .child(ListSubHeader::new(title.clone()).inset(true))
                    .into_any_element(),
            ),
            ProjectPickerEntry::OpenFolder { index, positions } => {
                let folder = self.open_folders.get(*index)?;
                let name = folder.name.clone();
                let path = folder.path.compact();
                let branch = folder.branch.clone();
                let is_active = folder.is_active;
                let worktree_id = folder.worktree_id;
                let positions = positions.clone();
                let show_path = self.style == ProjectPickerStyle::Modal;

                let secondary_actions = h_flex()
                    .gap_1()
                    .child(
                        IconButton::new(("remove-folder", worktree_id.to_usize()), IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip({
                                let focus_handle = self.focus_handle.clone();
                                move |_, cx| {
                                    Tooltip::for_action_in(
                                        "Remove Folder from Project",
                                        &RemoveSelected,
                                        &focus_handle,
                                        cx,
                                    )
                                }
                            })
                            .on_click(cx.listener(move |picker, _, window, cx| {
                                let Some(workspace) = picker.delegate.workspace.upgrade() else {
                                    return;
                                };
                                workspace.update(cx, |workspace, cx| {
                                    let project = workspace.project().clone();
                                    project.update(cx, |project, cx| {
                                        project.remove_worktree(worktree_id, cx);
                                    });
                                });
                                picker.delegate.open_folders =
                                    get_open_folders(workspace.read(cx), cx);
                                let query = picker.query(cx);
                                picker.update_matches(query, window, cx);
                            })),
                    )
                    .into_any_element();

                let icon = icon_for_remote_connection(folder.connection_options.as_ref());
                let show_icon = self.filtered_entries_include_remote_project();

                let tooltip_path: SharedString = path.to_string_lossy().to_string().into();
                let tooltip_branch = branch.clone();

                Some(
                    ListItem::new(ix)
                        .toggle_state(selected)
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .child(
                            h_flex()
                                .id("open_folder_item")
                                .w_full()
                                .min_w_0()
                                .gap_2p5()
                                .when(show_icon, |this| {
                                    this.child(Icon::new(icon).color(Color::Muted))
                                })
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .child(
                                            h_flex()
                                                .gap_1()
                                                .child(HighlightedLabel::new(
                                                    name.to_string(),
                                                    positions,
                                                ))
                                                .when_some(branch, |this, branch| {
                                                    this.child(
                                                        Label::new(branch)
                                                            .color(Color::Muted)
                                                            .truncate(),
                                                    )
                                                })
                                                .when(is_active, |this| {
                                                    this.child(
                                                        Icon::new(IconName::Check)
                                                            .size(IconSize::Small)
                                                            .color(Color::Accent),
                                                    )
                                                }),
                                        )
                                        .when(show_path, |this| {
                                            this.child(
                                                Label::new(path.to_string_lossy().to_string())
                                                    .size(LabelSize::Small)
                                                    .color(Color::Muted),
                                            )
                                        }),
                                )
                                .when(!show_path, |this| {
                                    this.tooltip(move |_, cx| {
                                        if let Some(branch) = tooltip_branch.clone() {
                                            Tooltip::with_meta(
                                                format!("{}/{}", name, branch),
                                                None,
                                                tooltip_path.clone(),
                                                cx,
                                            )
                                        } else {
                                            Tooltip::simple(tooltip_path.clone(), cx)
                                        }
                                    })
                                }),
                        )
                        .end_slot(secondary_actions)
                        .show_end_slot_on_hover()
                        .into_any_element(),
                )
            }
            ProjectPickerEntry::ProjectGroup(hit) => {
                let key = self.window_project_groups.get(hit.candidate_id)?;
                let is_active = self.is_active_project_group(key, cx);
                let paths = key.path_list();
                let ordered_paths: Vec<_> = paths
                    .ordered_paths()
                    .map(|p| p.compact().to_string_lossy().to_string())
                    .collect();
                let tooltip_path: SharedString = ordered_paths.join("\n").into();
                let icon = icon_for_project_group(key);
                let show_icon = self.filtered_entries_include_remote_project();

                let mut path_start_offset = 0;
                let (match_labels, path_highlights): (Vec<_>, Vec<_>) = paths
                    .ordered_paths()
                    .map(|p| p.compact())
                    .map(|path| {
                        let highlighted_text =
                            highlights_for_path(path.as_ref(), &hit.positions, path_start_offset);
                        path_start_offset += highlighted_text.1.text.len();
                        highlighted_text
                    })
                    .unzip();

                let highlighted_match = HighlightedMatchWithPaths {
                    prefix: None,
                    match_label: HighlightedMatch::join(match_labels.into_iter().flatten(), ", "),
                    paths: path_highlights,
                    active: is_active,
                };

                let project_group_key = key.clone();
                let is_local = key.host().is_none();
                let has_multiple_groups = self.window_project_groups.len() >= 2;
                let secondary_actions = h_flex()
                    .gap_0p5()
                    .when(is_local && has_multiple_groups, |this| {
                        this.child(
                            IconButton::new("move_to_new_window", IconName::ArrowUpRight)
                                .icon_size(IconSize::Small)
                                .tooltip({
                                    let focus_handle = self.focus_handle.clone();
                                    move |_, cx| {
                                        Tooltip::for_action_in(
                                            "Open in New Window",
                                            &menu::SecondaryConfirm,
                                            &focus_handle,
                                            cx,
                                        )
                                    }
                                })
                                .on_click({
                                    let project_group_key = project_group_key.clone();
                                    cx.listener(move |_picker, _, window, cx| {
                                        cx.stop_propagation();
                                        window.prevent_default();
                                        move_project_group_to_new_window(
                                            &project_group_key,
                                            window,
                                            cx,
                                        );
                                        cx.emit(DismissEvent);
                                    })
                                }),
                        )
                    })
                    .when(!is_active, |this| {
                        this.child(
                            IconButton::new("remove_open_project", IconName::Close)
                                .icon_size(IconSize::Small)
                                .tooltip({
                                    let focus_handle = self.focus_handle.clone();
                                    move |_, cx| {
                                        Tooltip::for_action_in(
                                            "Remove Project from Window",
                                            &RemoveSelected,
                                            &focus_handle,
                                            cx,
                                        )
                                    }
                                })
                                .on_click({
                                    let project_group_key = project_group_key.clone();
                                    cx.listener(move |picker, _, window, cx| {
                                        cx.stop_propagation();
                                        window.prevent_default();
                                        picker.delegate.remove_project_group(
                                            project_group_key.clone(),
                                            window,
                                            cx,
                                        );
                                        let query = picker.query(cx);
                                        picker.update_matches(query, window, cx);
                                    })
                                }),
                        )
                    })
                    .into_any_element();

                Some(
                    ListItem::new(ix)
                        .inset(true)
                        .toggle_state(selected)
                        .spacing(ListItemSpacing::Sparse)
                        .child(
                            h_flex()
                                .id("open_project_info_container")
                                .w_full()
                                .min_w_0()
                                .gap_2p5()
                                .when(show_icon, |this| {
                                    this.child(Icon::new(icon).color(Color::Muted))
                                })
                                .child({
                                    let mut highlighted = highlighted_match;
                                    if !self.render_paths {
                                        highlighted.paths.clear();
                                    }
                                    highlighted.render(window, cx)
                                })
                                .tooltip(Tooltip::text(tooltip_path)),
                        )
                        .end_slot(secondary_actions)
                        .show_end_slot_on_hover()
                        .into_any_element(),
                )
            }
            ProjectPickerEntry::RecentProject(hit) => {
                let workspace = self.workspaces.get(hit.candidate_id)?;
                let location = &workspace.location;
                let raw_paths = &workspace.paths;
                let identity_paths = &workspace.identity_paths;
                let is_local = matches!(location, SerializedWorkspaceLocation::Local);
                let paths_to_add = raw_paths.paths().to_vec();
                let ordered_paths: Vec<_> = identity_paths
                    .ordered_paths()
                    .map(|p| p.compact().to_string_lossy().to_string())
                    .collect();
                let tooltip_path: SharedString = match &location {
                    SerializedWorkspaceLocation::Remote(options) => {
                        let host = options.display_name();
                        if ordered_paths.len() == 1 {
                            format!("{} ({})", ordered_paths[0], host).into()
                        } else {
                            format!("{}\n({})", ordered_paths.join("\n"), host).into()
                        }
                    }
                    _ => ordered_paths.join("\n").into(),
                };

                let mut path_start_offset = 0;
                let (match_labels, paths): (Vec<_>, Vec<_>) = identity_paths
                    .ordered_paths()
                    .map(|p| p.compact())
                    .map(|path| {
                        let highlighted_text =
                            highlights_for_path(path.as_ref(), &hit.positions, path_start_offset);
                        path_start_offset += highlighted_text.1.text.len();
                        highlighted_text
                    })
                    .unzip();

                let tooltip_title = if paths.len() > 1 {
                    "Add Folders to this Project"
                } else {
                    "Add Folder to this Project"
                };

                let prefix = match &location {
                    SerializedWorkspaceLocation::Remote(options) => {
                        Some(SharedString::from(options.display_name()))
                    }
                    _ => None,
                };

                let highlighted_match = HighlightedMatchWithPaths {
                    prefix,
                    match_label: HighlightedMatch::join(match_labels.into_iter().flatten(), ", "),
                    paths,
                    active: false,
                };

                let focus_handle = self.focus_handle.clone();
                let secondary_confirm_tooltip = if self.create_new_window {
                    "Open Project in This Window"
                } else {
                    "Open Project in New Window"
                };
                let primary_confirm_tooltip = if self.create_new_window {
                    "Open Project in New Window"
                } else {
                    "Open Project in This Window"
                };
                let secondary_confirm_icon = if self.create_new_window {
                    IconName::ThisWindow
                } else {
                    IconName::ArrowUpRight
                };

                let secondary_actions = h_flex()
                    .gap_px()
                    .when(is_local, |this| {
                        this.child(
                            IconButton::new("add_to_workspace", IconName::FolderInclude)
                                .icon_size(IconSize::Small)
                                .tooltip({
                                    let focus_handle = self.focus_handle.clone();
                                    move |_, cx| {
                                        Tooltip::with_meta_in(
                                            tooltip_title,
                                            Some(&AddToWorkspace),
                                            "As a multi-root folder",
                                            &focus_handle,
                                            cx,
                                        )
                                    }
                                })
                                .on_click({
                                    let paths_to_add = paths_to_add.clone();
                                    cx.listener(move |picker, _event, window, cx| {
                                        cx.stop_propagation();
                                        window.prevent_default();
                                        picker.delegate.add_paths_to_project(
                                            paths_to_add.clone(),
                                            window,
                                            cx,
                                        );
                                    })
                                }),
                        )
                    })
                    .child(
                        IconButton::new("alternate_open", secondary_confirm_icon)
                            .icon_size(IconSize::Small)
                            .tooltip({
                                move |_, cx| {
                                    Tooltip::for_action_in(
                                        secondary_confirm_tooltip,
                                        &menu::SecondaryConfirm,
                                        &focus_handle,
                                        cx,
                                    )
                                }
                            })
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.delegate.set_selected_index(ix, window, cx);
                                this.delegate.confirm(true, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("delete", IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip({
                                let focus_handle = self.focus_handle.clone();
                                move |_, cx| {
                                    Tooltip::for_action_in(
                                        "Remove from Recent Projects",
                                        &RemoveSelected,
                                        &focus_handle,
                                        cx,
                                    )
                                }
                            })
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.delegate.delete_recent_project(ix, window, cx)
                            })),
                    )
                    .into_any_element();

                let icon = icon_for_remote_connection(match location {
                    SerializedWorkspaceLocation::Local => None,
                    SerializedWorkspaceLocation::Remote(options) => Some(options),
                });
                let show_icon = self.filtered_entries_include_remote_project();

                Some(
                    ListItem::new(ix)
                        .toggle_state(selected)
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .child(
                            h_flex()
                                .id("project_info_container")
                                .w_full()
                                .min_w_0()
                                .gap_2p5()
                                .flex_grow_1()
                                .when(show_icon, |this| {
                                    this.child(Icon::new(icon).color(Color::Muted))
                                })
                                .child({
                                    let mut highlighted = highlighted_match;
                                    if !self.render_paths {
                                        highlighted.paths.clear();
                                    }
                                    highlighted.render(window, cx)
                                })
                                .tooltip(move |_, cx| {
                                    Tooltip::with_meta(
                                        primary_confirm_tooltip,
                                        None,
                                        tooltip_path.clone(),
                                        cx,
                                    )
                                }),
                        )
                        .end_slot(secondary_actions)
                        .show_end_slot_on_hover()
                        .into_any_element(),
                )
            }
        }
    }
}
