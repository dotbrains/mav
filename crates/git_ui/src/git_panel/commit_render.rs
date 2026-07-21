use super::*;

impl GitPanel {
    pub(super) fn render_commit_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (can_commit, tooltip) = self.configure_commit_button(cx);
        let title = self.commit_button_title();
        let commit_tooltip_focus_handle = self.commit_editor.focus_handle(cx);
        let amend = self.amend_pending();
        let signoff = self.signoff_enabled;

        let label_color = if self.pending_commit.is_some() {
            Color::Disabled
        } else {
            Color::Default
        };

        div()
            .id("commit-wrapper")
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.show_placeholders =
                    *hovered && !this.has_staged_changes() && !this.has_unstaged_conflicts();
                cx.notify()
            }))
            .child(SplitButton::new(
                ButtonLike::new_rounded_left(ElementId::Name(
                    format!("split-button-left-{}", title).into(),
                ))
                .layer(ElevationIndex::ModalSurface)
                .size(ButtonSize::Compact)
                .child(
                    Label::new(title)
                        .size(LabelSize::Small)
                        .color(label_color)
                        .mr_0p5(),
                )
                .on_click({
                    let git_panel = cx.weak_entity();
                    move |_, window, cx| {
                        telemetry::event!("Git Committed", source = "Git Panel");
                        git_panel
                            .update(cx, |git_panel, cx| {
                                git_panel.commit_changes(
                                    CommitOptions {
                                        amend,
                                        signoff,
                                        allow_empty: false,
                                    },
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                    }
                })
                .disabled(!can_commit || self.modal_open)
                .tooltip({
                    let handle = commit_tooltip_focus_handle.clone();
                    move |_window, cx| {
                        if can_commit {
                            Tooltip::with_meta_in(
                                tooltip,
                                Some(&git::Commit),
                                format!(
                                    "git commit{}{}",
                                    if amend { " --amend" } else { "" },
                                    if signoff { " --signoff" } else { "" }
                                ),
                                &handle.clone(),
                                cx,
                            )
                        } else {
                            Tooltip::simple(tooltip, cx)
                        }
                    }
                }),
                self.render_git_commit_menu(
                    ElementId::Name(format!("split-button-right-{}", title).into()),
                    Some(commit_tooltip_focus_handle),
                    cx,
                )
                .into_any_element(),
            ))
    }

    pub(super) fn render_pending_amend(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .py_1p5()
            .px_2()
            .gap_1p5()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().colors().border.opacity(0.8))
            .child(
                div()
                    .flex_grow_1()
                    .overflow_hidden()
                    .max_w(relative(0.85))
                    .child(
                        Label::new("This will update your most recent commit.")
                            .size(LabelSize::Small)
                            .truncate(),
                    ),
            )
            .child(
                Button::new("cancel", "Cancel")
                    .label_size(LabelSize::Small)
                    .layer(ElevationIndex::ModalSurface)
                    .on_click(cx.listener(|this, _, _, cx| this.set_amend_pending(false, cx))),
            )
    }

    pub(super) fn render_previous_commit(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let active_repository = self.active_repository.as_ref()?;
        let branch = active_repository.read(cx).branch.as_ref()?;
        let commit = branch.most_recent_commit.as_ref()?.clone();
        let workspace = self.workspace.clone();
        let this = cx.entity();

        Some(
            h_flex()
                .p_1p5()
                .gap_1p5()
                .justify_between()
                .border_t_1()
                .border_color(cx.theme().colors().border.opacity(0.8))
                .child(
                    div()
                        .id("commit-msg-hover")
                        .cursor_pointer()
                        .px_1()
                        .rounded_sm()
                        .line_clamp(1)
                        .hover(|s| s.bg(cx.theme().colors().element_hover))
                        .child(
                            Label::new(commit.subject.clone())
                                .size(LabelSize::Small)
                                .truncate(),
                        )
                        .on_click({
                            let commit = commit.clone();
                            let repo = active_repository.downgrade();
                            move |_, window, cx| {
                                CommitView::open(
                                    commit.sha.to_string(),
                                    repo.clone(),
                                    workspace.clone(),
                                    None,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                        })
                        .hoverable_tooltip({
                            let repo = active_repository.clone();
                            move |window, cx| {
                                GitPanelMessageTooltip::new(
                                    this.clone(),
                                    commit.sha.clone(),
                                    repo.clone(),
                                    window,
                                    cx,
                                )
                                .into()
                            }
                        }),
                )
                .child(
                    h_flex()
                        .gap_0p5()
                        .when(commit.has_parent, |this| {
                            let has_unstaged = self.has_unstaged_changes();
                            this.child(
                                IconButton::new("undo", IconName::Undo)
                                    .icon_size(IconSize::Small)
                                    .tooltip(move |_window, cx| {
                                        Tooltip::with_meta(
                                            "Uncommit",
                                            Some(&git::Uncommit),
                                            if has_unstaged {
                                                "git reset HEAD^ --soft"
                                            } else {
                                                "git reset HEAD^"
                                            },
                                            cx,
                                        )
                                    })
                                    .on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.uncommit(window, cx)
                                        }),
                                    ),
                            )
                        })
                        .child(
                            IconButton::new("git-graph-button", IconName::GitGraph)
                                .icon_size(IconSize::Small)
                                .tooltip(|_window, cx| {
                                    Tooltip::for_action(
                                        "Open Git Graph",
                                        &crate::git_graph::Open,
                                        cx,
                                    )
                                })
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(crate::git_graph::Open.boxed_clone(), cx)
                                }),
                        ),
                ),
        )
    }
}
