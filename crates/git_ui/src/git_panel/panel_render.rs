use super::*;

impl GitPanel {
    pub(crate) fn render_remote_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let branch = self.active_repository.as_ref()?.read(cx).branch.clone();
        if !self.can_push_and_pull(cx) {
            return None;
        }
        Some(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .when_some(branch, |this, branch| {
                    let focus_handle = Some(self.focus_handle(cx));

                    this.children(render_remote_button(
                        "remote-button",
                        &branch,
                        focus_handle,
                        true,
                        self.pending_remote_operation,
                    ))
                })
                .into_any_element(),
        )
    }

    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab = self.active_tab;

        let focus_handle = self.focus_handle.clone();
        let tab = |id: ElementId,
                   active: bool,
                   show_changes: bool,
                   label: SharedString,
                   set_active_tab: GitPanelTab,
                   tooltip_action: Box<dyn Action>| {
            let focus_handle = focus_handle.clone();

            h_flex()
                .cursor_pointer()
                .id(id)
                .h_full()
                .py_1()
                .gap_1()
                .flex_1()
                .justify_center()
                .hover(|s| s.bg(cx.theme().colors().element_hover))
                .border_b_1()
                .when(!active, |s| {
                    s.bg(cx.theme().colors().editor_background.opacity(0.6))
                        .border_color(cx.theme().colors().border.opacity(0.6))
                })
                .child(Label::new(label.clone()).when(!active, |this| this.color(Color::Muted)))
                .when(show_changes && self.changes_count > 0, |this| {
                    this.child(
                        Label::new(format!("({})", self.changes_count))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                })
                .tooltip(Tooltip::for_action_title_in(
                    format!("Toggle {} Tab", label),
                    tooltip_action.as_ref(),
                    &focus_handle,
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_active_tab(set_active_tab, window, cx)
                }))
        };

        h_flex()
            .relative()
            .h(Tab::container_height(cx))
            .w_full()
            .child(tab(
                ElementId::Name("changes-tab".into()),
                active_tab == GitPanelTab::Changes,
                true,
                "Changes".into(),
                GitPanelTab::Changes,
                ActivateChangesTab.boxed_clone(),
            ))
            .child(Divider::vertical().color(ui::DividerColor::BorderFaded))
            .child(tab(
                ElementId::Name("history-tab".into()),
                active_tab != GitPanelTab::Changes,
                false,
                "History".into(),
                GitPanelTab::History,
                ActivateHistoryTab.boxed_clone(),
            ))
    }

    pub(super) fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (self.git_access, &self.active_repository) {
            (GitAccess::No, Some(repository)) => self.render_unsafe_repo_ui(repository, cx),
            (_, None) => self.render_uninitialized_ui(cx),
            (_, Some(_)) => self.render_no_changes_ui(cx),
        };

        v_flex()
            .gap_1p5()
            .flex_1()
            .items_center()
            .justify_center()
            .child(content)
    }

    pub(super) fn render_no_changes_ui(&self, cx: &Context<Self>) -> AnyElement {
        let show_branch_diff = self.changes_count == 0 && !self.is_on_main_branch(cx);

        v_flex()
            .gap_1()
            .items_center()
            .child(Label::new("No changes to commit").color(Color::Muted))
            .when(show_branch_diff, |this| {
                this.child(
                    Button::new("view_branch_diff", "View Branch Diff")
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::Outlined)
                        .on_click(move |_, _, cx| {
                            cx.defer(move |cx| {
                                cx.dispatch_action(&BranchDiff);
                            })
                        }),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_unsafe_repo_ui(
        &self,
        active_repository: &Entity<Repository>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let directory = active_repository.update(cx, |repository, _cx| {
            repository.snapshot().work_directory_abs_path
        });

        let message = format!(
            "Detected dubious ownership in repository at {}. \
            This happens when the .git/ directory is not owned by the current user. \
            If you want to learn more about safe directories, visit git's documentation.",
            directory.display()
        );

        v_flex()
                .px_4()
                .gap_1()
                .child(Label::new(message).color(Color::Muted))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            Button::new("trust_directory", "Trust Directory")
                            .label_size(LabelSize::Small)
                            .layer(ElevationIndex::ModalSurface)
                            .style(ButtonStyle::Filled)
                            .tooltip(Tooltip::text(
                                format!("git config --global --add safe.directory {}", directory.display())
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.add_safe_directory(window, cx);
                                })
                            )
                    )
                    .child(
                        Button::new("learn_more", "Learn More")
                            .label_size(LabelSize::Small)
                            .style(ButtonStyle::Outlined)
                            .end_icon(Icon::new(IconName::ArrowUpRight).size(IconSize::Small).color(Color::Muted))
                            .on_click(move |_, _, cx| cx.open_url("https://git-scm.com/docs/git-config#Documentation/git-config.txt-safedirectory"))
                    )
                )
                .into_any_element()
    }

    pub(super) fn render_uninitialized_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let worktree_count = self.project.read(cx).visible_worktrees(cx).count();
        if worktree_count > 0 && self.active_repository.is_none() {
            v_flex()
                .gap_1()
                .items_center()
                .child(Label::new("No Git Repositories").color(Color::Muted))
                .child(
                    Button::new("initialize_repository", "Initialize Repository")
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::Outlined)
                        .tooltip(Tooltip::for_action_title_in(
                            "git init",
                            &git::Init,
                            &self.focus_handle,
                        ))
                        .on_click(move |_, _, cx| {
                            cx.defer(move |cx| {
                                cx.dispatch_action(&git::Init);
                            })
                        }),
                )
                .into_any_element()
        } else if worktree_count == 0 {
            let focus_handle = self.focus_handle.clone();
            ProjectEmptyState::new(
                "Git Panel",
                focus_handle.clone(),
                KeyBinding::for_action_in(&workspace::Open::default(), &focus_handle, cx),
            )
            .on_open_project(|_, window, cx| {
                telemetry::event!("Git Panel Add Project Clicked");
                window.dispatch_action(workspace::Open::default().boxed_clone(), cx);
            })
            .on_clone_repo(|_, window, cx| {
                telemetry::event!("Git Panel Clone Repo Clicked");
                window.dispatch_action(git::Clone.boxed_clone(), cx);
            })
            .into_any_element()
        } else {
            Empty.into_any_element()
        }
    }

    pub(super) fn is_on_main_branch(&self, cx: &Context<Self>) -> bool {
        let Some(repo) = self.active_repository.as_ref() else {
            return false;
        };

        let Some(branch) = repo.read(cx).branch.as_ref() else {
            return false;
        };

        let branch_name = branch.name();
        matches!(branch_name, "main" | "master")
    }

    pub(super) fn render_buffer_header_controls(
        &self,
        entity: &Entity<Self>,
        file: &Arc<dyn File>,
        _: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let repo = self.active_repository.as_ref()?.read(cx);
        let project_path = (file.worktree_id(cx), file.path().clone()).into();
        let repo_path = repo.project_path_to_repo_path(&project_path, cx)?;
        let ix = self.entry_by_path(&repo_path)?;
        let entry = self.entries.get(ix)?;

        let is_staging_or_staged = repo
            .pending_ops_for_path(&repo_path)
            .map(|ops| !ops.last_op_errored() && (ops.staging() || ops.staged()))
            .or_else(|| {
                repo.status_for_path(&repo_path)
                    .and_then(|status| status.status.staging().as_bool())
            })
            .or_else(|| {
                entry
                    .status_entry()
                    .and_then(|entry| entry.staging.as_bool())
            });

        let checkbox = Checkbox::new("stage-file", is_staging_or_staged.into())
            .disabled(!self.has_write_access(cx))
            .fill()
            .elevation(ElevationIndex::Surface)
            .on_click({
                let entry = entry.clone();
                let git_panel = entity.downgrade();
                move |_, window, cx| {
                    git_panel
                        .update(cx, |this, cx| {
                            this.toggle_staged_for_entry(&entry, window, cx);
                            cx.stop_propagation();
                        })
                        .ok();
                }
            });
        Some(
            h_flex()
                .id("start-slot")
                .text_lg()
                .child(checkbox)
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    // prevent the list item active state triggering when toggling checkbox
                    cx.stop_propagation();
                })
                .into_any_element(),
        )
    }
}
