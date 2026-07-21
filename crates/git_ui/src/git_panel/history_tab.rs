use super::*;

impl GitPanel {
    pub(super) fn render_history_tab(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex().flex_1().size_full().overflow_hidden().map(|this| {
            let has_repo = self.active_repository.is_some();
            let has_commits = self
                .commit_history_shas
                .as_ref()
                .map_or(false, |shas| !shas.is_empty());
            let is_loading = self.commit_history_shas.is_none() && has_repo;
            if is_loading {
                this.child(
                    h_flex()
                        .flex_1()
                        .justify_center()
                        .child(Label::new("Loading Commit History…").color(Color::Muted)),
                )
            } else if !has_repo || !has_commits {
                this.child(
                    h_flex()
                        .flex_1()
                        .justify_center()
                        .child(Label::new("No commits yet").color(Color::Muted)),
                )
            } else {
                match self.render_commit_history(window, cx) {
                    Some(history) => this.child(history),
                    None => this.child(
                        h_flex()
                            .flex_1()
                            .justify_center()
                            .child(Label::new("Failed to load commits").color(Color::Muted)),
                    ),
                }
            }
        })
    }

    pub(super) fn select_next_history_entry(&mut self, cx: &mut Context<Self>) {
        let count = self.commit_history_shas.as_ref().map_or(0, Vec::len);
        if count == 0 {
            return;
        }
        let new_index = match self.focused_history_entry {
            None => 0,
            Some(i) => (i + 1).min(count - 1),
        };
        self.focused_history_entry = Some(new_index);
        self.history_keyboard_nav = true;
        self.commit_history_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn select_previous_history_entry(&mut self, cx: &mut Context<Self>) {
        let count = self.commit_history_shas.as_ref().map_or(0, Vec::len);
        if count == 0 {
            return;
        }
        let new_index = match self.focused_history_entry {
            None => 0,
            Some(i) => i.saturating_sub(1),
        };
        self.focused_history_entry = Some(new_index);
        self.history_keyboard_nav = true;
        self.commit_history_scroll_handle
            .scroll_to_item(new_index, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn open_selected_history_commit(&self, window: &mut Window, cx: &mut App) {
        let Some(index) = self.focused_history_entry else {
            return;
        };
        let Some(sha) = self.commit_history_shas.as_ref().and_then(|s| s.get(index)) else {
            return;
        };
        let Some(active_repository) = self.active_repository.as_ref() else {
            return;
        };
        CommitView::open(
            sha.to_string(),
            active_repository.downgrade(),
            self.workspace.clone(),
            None,
            None,
            window,
            cx,
        );
    }

    pub(super) fn activate_changes_tab(
        &mut self,
        _: &ActivateChangesTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(GitPanelTab::Changes, window, cx);
    }

    pub(super) fn activate_history_tab(
        &mut self,
        _: &ActivateHistoryTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(GitPanelTab::History, window, cx);
    }

    pub(super) fn set_active_tab(
        &mut self,
        tab: GitPanelTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab == tab {
            return;
        }
        self.active_tab = tab;
        match tab {
            GitPanelTab::History => {
                self.focus_handle.focus(window, cx);
                self.load_commit_history(cx);
                self.focused_history_entry = Some(0);
            }
            GitPanelTab::Changes => {
                self.focus_handle.focus(window, cx);
                self.commit_history_shas.take();
                self.focused_history_entry = None;
                self._repo_subscriptions.clear();
            }
        }
        cx.notify();
    }

    pub(super) fn preload_commit_history(&mut self, cx: &mut Context<Self>) {
        let Some(active_repository) = self.active_repository.as_ref() else {
            return;
        };

        let Some(branch) = active_repository.read(cx).branch.as_ref() else {
            return;
        };

        let branch_name = branch.name().to_string();
        let log_source = LogSource::Branch(branch_name.into());
        let log_order = LogOrder::DateOrder;

        // Kick off the git log fetch so data is ready when the user switches to History.
        // graph_data() is idempotent — if already loading/loaded, this is a no-op.
        active_repository.update(cx, |repository, cx| {
            repository.graph_data(log_source, log_order, 0..0, cx);
        });
    }

    pub(super) fn load_commit_history(&mut self, cx: &mut Context<Self>) {
        let Some(active_repository) = self.active_repository.as_ref() else {
            return;
        };

        if self._repo_subscriptions.is_empty() {
            self._repo_subscriptions.push(cx.subscribe(
                active_repository,
                |this, _repo, event, cx| {
                    if let RepositoryEvent::GraphEvent(_, _) = event {
                        if this.active_tab == GitPanelTab::History {
                            this.fetch_commit_history_shas(cx);
                        }
                    }
                },
            ));
            self._repo_subscriptions
                .push(cx.observe(active_repository, |_this, _repo, cx| {
                    cx.notify();
                }));
        }

        self.fetch_commit_history_shas(cx);
    }

    pub(super) fn fetch_commit_history_shas(&mut self, cx: &mut Context<Self>) {
        let Some(active_repository) = self.active_repository.as_ref() else {
            return;
        };

        let Some(branch) = active_repository.read(cx).branch.as_ref() else {
            return;
        };

        let branch_name = branch.name().to_string();
        let log_source = LogSource::Branch(branch_name.into());
        let log_order = LogOrder::DateOrder;

        self.commit_history_shas = Some(active_repository.update(cx, |repository, cx| {
            let response = repository.graph_data(log_source, log_order, 0..usize::MAX, cx);
            response.commits.iter().map(|commit| commit.sha).collect()
        }));
    }

    pub(super) fn git_remote(&self, cx: &mut App) -> Option<GitRemote> {
        let repo = self.active_repository.as_ref()?;
        let remote_url = repo.read(cx).default_remote_url()?;
        let provider_registry = GitHostingProviderRegistry::default_global(cx);
        let (provider, parsed) = parse_git_remote_url(provider_registry, &remote_url)?;
        Some(GitRemote {
            host: provider,
            owner: parsed.owner.into(),
            repo: parsed.repo.into(),
        })
    }

    pub(super) fn render_commit_history(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let shas = self.commit_history_shas.clone()?;
        let active_repository = self.active_repository.as_ref()?;
        let workspace = self.workspace.clone();
        let repo_weak = active_repository.downgrade();
        let item_count = shas.len();
        let commit_history_scroll_handle = self.commit_history_scroll_handle.clone();
        let remote = self.git_remote(cx);

        let focused_history_entry = self.focused_history_entry;
        let is_panel_focused = self.focus_handle.is_focused(window);
        let show_focus_border = self.history_keyboard_nav;

        let ahead_count = active_repository
            .read(cx)
            .branch
            .as_ref()
            .and_then(|b| b.upstream.as_ref())
            .and_then(|u| u.tracking.status())
            .map(|s| s.ahead as usize)
            .unwrap_or(0);

        Some(
            v_flex()
                .flex_1()
                .size_full()
                .overflow_hidden()
                .child(
                    uniform_list("commit_history_list", item_count, {
                        let workspace = workspace;
                        let repo_weak = repo_weak;
                        let git_panel = cx.weak_entity();
                        move |range, window, cx| {
                            let local_offset = time::UtcOffset::current_local_offset()
                                .unwrap_or(time::UtcOffset::UTC);
                            let now = time::OffsetDateTime::now_utc();

                            let visible_data: Vec<Option<Arc<CommitData>>> = repo_weak
                                .update(cx, |repository, cx| {
                                    shas[range.clone()]
                                        .iter()
                                        .map(|sha| {
                                            match repository.fetch_commit_data(*sha, false, cx) {
                                                CommitDataState::Loaded(data) => Some(data.clone()),
                                                CommitDataState::Loading(_) => None,
                                            }
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            shas[range.clone()]
                                .iter()
                                .zip(visible_data)
                                .enumerate()
                                .map(|(ix, (sha, data))| {
                                    let index = range.start + ix;
                                    let sha_string = sha.to_string();
                                    let sha_shared: SharedString = sha_string.clone().into();
                                    let short_sha: SharedString =
                                        sha_string[..7.min(sha_string.len())].to_string().into();

                                    let (subject, author_name, author_email, timestamp): (
                                        SharedString,
                                        SharedString,
                                        Option<SharedString>,
                                        Option<i64>,
                                    ) = match &data {
                                        Some(data) => (
                                            data.subject.clone(),
                                            data.author_name.clone(),
                                            Some(data.author_email.clone()),
                                            Some(data.commit_timestamp),
                                        ),
                                        None => ("Loading…".into(), "".into(), None, None),
                                    };

                                    let relative_time: SharedString = timestamp
                                        .and_then(|ts| {
                                            time::OffsetDateTime::from_unix_timestamp(ts).ok()
                                        })
                                        .map(|dt| {
                                            time_format::format_localized_timestamp(
                                                dt,
                                                now,
                                                local_offset,
                                                time_format::TimestampFormat::Relative,
                                            )
                                            .into()
                                        })
                                        .unwrap_or_else(|| "".into());

                                    let avatar = CommitAvatar::new(
                                        &sha_shared,
                                        author_email,
                                        remote.as_ref(),
                                    )
                                    .size(px(14.))
                                    .render(window, cx);

                                    let is_unpushed = index < ahead_count;
                                    let is_focused = focused_history_entry == Some(index);
                                    let workspace = workspace.clone();
                                    let repo = repo_weak.clone();
                                    let sha_for_click = sha_string;

                                    let dot_separator = || {
                                        Label::new("•")
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .alpha(0.5)
                                    };

                                    v_flex()
                                        .id(("commit-history-item", index))
                                        .cursor_pointer()
                                        .w_full()
                                        .py_1()
                                        .px_2()
                                        .gap_0p5()
                                        .border_1()
                                        .border_color(gpui::transparent_black())
                                        .when(
                                            is_focused && is_panel_focused && show_focus_border,
                                            |this| {
                                                this.border_color(
                                                    cx.theme().colors().panel_focused_border,
                                                )
                                            },
                                        )
                                        .hover(|s| s.bg(cx.theme().colors().element_hover))
                                        .child(
                                            h_flex()
                                                .gap_1()
                                                .w_full()
                                                .child(Label::new(subject).truncate())
                                                .when(is_unpushed, |this| {
                                                    this.child(
                                                        Icon::new(IconName::ArrowUp)
                                                            .size(IconSize::XSmall),
                                                    )
                                                }),
                                        )
                                        .child(
                                            h_flex()
                                                .gap_1p5()
                                                .child(avatar)
                                                .when(!author_name.is_empty(), |this| {
                                                    this.child(
                                                        Label::new(author_name)
                                                            .size(LabelSize::Small)
                                                            .color(Color::Muted),
                                                    )
                                                    .child(dot_separator())
                                                })
                                                .when(!relative_time.is_empty(), |this| {
                                                    this.child(
                                                        Label::new(relative_time)
                                                            .size(LabelSize::Small)
                                                            .color(Color::Muted),
                                                    )
                                                    .child(dot_separator())
                                                })
                                                .child(
                                                    Label::new(short_sha.clone())
                                                        .size(LabelSize::Small)
                                                        .color(Color::Muted),
                                                ),
                                        )
                                        .tooltip(move |_, cx| {
                                            Tooltip::with_meta(
                                                "View Commit",
                                                None,
                                                short_sha.clone(),
                                                cx,
                                            )
                                        })
                                        .on_mouse_down(gpui::MouseButton::Left, {
                                            let git_panel = git_panel.clone();
                                            move |_, _, cx| {
                                                git_panel
                                                    .update(cx, |panel, cx| {
                                                        panel.focused_history_entry = Some(index);
                                                        panel.history_keyboard_nav = false;
                                                        cx.notify();
                                                    })
                                                    .ok();
                                            }
                                        })
                                        .on_click(move |_, window, cx| {
                                            CommitView::open(
                                                sha_for_click.clone(),
                                                repo.clone(),
                                                workspace.clone(),
                                                None,
                                                None,
                                                window,
                                                cx,
                                            );
                                        })
                                        .into_any_element()
                                })
                                .collect()
                        }
                    })
                    .size_full()
                    .track_scroll(&commit_history_scroll_handle),
                )
                .vertical_scrollbar_for(&commit_history_scroll_handle, window, cx),
        )
    }
}
