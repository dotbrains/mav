use super::*;

impl GitGraph {
    pub(super) fn render_chip(
        &self,
        name: &SharedString,
        accent_color: gpui::Hsla,
        is_head: bool,
    ) -> impl IntoElement {
        Chip::new(name.clone())
            .label_size(LabelSize::Small)
            .truncate()
            .map(|chip| {
                if is_head {
                    chip.icon(IconName::Check)
                        .bg_color(accent_color.opacity(0.25))
                        .border_color(accent_color.opacity(0.5))
                } else {
                    chip.bg_color(accent_color.opacity(0.08))
                        .border_color(accent_color.opacity(0.25))
                }
            })
    }

    /// Renders a ref chip for the commit at `commit_idx`. Chips that name a ref
    /// (branch, remote ref, or tag) get a right-click handler that opens a
    /// ref-specific context menu, so that custom commands can be resolved
    /// against the clicked ref.
    pub(super) fn render_ref_chip(
        &self,
        name: &SharedString,
        accent_color: gpui::Hsla,
        is_head: bool,
        commit_idx: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = self.render_chip(name, accent_color, is_head);
        let Some(ref_name) = Self::ref_name_from_decoration(name) else {
            return chip.into_any_element();
        };
        div()
            .child(chip)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.deploy_entry_context_menu(
                        event.position,
                        commit_idx,
                        Some(ref_name.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_table_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Vec<AnyElement>> {
        let repository = self.get_repository(cx);

        let head_branch_name: Option<SharedString> = repository.as_ref().and_then(|repo| {
            repo.read(cx)
                .snapshot()
                .branch
                .as_ref()
                .map(|branch| SharedString::from(branch.name().to_string()))
        });

        let row_height = Self::row_height(window, cx);
        let has_context_menu = self.has_context_menu();

        // We fetch data outside the visible viewport to avoid loading entries when
        // users scroll through the git graph
        if let Some(repository) = repository.as_ref() {
            const FETCH_RANGE: usize = 100;
            repository.update(cx, |repository, cx| {
                self.graph_data.commits[range.start.saturating_sub(FETCH_RANGE)
                    ..(range.end + FETCH_RANGE)
                        .min(self.graph_data.commits.len().saturating_sub(1))]
                    .iter()
                    .for_each(|commit| {
                        repository.fetch_commit_data(commit.data.sha, false, cx);
                    });
            });
        }

        range
            .map(|idx| {
                let Some((commit, repository)) =
                    self.graph_data.commits.get(idx).zip(repository.as_ref())
                else {
                    return vec![
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                    ];
                };

                let data = repository.update(cx, |repository, cx| {
                    repository
                        .fetch_commit_data(commit.data.sha, false, cx)
                        .clone()
                });

                let short_sha = commit.data.sha.display_short();
                let mut formatted_time = String::new();
                let subject: SharedString;
                let author_name: SharedString;

                if let CommitDataState::Loaded(ref data) = data {
                    subject = data.subject.clone();
                    author_name = data.author_name.clone();
                    formatted_time = format_timestamp(data.commit_timestamp);
                } else {
                    subject = "Loading…".into();
                    author_name = "".into();
                }

                let accent_colors = cx.theme().accents();
                let accent_color = accent_colors
                    .0
                    .get(commit.color_idx)
                    .copied()
                    .unwrap_or_else(|| accent_colors.0.first().copied().unwrap_or_default());

                let is_selected = self.selected_entry_idx == Some(idx);
                let is_matched = self.search_state.matches.contains(&commit.data.sha);
                let column_label = |label: SharedString| {
                    Label::new(label)
                        .when(!is_selected, |c| c.color(Color::Muted))
                        .truncate()
                        .into_any_element()
                };

                let subject_label = if is_matched {
                    let query = match &self.search_state.state {
                        QueryState::Confirmed((query, _)) => Some(query.clone()),
                        _ => None,
                    };
                    let highlight_ranges = query
                        .and_then(|q| {
                            let ranges = if self.search_state.case_sensitive {
                                subject
                                    .match_indices(q.as_str())
                                    .map(|(start, matched)| start..start + matched.len())
                                    .collect::<Vec<_>>()
                            } else {
                                let q = q.to_lowercase();
                                let subject_lower = subject.to_lowercase();

                                subject_lower
                                    .match_indices(&q)
                                    .filter_map(|(start, matched)| {
                                        let end = start + matched.len();
                                        subject.is_char_boundary(start).then_some(()).and_then(
                                            |_| subject.is_char_boundary(end).then_some(start..end),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };

                            (!ranges.is_empty()).then_some(ranges)
                        })
                        .unwrap_or_default();
                    HighlightedLabel::from_ranges(subject, highlight_ranges)
                        .when(!is_selected, |c| c.color(Color::Muted))
                        .truncate()
                        .into_any_element()
                } else {
                    column_label(subject)
                };

                vec![
                    div()
                        .id(ElementId::NamedInteger("commit-subject".into(), idx as u64))
                        .overflow_hidden()
                        .when(!has_context_menu, |this| {
                            if let CommitDataState::Loaded(commit_data) = &data {
                                let sha = commit.data.sha.to_string();
                                let author_name = commit_data.author_name.clone();
                                let author_email = commit_data.author_email.clone();
                                let message = commit_data.message.clone();
                                let commit_timestamp = commit_data.commit_timestamp;
                                let workspace = self.workspace.clone();
                                let repository = repository.clone();
                                this.hoverable_tooltip(move |_window, cx| {
                                    let remote_url = repository.read(cx).default_remote_url();
                                    let provider_registry =
                                        GitHostingProviderRegistry::default_global(cx);
                                    let commit_details = CommitDetails {
                                        sha: sha.clone().into(),
                                        author_name: author_name.clone(),
                                        author_email: author_email.clone(),
                                        commit_time: OffsetDateTime::from_unix_timestamp(
                                            commit_timestamp,
                                        )
                                        .unwrap_or_else(|_| OffsetDateTime::now_utc()),
                                        message: Some(ParsedCommitMessage::parse(
                                            sha.clone(),
                                            message.to_string(),
                                            remote_url.as_deref(),
                                            Some(provider_registry),
                                        )),
                                    };
                                    cx.new(|cx| {
                                        CommitTooltip::new(
                                            commit_details,
                                            repository.clone(),
                                            workspace.clone(),
                                            cx,
                                        )
                                    })
                                    .into()
                                })
                            } else {
                                this
                            }
                        })
                        .child(
                            h_flex()
                                .gap_2()
                                .overflow_hidden()
                                .children((!commit.data.ref_names.is_empty()).then(|| {
                                    h_flex().gap_1().children(commit.data.ref_names.iter().map(
                                        |name| {
                                            let is_head =
                                                Self::is_head_ref(name.as_ref(), &head_branch_name);
                                            self.render_ref_chip(
                                                name,
                                                accent_color,
                                                is_head,
                                                idx,
                                                cx,
                                            )
                                        },
                                    ))
                                }))
                                .child(subject_label),
                        )
                        .into_any_element(),
                    column_label(formatted_time.into()),
                    column_label(author_name),
                    column_label(short_sha.into()),
                ]
            })
            .collect()
    }
}
