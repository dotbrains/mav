use super::*;

impl ThreadView {
    pub(super) fn render_codex_windows_warning(&self, cx: &mut Context<Self>) -> Callout {
        Callout::new()
            .border_position(self.callout_border_position())
            .icon(IconName::Warning)
            .severity(Severity::Warning)
            .title("Codex on Windows")
            .description("For best performance, run Codex in Windows Subsystem for Linux (WSL2)")
            .actions_slot(
                Button::new("open-wsl-modal", "Open in WSL").on_click(cx.listener({
                    move |_, _, _window, cx| {
                        #[cfg(windows)]
                        _window.dispatch_action(
                            mav_actions::wsl_actions::OpenWsl::default().boxed_clone(),
                            cx,
                        );
                        cx.notify();
                    }
                })),
            )
            .dismiss_action(
                IconButton::new("dismiss", IconName::Close)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .tooltip(Tooltip::text("Dismiss Warning"))
                    .on_click(cx.listener({
                        move |this, _, _, cx| {
                            this.show_codex_windows_warning = false;
                            cx.notify();
                        }
                    })),
            )
    }

    pub(super) fn render_skill_loading_issues(&self, cx: &mut Context<Self>) -> Vec<Callout> {
        let border_position = self.callout_border_position();

        let description_warnings = self
            .skill_loading_issues
            .iter()
            .filter(|issue| issue.kind == SkillLoadingIssueKind::DescriptionTooLong)
            .cloned()
            .collect::<Vec<_>>();

        let long_description_warning =
            self.render_skill_description_warnings(description_warnings, cx);

        let other_warnings = self
            .skill_loading_issues
            .iter()
            .filter(|issue| issue.kind != SkillLoadingIssueKind::DescriptionTooLong)
            .enumerate()
            .map(|(index, issue)| {
                let abs_path = issue.path.clone();
                let workspace = self.workspace.clone();
                let path_label = issue.path.display().to_string();
                let target = issue.clone();

                let title = match issue.kind {
                    SkillLoadingIssueKind::LoadFailed => "Skill Failed to Load",
                    SkillLoadingIssueKind::DescriptionTooLong => unreachable!(),
                    SkillLoadingIssueKind::CatalogBudgetExceeded => {
                        "Skill Omitted from Model Catalog"
                    }
                };

                Callout::new()
                    .icon(IconName::Warning)
                    .severity(Severity::Warning)
                    .title(title)
                    .description(format!("{}\n{path_label}", issue.message))
                    .actions_slot(
                        Button::new(("open-skill-file", index), "Open Skill")
                            .style(ButtonStyle::Outlined)
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |_, _, window, cx| {
                                let abs_path = abs_path.clone();
                                workspace
                                    .update(cx, |workspace, cx| {
                                        workspace
                                            .open_abs_path(
                                                abs_path,
                                                workspace::OpenOptions::default(),
                                                window,
                                                cx,
                                            )
                                            .detach_and_log_err(cx);
                                    })
                                    .ok();
                            })),
                    )
                    .dismiss_action(
                        IconButton::new(("dismiss-skill-issue", index), IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Dismiss"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.skill_loading_issues.retain(|issue| *issue != target);
                                this.dismissed_skill_loading_issues.insert(target.clone());
                                cx.notify();
                            })),
                    )
            })
            .collect::<Vec<_>>();

        long_description_warning
            .into_iter()
            .chain(other_warnings)
            .map(|callout| callout.border_position(border_position))
            .collect()
    }

    fn render_skill_description_warnings(
        &self,
        description_warnings: Vec<SkillLoadingIssue>,
        cx: &mut Context<Self>,
    ) -> Option<Callout> {
        if description_warnings.is_empty() {
            return None;
        }

        let warning_count = description_warnings.len();
        let title = if warning_count == 1 {
            "1 Skill Loaded with a Long Description".to_string()
        } else {
            format!("{warning_count} Skills Loaded with Long Descriptions")
        };

        let rows = description_warnings
            .iter()
            .enumerate()
            .map(|(index, issue)| {
                let abs_path = issue.path.clone();
                let workspace = self.workspace.clone();
                let full_path = issue.path.display().to_string();
                let file_label = skill_issue_file_label(&issue.path);

                ButtonLike::new(("skill-description-warning-file", index))
                    .full_width()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1()
                            .child(
                                Icon::new(IconName::Dash)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(file_label).size(LabelSize::Small)),
                    )
                    .tooltip(move |_, cx| {
                        Tooltip::with_meta("Open Skill", None, full_path.clone(), cx)
                    })
                    .on_click(cx.listener(move |_, _, window, cx| {
                        let abs_path = abs_path.clone();
                        workspace
                            .update(cx, |workspace, cx| {
                                workspace
                                    .open_abs_path(
                                        abs_path,
                                        workspace::OpenOptions::default(),
                                        window,
                                        cx,
                                    )
                                    .detach_and_log_err(cx);
                            })
                            .ok();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let callout = Callout::new()
            .icon(IconName::Warning)
            .severity(Severity::Warning)
            .title(title)
            .description_slot(
                v_flex()
                    .gap_1()
                    .child(
                        Label::new(format!(
                            "Ensure skill descriptions are at most {MAX_SKILL_DESCRIPTION_LEN} bytes; longer ones may consume more model-context tokens."
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    )
                    .children(rows),
            );

        let targets = description_warnings;

        Some(
            callout.dismiss_action(
                IconButton::new("dismiss-skill-description-warnings", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Dismiss"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.skill_loading_issues
                            .retain(|issue| !targets.contains(issue));
                        for target in &targets {
                            this.dismissed_skill_loading_issues.insert(target.clone());
                        }
                        cx.notify();
                    })),
            ),
        )
    }

    pub(super) fn render_external_source_prompt_warning(&self, cx: &mut Context<Self>) -> Callout {
        Callout::new()
            .border_position(self.callout_border_position())
            .icon(IconName::Warning)
            .severity(Severity::Warning)
            .title("Review Before Sending")
            .description("This prompt was pre-filled by an external link. Read it carefully before you submit it to the model.")
            .dismiss_action(
                IconButton::new("dismiss-external-source-prompt-warning", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Dismiss Warning"))
                    .on_click(cx.listener({
                        move |this, _, _, cx| {
                            this.show_external_source_prompt_warning = false;
                            cx.notify();
                        }
                    })),
            )
    }

    pub(super) fn render_multi_root_callout(&self, cx: &mut Context<Self>) -> Option<Callout> {
        if self.multi_root_callout_dismissed {
            return None;
        }

        if self.as_native_connection(cx).is_some() {
            return None;
        }

        if self
            .thread
            .read(cx)
            .connection()
            .supports_session_additional_directories()
        {
            return None;
        }

        let project = self.project.upgrade()?;
        let worktree_count = project.read(cx).visible_worktrees(cx).count();
        if worktree_count <= 1 {
            return None;
        }

        let work_dirs = self.thread.read(cx).work_dirs()?;
        let active_dir = work_dirs
            .ordered_paths()
            .next()
            .and_then(|p| p.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "one folder".to_string());

        Some(
            Callout::new()
                .severity(Severity::Warning)
                .icon(IconName::Warning)
                .title("This agent doesn't currently support multi-root workspaces")
                .description(format!(
                    "It currently only operates by default on \"{}\".",
                    active_dir
                ))
                .border_position(self.callout_border_position())
                .dismiss_action(
                    IconButton::new("dismiss-multi-root-callout", IconName::Close)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Dismiss"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.multi_root_callout_dismissed = true;
                            cx.notify();
                        })),
                ),
        )
    }

    pub(super) fn render_new_version_callout(
        &self,
        version: &SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let server_view = self.server_view.clone();
        let has_version = !version.is_empty();
        let title = if has_version {
            "New Version Available"
        } else {
            "Agent Update Available"
        };
        let button_label = if has_version {
            format!("Update to v{}", version)
        } else {
            "Reconnect".to_string()
        };

        v_flex().w_full().justify_end().child(
            h_flex()
                .p_2()
                .pr_3()
                .w_full()
                .gap_1p5()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().element_background)
                .child(
                    h_flex()
                        .flex_1()
                        .gap_1p5()
                        .child(
                            Icon::new(IconName::Download)
                                .color(Color::Accent)
                                .size(IconSize::Small),
                        )
                        .child(Label::new(title).size(LabelSize::Small)),
                )
                .child(
                    Button::new("update-button", button_label)
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::Tinted(TintColor::Accent))
                        .on_click(move |_, window, cx| {
                            server_view
                                .update(cx, |view, cx| view.reset(window, cx))
                                .ok();
                        }),
                ),
        )
    }

    pub(super) fn render_token_limit_callout(&self, cx: &mut Context<Self>) -> Option<Callout> {
        if self.token_limit_callout_dismissed || self.as_native_thread(cx).is_none() {
            return None;
        }

        let token_usage = self.thread.read(cx).token_usage()?;

        if token_usage.max_tokens >= agent::MIN_COMPACTION_CONTEXT_WINDOW {
            return None;
        }

        let ratio = token_usage.ratio();

        let (severity, icon, title) = match ratio {
            acp_thread::TokenUsageRatio::Normal => return None,
            acp_thread::TokenUsageRatio::Warning => (
                Severity::Warning,
                IconName::Warning,
                "Thread reaching the token limit soon",
            ),
            acp_thread::TokenUsageRatio::Exceeded => (
                Severity::Error,
                IconName::XCircle,
                "Thread reached the token limit",
            ),
        };

        let description = "To continue, run /compact or start a new thread and @-mention this one";

        Some(
            Callout::new()
                .border_position(self.callout_border_position())
                .severity(severity)
                .icon(icon)
                .title(title)
                .description(description)
                .actions_slot(
                    h_flex().gap_0p5().child(
                        Button::new("start-new-thread", "Start New Thread")
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let session_id = this.thread.read(cx).session_id().clone();
                                window.dispatch_action(
                                    crate::NewNativeAgentThreadFromSummary {
                                        from_session_id: session_id,
                                    }
                                    .boxed_clone(),
                                    cx,
                                );
                            })),
                    ),
                )
                .dismiss_action(self.dismiss_error_button(cx)),
        )
    }
}
