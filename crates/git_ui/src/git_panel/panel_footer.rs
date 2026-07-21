use super::*;

#[derive(IntoElement, RegisterComponent)]
pub struct PanelRepoFooter {
    active_repository: SharedString,
    branch: Option<Branch>,
    head_commit: Option<CommitDetails>,

    // Getting a GitPanel in previews will be difficult.
    //
    // For now just take an option here, and we won't bind handlers to buttons in previews.
    git_panel: Option<Entity<GitPanel>>,
}

impl PanelRepoFooter {
    pub fn new(
        active_repository: SharedString,
        branch: Option<Branch>,
        head_commit: Option<CommitDetails>,
        git_panel: Option<Entity<GitPanel>>,
    ) -> Self {
        Self {
            active_repository,
            branch,
            head_commit,
            git_panel,
        }
    }

    pub fn new_preview(active_repository: SharedString, branch: Option<Branch>) -> Self {
        Self {
            active_repository,
            branch,
            head_commit: None,
            git_panel: None,
        }
    }
}

impl RenderOnce for PanelRepoFooter {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let project = self
            .git_panel
            .as_ref()
            .map(|panel| panel.read(cx).project.clone());

        let (workspace, repo) = self
            .git_panel
            .as_ref()
            .map(|panel| {
                let panel = panel.read(cx);
                (panel.workspace.clone(), panel.active_repository.clone())
            })
            .unzip();

        let single_repo = project
            .as_ref()
            .map(|project| project.read(cx).git_store().read(cx).repositories().len() == 1)
            .unwrap_or(true);

        const MAX_SHORT_SHA_LEN: usize = 8;
        let branch_name = self
            .branch
            .as_ref()
            .map(|branch| branch.name().to_owned())
            .or_else(|| {
                self.head_commit.as_ref().map(|commit| {
                    commit
                        .sha
                        .chars()
                        .take(MAX_SHORT_SHA_LEN)
                        .collect::<String>()
                })
            })
            .unwrap_or_else(|| " (no branch)".to_owned());
        let show_separator = self.branch.is_some() || self.head_commit.is_some();

        let active_repo_name = self.active_repository.clone();

        let repo_selector = PopoverMenu::new("repository-switcher")
            .menu({
                let project = project;
                move |window, cx| {
                    let project = project.clone()?;
                    Some(cx.new(|cx| RepositorySelector::new(project, rems(20.), window, cx)))
                }
            })
            .trigger_with_tooltip(
                Button::new("repo-selector", active_repo_name)
                    .size(ButtonSize::None)
                    .label_size(LabelSize::Small)
                    .truncate(true),
                move |_, cx| {
                    if single_repo {
                        cx.new(|_| Empty).into()
                    } else {
                        Tooltip::simple("Switch Active Repository", cx)
                    }
                },
            )
            .anchor(Anchor::BottomLeft)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            })
            .into_any_element();

        let branch_selector_button = Button::new("branch-selector", branch_name)
            .size(ButtonSize::None)
            .label_size(LabelSize::Small)
            .truncate(true)
            .on_click(|_, window, cx| {
                window.dispatch_action(mav_actions::git::Switch.boxed_clone(), cx);
            });

        let branch_selector = PopoverMenu::new("popover-button")
            .menu(move |window, cx| {
                let workspace = workspace.clone()?;
                let repo = repo.clone().flatten();
                Some(branch_picker::popover(workspace, false, repo, window, cx))
            })
            .trigger_with_tooltip(
                branch_selector_button,
                Tooltip::for_action_title("Switch Branch", &mav_actions::git::Switch),
            )
            .anchor(Anchor::BottomLeft)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            });

        h_flex()
            .h_9()
            .w_full()
            .px_2()
            .justify_between()
            .gap_1()
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .gap_px()
                    .child(Icon::new(IconName::GitBranch).size(IconSize::Small).color(
                        if single_repo {
                            Color::Disabled
                        } else {
                            Color::Muted
                        },
                    ))
                    .when(!single_repo, |this| {
                        this.child(div().child(repo_selector).min_w_0()).when(
                            show_separator,
                            |this| {
                                this.child(Label::new("/").size(LabelSize::Small).color(
                                    Color::Custom(cx.theme().colors().text_muted.opacity(0.4)),
                                ))
                            },
                        )
                    })
                    .child(div().child(branch_selector).min_w_0()),
            )
            .children(if let Some(git_panel) = self.git_panel {
                git_panel.update(cx, |git_panel, cx| git_panel.render_remote_button(cx))
            } else {
                None
            })
    }
}

impl Component for PanelRepoFooter {
    fn scope() -> ComponentScope {
        ComponentScope::VersionControl
    }

    fn description() -> &'static str {
        "The footer shown at the bottom of the git panel."
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> AnyElement {
        let unknown_upstream = None;
        let no_remote_upstream = Some(UpstreamTracking::Gone);
        let ahead_of_upstream = Some(
            UpstreamTrackingStatus {
                ahead: 2,
                behind: 0,
            }
            .into(),
        );
        let behind_upstream = Some(
            UpstreamTrackingStatus {
                ahead: 0,
                behind: 2,
            }
            .into(),
        );
        let ahead_and_behind_upstream = Some(
            UpstreamTrackingStatus {
                ahead: 3,
                behind: 1,
            }
            .into(),
        );

        let not_ahead_or_behind_upstream = Some(
            UpstreamTrackingStatus {
                ahead: 0,
                behind: 0,
            }
            .into(),
        );

        fn branch(upstream: Option<UpstreamTracking>) -> Branch {
            Branch {
                is_head: true,
                ref_name: "some-branch".into(),
                upstream: upstream.map(|tracking| Upstream {
                    ref_name: "origin/some-branch".into(),
                    tracking,
                }),
                most_recent_commit: Some(CommitSummary {
                    sha: "abc123".into(),
                    subject: "Modify stuff".into(),
                    commit_timestamp: 1710932954,
                    author_name: "John Doe".into(),
                    has_parent: true,
                }),
            }
        }

        fn custom(branch_name: &str, upstream: Option<UpstreamTracking>) -> Branch {
            Branch {
                is_head: true,
                ref_name: branch_name.to_string().into(),
                upstream: upstream.map(|tracking| Upstream {
                    ref_name: format!("mav/{}", branch_name).into(),
                    tracking,
                }),
                most_recent_commit: Some(CommitSummary {
                    sha: "abc123".into(),
                    subject: "Modify stuff".into(),
                    commit_timestamp: 1710932954,
                    author_name: "John Doe".into(),
                    has_parent: true,
                }),
            }
        }

        fn active_repository(id: usize) -> SharedString {
            format!("repo-{}", id).into()
        }

        let example_width = px(340.);

        v_flex()
            .gap_6()
            .w_full()
            .flex_none()
            .children(vec![
                example_group_with_title(
                    "Action Button States",
                    vec![
                        single_example(
                            "No Branch",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(active_repository(1), None))
                                .into_any_element(),
                        ),
                        single_example(
                            "Remote status unknown",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(2),
                                    Some(branch(unknown_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "No Remote Upstream",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(3),
                                    Some(branch(no_remote_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Not Ahead or Behind",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(4),
                                    Some(branch(not_ahead_or_behind_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Behind remote",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(5),
                                    Some(branch(behind_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Ahead of remote",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(6),
                                    Some(branch(ahead_of_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Ahead and behind remote",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    active_repository(7),
                                    Some(branch(ahead_and_behind_upstream)),
                                ))
                                .into_any_element(),
                        ),
                    ],
                )
                .grow()
                .vertical(),
            ])
            .children(vec![
                example_group_with_title(
                    "Labels",
                    vec![
                        single_example(
                            "Short Branch & Repo",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("mav"),
                                    Some(custom("main", behind_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Long Branch",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("mav"),
                                    Some(custom(
                                        "redesign-and-update-git-ui-list-entry-style",
                                        behind_upstream,
                                    )),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Long Repo",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("mav-industries-community-examples"),
                                    Some(custom("gpui", ahead_of_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Long Repo & Branch",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("mav-industries-community-examples"),
                                    Some(custom(
                                        "redesign-and-update-git-ui-list-entry-style",
                                        behind_upstream,
                                    )),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Uppercase Repo",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("LICENSES"),
                                    Some(custom("main", ahead_of_upstream)),
                                ))
                                .into_any_element(),
                        ),
                        single_example(
                            "Uppercase Branch",
                            div()
                                .w(example_width)
                                .overflow_hidden()
                                .child(PanelRepoFooter::new_preview(
                                    SharedString::from("mav"),
                                    Some(custom("update-README", behind_upstream)),
                                ))
                                .into_any_element(),
                        ),
                    ],
                )
                .grow()
                .vertical(),
            ])
            .into_any_element()
    }
}
