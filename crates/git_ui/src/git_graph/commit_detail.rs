use super::*;

impl GitGraph {
    pub(super) fn render_loading_spinner(&self, cx: &App) -> AnyElement {
        let rems = TextSize::Large.rems(cx);
        Icon::new(IconName::LoadCircle)
            .size(IconSize::Custom(rems))
            .color(Color::Accent)
            .with_rotate_animation(3)
            .into_any_element()
    }

    pub(super) fn render_commit_detail_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(selected_idx) = self.selected_entry_idx else {
            return Empty.into_any_element();
        };

        let Some(commit_entry) = self.graph_data.commits.get(selected_idx) else {
            return Empty.into_any_element();
        };

        let Some(repository) = self.get_repository(cx) else {
            return Empty.into_any_element();
        };

        let data = repository.update(cx, |repository, cx| {
            repository
                .fetch_commit_data(commit_entry.data.sha, false, cx)
                .clone()
        });

        let full_sha: SharedString = commit_entry.data.sha.to_string().into();
        let ref_names = commit_entry.data.ref_names.clone();

        let head_branch_name: Option<SharedString> = repository
            .read(cx)
            .snapshot()
            .branch
            .as_ref()
            .map(|branch| SharedString::from(branch.name().to_string()));

        let accent_colors = cx.theme().accents();
        let accent_color = accent_colors
            .0
            .get(commit_entry.color_idx)
            .copied()
            .unwrap_or_else(|| accent_colors.0.first().copied().unwrap_or_default());

        // todo(git graph): We should use the full commit message here
        let (author_name, author_email, commit_timestamp, commit_message) = match &data {
            CommitDataState::Loaded(data) => (
                data.author_name.clone(),
                data.author_email.clone(),
                Some(data.commit_timestamp),
                data.subject.clone(),
            ),
            CommitDataState::Loading(_) => ("Loading…".into(), "".into(), None, "Loading…".into()),
        };

        let date_string = commit_timestamp
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .map(|datetime| {
                let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
                let local_datetime = datetime.to_offset(local_offset);
                let format =
                    time::format_description::parse("[month repr:short] [day], [year]").ok();
                format
                    .and_then(|f| local_datetime.format(&f).ok())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let remote = repository.update(cx, |repo, cx| {
            let remote_url = repo.default_remote_url()?;
            let provider_registry = GitHostingProviderRegistry::default_global(cx);
            let (provider, parsed) = parse_git_remote_url(provider_registry, &remote_url)?;
            Some(GitRemote {
                host: provider,
                owner: parsed.owner.into(),
                repo: parsed.repo.into(),
            })
        });

        let avatar = {
            let author_email_for_avatar = if author_email.is_empty() {
                None
            } else {
                Some(author_email.clone())
            };

            CommitAvatar::new(&full_sha, author_email_for_avatar, remote.as_ref())
                .size(px(40.))
                .render(window, cx)
        };

        let changed_files_count = self
            .selected_commit_diff
            .as_ref()
            .map(|diff| diff.files.len())
            .unwrap_or(0);

        let (total_lines_added, total_lines_removed) =
            self.selected_commit_diff_stats.unwrap_or((0, 0));

        let changed_file_entries: Vec<ChangedFileEntry> = self
            .selected_commit_diff
            .as_ref()
            .map(|diff| {
                let mut files = diff.files.iter().collect::<Vec<_>>();
                if !self.changed_files_view_mode.is_tree() {
                    files.sort_by_key(|file| file.status());
                }
                files
                    .into_iter()
                    .map(|file| ChangedFileEntry::from_commit_file(file, cx))
                    .collect()
            })
            .unwrap_or_default();
        let changed_file_entries = Rc::new(changed_file_entries);
        let tree_entries: Rc<Vec<ChangedFileTreeEntry>> = if self.changed_files_view_mode.is_tree()
        {
            Rc::new(build_changed_file_tree_entries(
                changed_file_entries.as_ref().clone(),
                &self.changed_files_expanded_dirs,
            ))
        } else {
            Rc::default()
        };

        v_flex()
            .min_w(px(300.))
            .h_full()
            .bg(cx.theme().colors().editor_background)
            .flex_basis(DefiniteLength::Fraction(
                self.commit_details_split_state.read(cx).right_ratio(),
            ))
            .child(
                v_flex()
                    .relative()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .child(
                        div().absolute().top_2().right_2().child(
                            IconButton::new("close-detail", IconName::Close)
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_entry_idx = None;
                                    this.selected_commit_diff = None;
                                    this.selected_commit_diff_stats = None;
                                    this.changed_files_expanded_dirs.clear();
                                    this._commit_diff_task = None;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        v_flex()
                            .py_1()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .child(avatar)
                            .child(
                                v_flex()
                                    .items_center()
                                    .child(Label::new(author_name))
                                    .child(
                                        Label::new(date_string)
                                            .color(Color::Muted)
                                            .size(LabelSize::Small),
                                    ),
                            ),
                    )
                    .children((!ref_names.is_empty()).then(|| {
                        h_flex().gap_1().flex_wrap().justify_center().children(
                            ref_names.iter().map(|name| {
                                let is_head = Self::is_head_ref(name.as_ref(), &head_branch_name);
                                self.render_ref_chip(name, accent_color, is_head, selected_idx, cx)
                            }),
                        )
                    }))
                    .child(
                        v_flex()
                            .ml_neg_1()
                            .gap_1p5()
                            .when(!author_email.is_empty(), |this| {
                                let copied_state: Entity<CopiedState> = window.use_keyed_state(
                                    "author-email-copy",
                                    cx,
                                    CopiedState::new,
                                );
                                let is_copied = copied_state.read(cx).is_copied();

                                let (icon, icon_color, tooltip_label) = if is_copied {
                                    (IconName::Check, Color::Success, "Email Copied!")
                                } else {
                                    (IconName::Envelope, Color::Muted, "Copy Email")
                                };

                                let copy_email = author_email.clone();
                                let author_email_for_tooltip = author_email.clone();

                                this.child(
                                    Button::new("author-email-copy", author_email.clone())
                                        .start_icon(
                                            Icon::new(icon).size(IconSize::Small).color(icon_color),
                                        )
                                        .label_size(LabelSize::Small)
                                        .truncate(true)
                                        .color(Color::Muted)
                                        .tooltip(move |_, cx| {
                                            Tooltip::with_meta(
                                                tooltip_label,
                                                None,
                                                author_email_for_tooltip.clone(),
                                                cx,
                                            )
                                        })
                                        .on_click(move |_, _, cx| {
                                            copied_state.update(cx, |state, _cx| {
                                                state.mark_copied();
                                            });
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy_email.to_string(),
                                            ));
                                            let state_id = copied_state.entity_id();
                                            cx.spawn(async move |cx| {
                                                cx.background_executor()
                                                    .timer(COPIED_STATE_DURATION)
                                                    .await;
                                                cx.update(|cx| {
                                                    cx.notify(state_id);
                                                })
                                            })
                                            .detach();
                                        }),
                                )
                            })
                            .child({
                                let copy_sha = full_sha.clone();
                                let copied_state: Entity<CopiedState> =
                                    window.use_keyed_state("sha-copy", cx, CopiedState::new);
                                let is_copied = copied_state.read(cx).is_copied();

                                let (icon, icon_color, tooltip_label) = if is_copied {
                                    (IconName::Check, Color::Success, "Commit SHA Copied!")
                                } else {
                                    (IconName::Hash, Color::Muted, "Copy Commit SHA")
                                };

                                Button::new("sha-button", &full_sha)
                                    .start_icon(
                                        Icon::new(icon).size(IconSize::Small).color(icon_color),
                                    )
                                    .label_size(LabelSize::Small)
                                    .truncate(true)
                                    .color(Color::Muted)
                                    .tooltip({
                                        let full_sha = full_sha.clone();
                                        move |_, cx| {
                                            Tooltip::with_meta(
                                                tooltip_label,
                                                None,
                                                full_sha.clone(),
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(move |_, _, cx| {
                                        copied_state.update(cx, |state, _cx| {
                                            state.mark_copied();
                                        });
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_sha.to_string(),
                                        ));
                                        let state_id = copied_state.entity_id();
                                        cx.spawn(async move |cx| {
                                            cx.background_executor()
                                                .timer(COPIED_STATE_DURATION)
                                                .await;
                                            cx.update(|cx| {
                                                cx.notify(state_id);
                                            })
                                        })
                                        .detach();
                                    })
                            })
                            .when_some(remote.clone(), |this, remote| {
                                let provider_name = remote.host.name();
                                let icon = crate::get_provider_icon(provider_name.as_str());
                                let parsed_remote = ParsedGitRemote {
                                    owner: remote.owner.as_ref().into(),
                                    repo: remote.repo.as_ref().into(),
                                };
                                let params = BuildCommitPermalinkParams {
                                    sha: full_sha.as_ref(),
                                };
                                let url = remote
                                    .host
                                    .build_commit_permalink(&parsed_remote, params)
                                    .to_string();

                                this.child(
                                    Button::new(
                                        "view-on-provider",
                                        format!("View on {}", provider_name),
                                    )
                                    .start_icon(
                                        Icon::new(icon).size(IconSize::Small).color(Color::Muted),
                                    )
                                    .label_size(LabelSize::Small)
                                    .truncate(true)
                                    .color(Color::Muted)
                                    .on_click(
                                        move |_, _, cx| {
                                            cx.open_url(&url);
                                        },
                                    ),
                                )
                            }),
                    ),
            )
            .child(Divider::horizontal())
            .child(div().p_2().child(Label::new(commit_message)))
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .min_w_0()
                    .p_2()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1()
                            .w_full()
                            .justify_between()
                            .child(
                                Label::new(format!(
                                    "{} Changed {}",
                                    changed_files_count,
                                    if changed_files_count == 1 {
                                        "File"
                                    } else {
                                        "Files"
                                    }
                                ))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(DiffStat::new(
                                        "commit-diff-stat",
                                        total_lines_added,
                                        total_lines_removed,
                                    ))
                                    .child(
                                        IconButton::new(
                                            "toggle-changed-files-view",
                                            IconName::ListTree,
                                        )
                                        .shape(ui::IconButtonShape::Square)
                                        .icon_size(IconSize::Small)
                                        .toggle_state(self.changed_files_view_mode.is_tree())
                                        .tooltip({
                                            let tooltip = if self.changed_files_view_mode.is_tree()
                                            {
                                                "Show Flat View"
                                            } else {
                                                "Show Tree View"
                                            };
                                            move |_, cx| {
                                                Tooltip::for_action(
                                                    tooltip,
                                                    &ToggleChangedFilesView,
                                                    cx,
                                                )
                                            }
                                        })
                                        .on_click(
                                            cx.listener(|this, _, _window, cx| {
                                                this.changed_files_view_mode =
                                                    this.changed_files_view_mode.toggled();
                                                this.changed_files_scroll_handle
                                                    .scroll_to_item(0, ScrollStrategy::Top);
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("changed-files-container")
                            .flex_1()
                            .min_h_0()
                            .child({
                                let flat_entries = changed_file_entries;
                                let is_tree_view = self.changed_files_view_mode.is_tree();
                                let entry_count = if is_tree_view {
                                    tree_entries.len()
                                } else {
                                    flat_entries.len()
                                };
                                let commit_sha = full_sha.clone();
                                let repository = repository.downgrade();
                                let workspace = self.workspace.clone();
                                let git_graph = cx.weak_entity();
                                uniform_list(
                                    "changed-files-list",
                                    entry_count,
                                    move |range, _window, cx| {
                                        range
                                            .map(|ix| {
                                                if is_tree_view {
                                                    match &tree_entries[ix] {
                                                        ChangedFileTreeEntry::Directory(entry) => {
                                                            entry.render(ix, git_graph.clone(), cx)
                                                        }
                                                        ChangedFileTreeEntry::File(entry) => {
                                                            entry.entry.render(
                                                                ix,
                                                                entry.depth,
                                                                None,
                                                                commit_sha.clone(),
                                                                repository.clone(),
                                                                workspace.clone(),
                                                                cx,
                                                            )
                                                        }
                                                    }
                                                } else {
                                                    let directory_label = (!flat_entries[ix]
                                                        .dir_path
                                                        .is_empty())
                                                    .then(|| flat_entries[ix].dir_path.clone());
                                                    flat_entries[ix].render(
                                                        ix,
                                                        0,
                                                        directory_label,
                                                        commit_sha.clone(),
                                                        repository.clone(),
                                                        workspace.clone(),
                                                        cx,
                                                    )
                                                }
                                            })
                                            .collect()
                                    },
                                )
                                .size_full()
                                .ml_neg_1()
                                .track_scroll(&self.changed_files_scroll_handle)
                            })
                            .vertical_scrollbar_for(&self.changed_files_scroll_handle, window, cx),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                h_flex().p_1p5().w_full().child(
                    Button::new("view-commit", "View Commit")
                        .full_width()
                        .style(ButtonStyle::OutlinedGhost)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_commit_view(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }
}
