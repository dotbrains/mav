use super::*;

impl Sidebar {
    pub(super) fn render_project_header_ellipsis_menu(
        &self,
        ix: usize,
        id_prefix: &str,
        project_group_key: &ProjectGroupKey,
        is_active: bool,
        has_threads: bool,
        group_name: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let multi_workspace = self.multi_workspace.clone();
        let project_group_key = project_group_key.clone();

        let show_multi_project_entries = multi_workspace
            .read_with(cx, |mw, _| {
                project_group_key.host().is_none() && mw.project_group_keys().len() >= 2
            })
            .unwrap_or(false);

        let this = cx.weak_entity();

        let trigger_id = SharedString::from(format!("{id_prefix}-ellipsis-menu-{ix}"));
        let menu_handle = self
            .project_header_menu_handles
            .get(&ix)
            .cloned()
            .unwrap_or_default();
        let is_menu_open = menu_handle.is_deployed();

        PopoverMenu::new(format!("{id_prefix}project-header-menu-{ix}"))
            .with_handle(menu_handle)
            .trigger(
                IconButton::new(trigger_id, IconName::Ellipsis)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                    .icon_size(IconSize::Small)
                    .when(!is_menu_open, |el| el.visible_on_hover(group_name)),
            )
            .on_open(Rc::new({
                let this = this.clone();
                move |_window, cx| {
                    this.update(cx, |sidebar, cx| {
                        sidebar.project_header_menu_ix = Some(ix);
                        cx.notify();
                    })
                    .ok();
                }
            }))
            .menu(move |window, cx| {
                let multi_workspace = multi_workspace.clone();
                let project_group_key = project_group_key.clone();
                let this_for_menu = this.clone();

                let open_workspaces = multi_workspace
                    .read_with(cx, |multi_workspace, cx| {
                        multi_workspace
                            .workspaces_for_project_group(&project_group_key, cx)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                // Compute reorder state at menu-open time so it reflects the
                // most recent group ordering.
                let (group_index, total_groups) = multi_workspace
                    .read_with(cx, |mw, _| {
                        let keys = mw.project_group_keys();
                        let index = keys.iter().position(|k| k == &project_group_key);
                        (index, keys.len())
                    })
                    .unwrap_or((None, 0));
                let show_reorder_entries = total_groups >= 2;
                let can_move_up = group_index.is_some_and(|i| i > 0);
                let can_move_down = group_index.is_some_and(|i| i + 1 < total_groups);

                let active_workspace = multi_workspace
                    .read_with(cx, |multi_workspace, _cx| {
                        multi_workspace.workspace().clone()
                    })
                    .ok();
                let workspace_labels: Vec<_> = open_workspaces
                    .iter()
                    .map(|workspace| workspace_menu_worktree_labels(workspace, cx))
                    .collect();
                let workspace_is_active: Vec<_> = open_workspaces
                    .iter()
                    .map(|workspace| active_workspace.as_ref() == Some(workspace))
                    .collect();

                let menu =
                    ContextMenu::build_persistent(window, cx, move |menu, _window, menu_cx| {
                        let menu = menu.end_slot_action(Box::new(menu::SecondaryConfirm));
                        let weak_menu = menu_cx.weak_entity();

                        let menu = menu.when(show_multi_project_entries, |this| {
                            this.entry(
                                "Open Project in New Window",
                                Some(Box::new(workspace::MoveProjectToNewWindow)),
                                {
                                    let project_group_key = project_group_key.clone();
                                    let multi_workspace = multi_workspace.clone();
                                    move |window, cx| {
                                        multi_workspace
                                            .update(cx, |multi_workspace, cx| {
                                                multi_workspace
                                                    .open_project_group_in_new_window(
                                                        &project_group_key,
                                                        window,
                                                        cx,
                                                    )
                                                    .detach_and_log_err(cx);
                                            })
                                            .ok();
                                    }
                                },
                            )
                        });

                        let menu = menu
                            .custom_entry(
                                {
                                    move |_window, cx| {
                                        let action = h_flex()
                                            .opacity(0.6)
                                            .children(render_modifiers(
                                                &Modifiers::secondary_key(),
                                                PlatformStyle::platform(),
                                                None,
                                                Some(TextSize::Default.rems(cx).into()),
                                                false,
                                            ))
                                            .child(Label::new("-click").color(Color::Muted));

                                        let label = if has_threads {
                                            "Focus Last Project"
                                        } else {
                                            "Focus Project"
                                        };

                                        h_flex()
                                            .w_full()
                                            .justify_between()
                                            .gap_4()
                                            .child(
                                                Label::new(label)
                                                    .when(is_active, |s| s.color(Color::Disabled)),
                                            )
                                            .child(action)
                                            .into_any_element()
                                    }
                                },
                                {
                                    let project_group_key = project_group_key.clone();
                                    let this = this_for_menu.clone();
                                    move |window, cx| {
                                        if is_active {
                                            return;
                                        }
                                        this.update(cx, |sidebar, cx| {
                                            if let Some(workspace) =
                                                sidebar.workspace_for_group(&project_group_key, cx)
                                            {
                                                sidebar.activate_workspace(&workspace, window, cx);
                                            } else {
                                                sidebar.open_workspace_for_group(
                                                    &project_group_key,
                                                    window,
                                                    cx,
                                                );
                                            }
                                            sidebar.selection = None;
                                            sidebar.active_entry = None;
                                        })
                                        .ok();
                                    }
                                },
                            )
                            .selectable(!is_active);

                        let menu = if open_workspaces.is_empty() {
                            menu
                        } else {
                            let mut menu = menu.separator().header("Open Worktrees");

                            for (
                                workspace_index,
                                ((workspace, workspace_label), is_active_workspace),
                            ) in open_workspaces
                                .iter()
                                .cloned()
                                .zip(workspace_labels.iter().cloned())
                                .zip(workspace_is_active.iter().copied())
                                .enumerate()
                            {
                                let activate_multi_workspace = multi_workspace.clone();
                                let close_multi_workspace = multi_workspace.clone();
                                let activate_weak_menu = weak_menu.clone();
                                let close_weak_menu = weak_menu.clone();
                                let activate_workspace = workspace.clone();
                                let close_workspace = workspace.clone();

                                menu = menu.custom_entry(
                                    move |_window, _cx| {
                                        let close_multi_workspace = close_multi_workspace.clone();
                                        let close_weak_menu = close_weak_menu.clone();
                                        let close_workspace = close_workspace.clone();
                                        let row_group_name = SharedString::from(format!(
                                            "workspace-menu-row-{workspace_index}"
                                        ));

                                        h_flex()
                                            .group(&row_group_name)
                                            .w_full()
                                            .gap_2()
                                            .justify_between()
                                            .child(h_flex().min_w_0().gap_1().children(
                                                workspace_label.iter().enumerate().map(
                                                    |(label_ix, label)| {
                                                        h_flex()
                                                            .gap_1()
                                                            .when(label_ix > 0, |this| {
                                                                this.child(
                                                                    Label::new("•").alpha(0.25),
                                                                )
                                                            })
                                                            .child(label.render())
                                                            .into_any_element()
                                                    },
                                                ),
                                            ))
                                            .when(is_active_workspace, |this| {
                                                this.pr_1().child(
                                                    Icon::new(IconName::Check)
                                                        .size(IconSize::Small)
                                                        .color(Color::Accent),
                                                )
                                            })
                                            .when(!is_active_workspace, |this| {
                                                let close_multi_workspace =
                                                    close_multi_workspace.clone();
                                                let close_weak_menu = close_weak_menu.clone();
                                                let close_workspace = close_workspace.clone();

                                                this.child(
                                                    IconButton::new(
                                                        ("close-workspace", workspace_index),
                                                        IconName::Close,
                                                    )
                                                    .icon_size(IconSize::Small)
                                                    .visible_on_hover(&row_group_name)
                                                    .tooltip(Tooltip::text("Close Worktree"))
                                                    .on_click(move |_, window, cx| {
                                                        cx.stop_propagation();
                                                        window.prevent_default();
                                                        close_multi_workspace
                                                            .update(cx, |multi_workspace, cx| {
                                                                multi_workspace
                                                                    .close_workspace(
                                                                        &close_workspace,
                                                                        window,
                                                                        cx,
                                                                    )
                                                                    .detach_and_log_err(cx);
                                                            })
                                                            .ok();
                                                        close_weak_menu
                                                            .update(cx, |_, cx| {
                                                                cx.emit(DismissEvent)
                                                            })
                                                            .ok();
                                                    }),
                                                )
                                            })
                                            .into_any_element()
                                    },
                                    move |window, cx| {
                                        activate_multi_workspace
                                            .update(cx, |multi_workspace, cx| {
                                                multi_workspace.activate(
                                                    activate_workspace.clone(),
                                                    None,
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                        activate_weak_menu
                                            .update(cx, |_, cx| cx.emit(DismissEvent))
                                            .ok();
                                    },
                                );
                            }

                            menu
                        };

                        let menu = menu.when(show_reorder_entries, |this| {
                            let move_up_multi_workspace = multi_workspace.clone();
                            let move_up_key = project_group_key.clone();
                            let move_up_weak_menu = weak_menu.clone();
                            let move_down_multi_workspace = multi_workspace.clone();
                            let move_down_key = project_group_key.clone();
                            let move_down_weak_menu = weak_menu.clone();

                            this.separator()
                                .item(
                                    ContextMenuEntry::new("Move Up")
                                        .disabled(!can_move_up)
                                        .handler(move |_window, cx| {
                                            move_up_multi_workspace
                                                .update(cx, |mw, cx| {
                                                    mw.move_project_group_up(&move_up_key, cx);
                                                })
                                                .ok();
                                            move_up_weak_menu
                                                .update(cx, |_, cx| cx.emit(DismissEvent))
                                                .ok();
                                        }),
                                )
                                .item(
                                    ContextMenuEntry::new("Move Down")
                                        .disabled(!can_move_down)
                                        .handler(move |_window, cx| {
                                            move_down_multi_workspace
                                                .update(cx, |mw, cx| {
                                                    mw.move_project_group_down(&move_down_key, cx);
                                                })
                                                .ok();
                                            move_down_weak_menu
                                                .update(cx, |_, cx| cx.emit(DismissEvent))
                                                .ok();
                                        }),
                                )
                        });

                        let project_group_key = project_group_key.clone();
                        let remove_multi_workspace = multi_workspace.clone();
                        menu.separator().entry("Remove", None, move |window, cx| {
                            remove_multi_workspace
                                .update(cx, |multi_workspace, cx| {
                                    multi_workspace
                                        .remove_project_group(&project_group_key, window, cx)
                                        .detach_and_log_err(cx);
                                })
                                .ok();
                            weak_menu.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
                        })
                    });

                let this = this.clone();

                window
                    .subscribe(&menu, cx, move |_, _: &gpui::DismissEvent, _window, cx| {
                        this.update(cx, |sidebar, cx| {
                            sidebar.project_header_menu_ix = None;
                            cx.notify();
                        })
                        .ok();
                    })
                    .detach();

                Some(menu)
            })
            .anchor(gpui::Anchor::TopRight)
            .offset(gpui::Point {
                x: px(0.),
                y: px(1.),
            })
            .into_any_element()
    }
}
