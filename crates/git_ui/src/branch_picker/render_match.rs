use super::*;

impl BranchListDelegate {
    fn render_branch_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<ListItem> {
        let entry = &self.matches.get(ix)?;

        let (commit_time, absolute_time, author_name, subject) = entry
            .as_branch()
            .and_then(|branch| {
                branch.most_recent_commit.as_ref().map(|commit| {
                    let subject = commit.subject.clone();
                    let commit_time = OffsetDateTime::from_unix_timestamp(commit.commit_timestamp)
                        .unwrap_or_else(|_| OffsetDateTime::now_utc());
                    let local_offset =
                        time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
                    let formatted_time = time_format::format_localized_timestamp(
                        commit_time,
                        OffsetDateTime::now_utc(),
                        local_offset,
                        time_format::TimestampFormat::Relative,
                    );
                    let absolute_time = time_format::format_localized_timestamp(
                        commit_time,
                        OffsetDateTime::now_utc(),
                        local_offset,
                        time_format::TimestampFormat::EnhancedAbsolute,
                    );
                    let author = commit.author_name.clone();
                    (
                        Some(formatted_time),
                        Some(absolute_time),
                        Some(author),
                        Some(subject),
                    )
                })
            })
            .unwrap_or_else(|| (None, None, None, None));

        let is_head_branch = entry.as_branch().is_some_and(|branch| branch.is_head);
        let is_checked_branch = entry.as_branch().is_some_and(|branch| {
            if self.is_select_only() {
                self.branch_selection_behavior
                    .selected_branch()
                    .is_some_and(|selected_branch| branch_matches_ref(branch, selected_branch))
            } else {
                branch.is_head
            }
        });

        let entry_icon = match entry {
            Entry::NewUrl { .. } | Entry::NewBranch { .. } | Entry::NewRemoteName { .. } => {
                IconName::Plus
            }
            Entry::Branch { branch, .. } => {
                if is_checked_branch {
                    IconName::Check
                } else if branch.is_remote() {
                    IconName::Screen
                } else {
                    IconName::GitBranch
                }
            }
        };

        let entry_title = match entry {
            Entry::NewUrl { .. } => Label::new("Create Remote Repository")
                .single_line()
                .truncate()
                .into_any_element(),
            Entry::NewBranch { name } => Label::new(format!("Create Branch: \"{name}\"…"))
                .single_line()
                .truncate()
                .into_any_element(),
            Entry::NewRemoteName { name, .. } => Label::new(format!("Create Remote: \"{name}\""))
                .single_line()
                .truncate()
                .into_any_element(),
            Entry::Branch { branch, positions } => {
                HighlightedLabel::new(branch.name().to_string(), positions.clone())
                    .single_line()
                    .truncate()
                    .into_any_element()
            }
        };

        let focus_handle = self.focus_handle.clone();
        let picker = cx.entity();
        let is_new_items = matches!(
            entry,
            Entry::NewUrl { .. } | Entry::NewBranch { .. } | Entry::NewRemoteName { .. }
        );

        let deleted_branch_icon = |entry_ix: usize| {
            let picker = picker.clone();
            let focus_handle = focus_handle.clone();
            let force_delete = self.is_force_delete_hovering_index(entry_ix);

            div()
                .id(("delete-hover", entry_ix))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.delegate.hovered_delete_index = Some(entry_ix);
                    } else if this.delegate.hovered_delete_index == Some(entry_ix) {
                        this.delegate.hovered_delete_index = None;
                    }
                    cx.notify();
                }))
                .child(
                    IconButton::new(("delete", entry_ix), IconName::Trash)
                        .icon_size(IconSize::Small)
                        .when(force_delete, |this| this.icon_color(Color::Error))
                        .tooltip(move |_, cx| {
                            cx.new(|cx| {
                                DeleteBranchTooltip::new(
                                    picker.clone(),
                                    focus_handle.clone(),
                                    entry_ix,
                                    cx,
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.delegate.delete_at(
                                entry_ix,
                                this.delegate.modifiers.alt,
                                window,
                                cx,
                            );
                        })),
                )
        };

        let create_from_default_button = self.default_branch.as_ref().map(|default_branch| {
            let tooltip_label: SharedString = format!("Create New From: {default_branch}").into();
            let focus_handle = self.focus_handle.clone();

            IconButton::new("create_from_default", IconName::GitBranchPlus)
                .icon_size(IconSize::Small)
                .tooltip(move |_, cx| {
                    Tooltip::for_action_in(
                        tooltip_label.clone(),
                        &menu::SecondaryConfirm,
                        &focus_handle,
                        cx,
                    )
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.delegate.confirm(true, window, cx);
                }))
                .into_any_element()
        });

        Some(
            ListItem::new(format!("vcs-menu-{ix}"))
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(
                    h_flex()
                        .w_full()
                        .gap_2p5()
                        .flex_grow_1()
                        .child(
                            Icon::new(entry_icon)
                                .color(if is_checked_branch {
                                    Color::Accent
                                } else {
                                    Color::Muted
                                })
                                .size(IconSize::Small),
                        )
                        .child(
                            v_flex()
                                .id("info_container")
                                .w_full()
                                .child(entry_title)
                                .child({
                                    let message = match entry {
                                        Entry::NewUrl { url } => format!("Based off {url}"),
                                        Entry::NewRemoteName { url, .. } => {
                                            format!("Based off {url}")
                                        }
                                        Entry::NewBranch { .. } => {
                                            if let Some(current_branch) =
                                                self.repo.as_ref().and_then(|repo| {
                                                    repo.read(cx).branch.as_ref().map(|b| b.name())
                                                })
                                            {
                                                format!("Based off {}", current_branch)
                                            } else {
                                                "Based off the current branch".to_string()
                                            }
                                        }
                                        Entry::Branch { .. } => String::new(),
                                    };

                                    if matches!(entry, Entry::Branch { .. }) {
                                        let show_author_name = ProjectSettings::get_global(cx)
                                            .git
                                            .branch_picker
                                            .show_author_name;
                                        let has_author = show_author_name && author_name.is_some();
                                        let has_commit = commit_time.is_some();
                                        let author_for_meta =
                                            if show_author_name { author_name } else { None };

                                        let dot = || {
                                            Label::new("•")
                                                .alpha(0.5)
                                                .color(Color::Muted)
                                                .size(LabelSize::Small)
                                        };

                                        h_flex()
                                            .w_full()
                                            .min_w_0()
                                            .gap_1p5()
                                            .when_some(author_for_meta, |this, author| {
                                                this.child(
                                                    Label::new(author)
                                                        .color(Color::Muted)
                                                        .size(LabelSize::Small),
                                                )
                                            })
                                            .when_some(commit_time, |this, time| {
                                                this.when(has_author, |this| this.child(dot()))
                                                    .child(
                                                        Label::new(time)
                                                            .color(Color::Muted)
                                                            .size(LabelSize::Small),
                                                    )
                                            })
                                            .when_some(subject, |this, subj| {
                                                this.when(has_commit, |this| this.child(dot()))
                                                    .child(
                                                        Label::new(subj.to_string())
                                                            .color(Color::Muted)
                                                            .size(LabelSize::Small)
                                                            .truncate()
                                                            .flex_1(),
                                                    )
                                            })
                                            .when(!has_commit, |this| {
                                                this.child(
                                                    Label::new("No commits found")
                                                        .color(Color::Muted)
                                                        .size(LabelSize::Small),
                                                )
                                            })
                                            .into_any_element()
                                    } else {
                                        Label::new(message)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .truncate()
                                            .into_any_element()
                                    }
                                })
                                .when_some(
                                    entry.as_branch().map(|b| b.name().to_string()),
                                    |this, branch_name| {
                                        let absolute_time = absolute_time.clone();
                                        this.tooltip({
                                            let is_head = is_head_branch;
                                            let is_checked = is_checked_branch;
                                            let is_select_only = self.is_select_only();
                                            Tooltip::element(move |_, _| {
                                                v_flex()
                                                    .child(Label::new(branch_name.clone()))
                                                    .when(is_select_only && is_checked, |this| {
                                                        this.child(
                                                            Label::new("Selected Branch")
                                                                .size(LabelSize::Small)
                                                                .color(Color::Muted),
                                                        )
                                                    })
                                                    .when(is_head, |this| {
                                                        this.child(
                                                            Label::new("Current Branch")
                                                                .size(LabelSize::Small)
                                                                .color(Color::Muted),
                                                        )
                                                    })
                                                    .when_some(
                                                        absolute_time.clone(),
                                                        |this, time| {
                                                            this.child(
                                                                Label::new(time)
                                                                    .size(LabelSize::Small)
                                                                    .color(Color::Muted),
                                                            )
                                                        },
                                                    )
                                                    .into_any_element()
                                            })
                                        })
                                    },
                                ),
                        ),
                )
                .when(
                    !self.is_select_only() && !is_new_items && !is_head_branch,
                    |this| {
                        this.end_slot(deleted_branch_icon(ix))
                            .show_end_slot_on_hover()
                    },
                )
                .when_some(
                    if is_new_items {
                        create_from_default_button
                    } else {
                        None
                    },
                    |this, create_from_default_button| {
                        this.end_slot(create_from_default_button)
                            .show_end_slot_on_hover()
                    },
                ),
        )
    }
}
