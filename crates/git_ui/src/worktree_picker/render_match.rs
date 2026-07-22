use super::*;

impl WorktreePickerDelegate {
    fn render_match_impl(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let entry = self.matches.get(ix)?;

        match entry {
            WorktreeEntry::Separator => Some(
                div()
                    .py(DynamicSpacing::Base04.rems(cx))
                    .child(Divider::horizontal())
                    .into_any_element(),
            ),
            WorktreeEntry::SectionHeader(label) => Some(
                ListSubHeader::new(label.clone())
                    .inset(true)
                    .into_any_element(),
            ),
            WorktreeEntry::CreateFromCurrentBranch => {
                let branch_label = WorktreeCreateTarget::CurrentBranch.branch_label(
                    self.has_multiple_repositories,
                    self.current_branch_name.as_deref(),
                );

                let label = format!("Create new worktree based on {branch_label}");

                let item = create_new_list_item(
                    "create-from-current".to_string().into(),
                    label.into(),
                    self.creation_blocked_reason(cx),
                    selected,
                );

                Some(item.into_any_element())
            }
            WorktreeEntry::CreateFromDefaultBranch { default_branch } => {
                let branch_label = WorktreeCreateTarget::DefaultBranch(default_branch.clone())
                    .branch_label(
                        self.has_multiple_repositories,
                        self.current_branch_name.as_deref(),
                    );
                let label = format!("Create new worktree based on {branch_label}");

                let item = create_new_list_item(
                    "create-from-main".to_string().into(),
                    label.into(),
                    self.creation_blocked_reason(cx),
                    selected,
                );

                Some(item.into_any_element())
            }
            WorktreeEntry::Worktree {
                worktree,
                positions,
            } => {
                let main_worktree_path = self
                    .all_worktrees
                    .iter()
                    .find(|wt| wt.is_main)
                    .map(|wt| wt.path.as_path());
                let display_name = worktree.directory_name(main_worktree_path);
                let first_line = display_name.lines().next().unwrap_or(&display_name);
                let positions: Vec<_> = positions
                    .iter()
                    .copied()
                    .filter(|&pos| pos < first_line.len())
                    .collect();
                let path = worktree.path.compact().to_string_lossy().to_string();
                let sha = worktree.sha.chars().take(7).collect::<String>();

                let is_current = self.active_worktree_paths.contains(&worktree.path);
                let is_deleting = self.deleting_worktree_paths.contains(&worktree.path);
                let can_delete = self.can_delete_worktree(worktree);
                let can_remove_from_window =
                    !is_current && self.project_worktree_paths.contains(&worktree.path);

                let entry_icon = if is_current {
                    IconName::Check
                } else {
                    IconName::GitWorktree
                };
                let picker = cx.entity();

                Some(
                    ListItem::new(SharedString::from(format!("worktree-{ix}")))
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            h_flex()
                                .w_full()
                                .gap_2p5()
                                .child(
                                    Icon::new(entry_icon)
                                        .color(if is_current {
                                            Color::Accent
                                        } else {
                                            Color::Muted
                                        })
                                        .size(IconSize::Small),
                                )
                                .child(
                                    v_flex()
                                        .w_full()
                                        .min_w_0()
                                        .child(
                                            HighlightedLabel::new(first_line.to_owned(), positions)
                                                .truncate(),
                                        )
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .min_w_0()
                                                .gap_1p5()
                                                .when_some(
                                                    worktree.branch_name().map(|b| b.to_string()),
                                                    |this, branch| {
                                                        this.child(
                                                            Label::new(branch)
                                                                .size(LabelSize::Small)
                                                                .color(Color::Muted),
                                                        )
                                                        .child(
                                                            Label::new("\u{2022}")
                                                                .alpha(0.5)
                                                                .color(Color::Muted)
                                                                .size(LabelSize::Small),
                                                        )
                                                    },
                                                )
                                                .when(!sha.is_empty(), |this| {
                                                    this.child(
                                                        Label::new(sha)
                                                            .size(LabelSize::Small)
                                                            .color(Color::Muted),
                                                    )
                                                    .child(
                                                        Label::new("\u{2022}")
                                                            .alpha(0.5)
                                                            .color(Color::Muted)
                                                            .size(LabelSize::Small),
                                                    )
                                                })
                                                .child(
                                                    Label::new(path)
                                                        .truncate_start()
                                                        .color(Color::Muted)
                                                        .size(LabelSize::Small)
                                                        .flex_1(),
                                                ),
                                        ),
                                ),
                        )
                        .when(is_deleting, |this| {
                            this.end_slot(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        Icon::new(IconName::LoadCircle)
                                            .size(IconSize::Small)
                                            .color(Color::Muted)
                                            .with_rotate_animation(2),
                                    )
                                    .child(
                                        Label::new("Deleting…")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                        })
                        .when(!is_deleting && !is_current, |this| {
                            let open_in_new_window_button =
                                IconButton::new(("open-new-window", ix), IconName::ArrowUpRight)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Open in New Window"))
                                    .on_click(cx.listener(move |picker, _, window, cx| {
                                        let Some(entry) = picker.delegate.matches.get(ix) else {
                                            return;
                                        };
                                        if let WorktreeEntry::Worktree { worktree, .. } = entry {
                                            if picker
                                                .delegate
                                                .deleting_worktree_paths
                                                .contains(&worktree.path)
                                            {
                                                return;
                                            }
                                            window.dispatch_action(
                                                Box::new(OpenWorktreeInNewWindow {
                                                    path: worktree.path.clone(),
                                                }),
                                                cx,
                                            );
                                            cx.emit(DismissEvent);
                                        }
                                    }));

                            let focus_handle_delete = self.focus_handle.clone();
                            let force_delete = self.is_force_delete_hovering_index(ix);
                            let delete_button = div()
                                .id(("delete-worktree-hover", ix))
                                .on_hover(cx.listener(move |picker, hovered: &bool, _, cx| {
                                    if *hovered {
                                        picker.delegate.hovered_delete_index = Some(ix);
                                    } else if picker.delegate.hovered_delete_index == Some(ix) {
                                        picker.delegate.hovered_delete_index = None;
                                    }
                                    cx.notify();
                                }))
                                .child(
                                    IconButton::new(("delete-worktree", ix), IconName::Trash)
                                        .icon_size(IconSize::Small)
                                        .when(force_delete, |this| this.icon_color(Color::Error))
                                        .tooltip(move |_, cx| {
                                            cx.new(|cx| {
                                                DeleteWorktreeTooltip::new(
                                                    picker.clone(),
                                                    focus_handle_delete.clone(),
                                                    ix,
                                                    cx,
                                                )
                                            })
                                            .into()
                                        })
                                        .on_click(cx.listener(move |picker, _, window, cx| {
                                            let force = picker.delegate.modifiers.alt;
                                            picker.delegate.delete_worktree(ix, force, window, cx);
                                        })),
                                );

                            this.end_slot(
                                h_flex()
                                    .gap_0p5()
                                    .child(open_in_new_window_button)
                                    .when(can_remove_from_window, |this| {
                                        let worktree_path = worktree.path.clone();
                                        this.child(
                                            IconButton::new(
                                                ("remove-worktree-from-window", ix),
                                                IconName::Close,
                                            )
                                            .icon_size(IconSize::Small)
                                            .tooltip(Tooltip::text("Remove Worktree from Window"))
                                            .on_click(
                                                cx.listener(move |picker, _, window, cx| {
                                                    picker.delegate.remove_worktree_from_window(
                                                        &worktree_path,
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                            ),
                                        )
                                    })
                                    .when(can_delete, |this| this.child(delete_button)),
                            )
                            .show_end_slot_on_hover()
                        })
                        .into_any_element(),
                )
            }
            WorktreeEntry::CreateNamed {
                name,
                from_branch,
                disabled_reason,
            } => {
                let branch_label = from_branch
                    .as_ref()
                    .map(RemoteBranchName::display_name)
                    .unwrap_or_else(|| {
                        self.current_branch_name
                            .clone()
                            .unwrap_or_else(|| "HEAD".to_string())
                    });
                let label = format!("Create \"{name}\" based on {branch_label}");
                let element_id = match from_branch {
                    Some(branch) => format!("create-named-from-{}", branch.display_name()),
                    None => "create-named-from-current".to_string(),
                };

                let item = create_new_list_item(
                    element_id.into(),
                    label.into(),
                    disabled_reason.clone().map(SharedString::from),
                    selected,
                );

                Some(item.into_any_element())
            }
        }
    }
}

fn create_new_list_item(
    id: SharedString,
    label: SharedString,
    disabled_tooltip: Option<SharedString>,
    selected: bool,
) -> AnyElement {
    let is_disabled = disabled_tooltip.is_some();

    ListItem::new(id)
        .inset(true)
        .spacing(ListItemSpacing::Sparse)
        .toggle_state(selected)
        .child(
            h_flex()
                .w_full()
                .gap_2p5()
                .child(
                    Icon::new(IconName::Plus)
                        .map(|this| {
                            if is_disabled {
                                this.color(Color::Disabled)
                            } else {
                                this.color(Color::Muted)
                            }
                        })
                        .size(IconSize::Small),
                )
                .child(Label::new(label).when(is_disabled, |this| this.color(Color::Disabled))),
        )
        .when_some(disabled_tooltip, |this, reason| {
            this.tooltip(Tooltip::text(reason))
        })
        .into_any_element()
}
