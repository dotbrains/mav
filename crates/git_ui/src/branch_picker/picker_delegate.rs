use super::*;

impl PickerDelegate for BranchListDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "branch picker"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        match self.state {
            PickerState::List | PickerState::NewRemote | PickerState::NewBranch => {
                "Switch or type to create a branch…"
            }
            PickerState::CreateRemote(_) => "Enter a name for this remote…",
        }
        .into()
    }

    fn no_matches_text(&self, _window: &mut Window, _cx: &mut App) -> Option<SharedString> {
        match self.state {
            PickerState::CreateRemote(_) => {
                Some(SharedString::new_static("Remote name can't be empty"))
            }
            _ => None,
        }
    }

    fn render_editor(
        &self,
        editor: &Arc<dyn ErasedEditor>,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Div {
        let focus_handle = self.focus_handle.clone();
        let editor = editor.as_any().downcast_ref::<Entity<Editor>>().unwrap();

        let show_inline_filter =
            self.editor_position() == PickerEditorPosition::End || !self.show_footer;

        v_flex()
            .when(
                self.editor_position() == PickerEditorPosition::End,
                |this| this.child(Divider::horizontal()),
            )
            .when_some(self.branch_list_error.clone(), |this, error| {
                let message = format!("Some branches could not be loaded: {error}");
                this.child(
                    div()
                        .id("branch-list-error")
                        .p_1p5()
                        .child(
                            Banner::new().severity(Severity::Warning).child(
                                Label::new(message.clone())
                                    .size(LabelSize::Small)
                                    .single_line()
                                    .truncate(),
                            ),
                        )
                        .tooltip(Tooltip::text(message)),
                )
            })
            .child(
                h_flex()
                    .overflow_hidden()
                    .flex_none()
                    .h_9()
                    .px_2p5()
                    .child(editor.clone())
                    .when(show_inline_filter, |this| {
                        let tooltip_label = match self.branch_filter {
                            BranchFilter::All => "Filter Remote Branches",
                            BranchFilter::Remote => "Show All Branches",
                        };

                        this.gap_1().justify_between().child({
                            IconButton::new("filter-remotes", IconName::Filter)
                                .toggle_state(self.branch_filter == BranchFilter::Remote)
                                .icon_size(IconSize::Small)
                                .tooltip(move |_, cx| {
                                    Tooltip::for_action_in(
                                        tooltip_label,
                                        &branch_picker::FilterRemotes,
                                        &focus_handle,
                                        cx,
                                    )
                                })
                                .on_click(|_click, window, cx| {
                                    window.dispatch_action(
                                        branch_picker::FilterRemotes.boxed_clone(),
                                        cx,
                                    );
                                })
                        })
                    }),
            )
            .when(
                self.editor_position() == PickerEditorPosition::Start,
                |this| this.child(Divider::horizontal()),
            )
    }

    fn editor_position(&self) -> PickerEditorPosition {
        if self.is_select_only() {
            return PickerEditorPosition::Start;
        }

        match self.style {
            BranchListStyle::Modal => PickerEditorPosition::Start,
            BranchListStyle::Popover => PickerEditorPosition::End,
        }
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let all_branches = self.all_branches.clone();
        let branch_selection_context = self.is_select_only().then(|| {
            BranchSelectionContext::new(
                self.branch_selection_behavior.selected_branch().cloned(),
                self.repo.as_ref(),
                cx,
            )
        });

        let branch_filter = self.branch_filter;
        cx.spawn_in(window, async move |picker, cx| {
            let branch_matches_filter = |branch: &Branch| match branch_filter {
                BranchFilter::All => true,
                BranchFilter::Remote => branch.is_remote(),
            };

            let mut matches: Vec<Entry> = if query.is_empty() {
                let mut matches: Vec<Entry> = all_branches
                    .into_iter()
                    .filter(|branch| branch_matches_filter(branch))
                    .map(|branch| Entry::Branch {
                        branch,
                        positions: Vec::new(),
                    })
                    .collect();

                sort_branch_entries(&mut matches, branch_selection_context.as_ref());

                matches
            } else {
                let branches = all_branches
                    .iter()
                    .filter(|branch| branch_matches_filter(branch))
                    .collect::<Vec<_>>();
                let candidates = branches
                    .iter()
                    .enumerate()
                    .map(|(ix, branch)| StringMatchCandidate::new(ix, branch.name()))
                    .collect::<Vec<StringMatchCandidate>>();
                let mut matches: Vec<Entry> = fuzzy_nucleo::match_strings_async(
                    &candidates,
                    &query,
                    fuzzy_nucleo::Case::Smart,
                    fuzzy_nucleo::LengthPenalty::On,
                    10000,
                    &Default::default(),
                    cx.background_executor().clone(),
                )
                .await
                .into_iter()
                .map(|candidate| Entry::Branch {
                    branch: branches[candidate.candidate_id].clone(),
                    positions: candidate.positions,
                })
                .collect();

                sort_branch_entries(&mut matches, branch_selection_context.as_ref());

                matches
            };
            picker
                .update(cx, |picker, _| {
                    if let PickerState::CreateRemote(url) = &picker.delegate.state {
                        let query = normalize_branch_name(&query);
                        if !query.is_empty() {
                            picker.delegate.matches = vec![Entry::NewRemoteName {
                                name: query.clone(),
                                url: url.clone(),
                            }];
                            picker.delegate.selected_index = 0;
                        } else {
                            picker.delegate.matches = Vec::new();
                            picker.delegate.selected_index = 0;
                        }
                        picker.delegate.last_query = query;
                        return;
                    }

                    if !picker.delegate.is_select_only()
                        && !query.is_empty()
                        && !matches.first().is_some_and(|entry| entry.name() == query)
                    {
                        let query = normalize_branch_name(&query);
                        let is_url = query.trim_start_matches("git@").parse::<Url>().is_ok();
                        let entry = if is_url {
                            Entry::NewUrl { url: query }
                        } else {
                            Entry::NewBranch { name: query }
                        };
                        // Only transition to NewBranch/NewRemote states when we only show their list item
                        // Otherwise, stay in List state so footer buttons remain visible
                        picker.delegate.state = if matches.is_empty() {
                            if is_url {
                                PickerState::NewRemote
                            } else {
                                PickerState::NewBranch
                            }
                        } else {
                            PickerState::List
                        };
                        matches.push(entry);
                    } else {
                        picker.delegate.state = PickerState::List;
                    }
                    let delegate = &mut picker.delegate;
                    delegate.matches = matches;
                    if delegate.matches.is_empty() {
                        delegate.selected_index = 0;
                    } else if let Some(ref_name) = delegate.restore_selected_branch.take() {
                        delegate.selected_index = delegate
                            .matches
                            .iter()
                            .position(|entry| {
                                entry.as_branch().is_some_and(|branch| {
                                    branch.ref_name == ref_name
                                        || branch.name() == ref_name.as_ref()
                                })
                            })
                            .unwrap_or(0);
                    } else {
                        delegate.selected_index =
                            core::cmp::min(delegate.selected_index, delegate.matches.len() - 1);
                    }
                    delegate.last_query = query;
                })
                .log_err();
        })
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(entry) = self.matches.get(self.selected_index()) else {
            return;
        };

        match entry {
            Entry::Branch { branch, .. } => {
                if let BranchSelectionBehavior::Select { on_select, .. } =
                    &self.branch_selection_behavior
                {
                    on_select(branch.clone(), window, cx);
                    cx.emit(DismissEvent);
                    return;
                }

                let current_branch = self.repo.as_ref().map(|repo| {
                    repo.read_with(cx, |repo, _| {
                        repo.branch.as_ref().map(|branch| branch.ref_name.clone())
                    })
                });

                if current_branch
                    .flatten()
                    .is_some_and(|current_branch| current_branch == branch.ref_name)
                {
                    cx.emit(DismissEvent);
                    return;
                }

                let Some(repo) = self.repo.clone() else {
                    return;
                };

                let branch = branch.clone();
                cx.spawn(async move |_, cx| {
                    repo.update(cx, |repo, _| repo.change_branch(branch.name().to_string()))
                        .await??;

                    anyhow::Ok(())
                })
                .detach_and_prompt_err(
                    "Failed to change branch",
                    window,
                    cx,
                    |_, _, _| None,
                );
            }
            Entry::NewUrl { url } => {
                self.state = PickerState::CreateRemote(url.clone().into());
                self.matches = Vec::new();
                self.selected_index = 0;

                cx.defer_in(window, |picker, window, cx| {
                    picker.refresh_placeholder(window, cx);
                    picker.set_query("", window, cx);
                    cx.notify();
                });

                // returning early to prevent dismissing the modal, so a user can enter
                // a remote name first.
                return;
            }
            Entry::NewRemoteName { name, url } => {
                self.create_remote(name.clone(), url.to_string(), window, cx);
            }
            Entry::NewBranch { name } => {
                let from_branch = if secondary {
                    self.default_branch.clone()
                } else {
                    None
                };
                self.create_branch(from_branch, name.into(), window, cx);
            }
        }

        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.state = PickerState::List;
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        self.render_branch_match(ix, selected, cx)
    }

    fn render_footer(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        self.render_branch_footer(cx)
    }
}
