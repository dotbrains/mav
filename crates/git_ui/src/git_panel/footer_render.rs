use super::*;

impl GitPanel {
    pub(super) fn render_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let active_repository = self.active_repository.clone()?;
        let settings = ThemeSettings::get_global(cx);
        let panel_editor_style =
            git_commit_editor_style(settings.git_commit_buffer_font_size(cx), cx);
        let enable_coauthors = self.render_co_authors(cx);
        let editor_focus_handle = self.commit_editor.focus_handle(cx);
        let branch = active_repository.read(cx).branch.clone();
        let head_commit = active_repository.read(cx).head_commit.clone();

        let footer_size = px(32.);
        let gap = px(9.0);
        let max_height = panel_editor_style
            .text
            .line_height_in_pixels(window.rem_size())
            * MAX_PANEL_EDITOR_LINES
            + gap;

        let git_panel = cx.entity();
        let display_name = SharedString::from(Arc::from(
            active_repository
                .read(cx)
                .display_name()
                .trim_end_matches("/"),
        ));
        let editor_is_long = self.commit_editor.update(cx, |editor, cx| {
            editor.max_point(cx).row().0 >= MAX_PANEL_EDITOR_LINES as u32
        });

        let max_title_length = GitPanelSettings::get_global(cx).commit_title_max_length;
        let title_exceeds_limit = if max_title_length > 0 {
            self.commit_editor
                .read(cx)
                .text(cx)
                .lines()
                .next()
                .is_some_and(|title| commit_title_exceeds_limit(title, max_title_length))
        } else {
            false
        };

        let footer = v_flex()
            .when(self.commit_editor_expanded, |this| this.flex_1().min_h_0())
            .child(PanelRepoFooter::new(
                display_name,
                branch,
                head_commit,
                Some(git_panel),
            ))
            .when(title_exceeds_limit, |this| {
                this.child(
                    h_flex()
                        .px_2()
                        .py_1()
                        .gap_1()
                        .border_t_1()
                        .border_color(cx.theme().status().warning_border)
                        .bg(cx.theme().status().warning_background.opacity(0.5))
                        .child(
                            Icon::new(IconName::Warning)
                                .size(IconSize::XSmall)
                                .color(Color::Warning),
                        )
                        .child(
                            Label::new(format!(
                                "Commit message title exceeds {max_title_length}-character limit."
                            ))
                            .size(LabelSize::Small),
                        ),
                )
            })
            .child(
                panel_editor_container(window, cx)
                    .id("commit-editor-container")
                    .cursor_text()
                    .relative()
                    .w_full()
                    .when(self.commit_editor_expanded, |this| this.flex_1().min_h_0())
                    .when(!self.commit_editor_expanded, |this| {
                        this.h(max_height + footer_size)
                    })
                    .border_t_1()
                    .border_color(if title_exceeds_limit {
                        cx.theme().status().warning_border
                    } else {
                        cx.theme().colors().border
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        window.focus(&this.commit_editor.focus_handle(cx), cx);
                    }))
                    .child(
                        h_flex()
                            .id("commit-footer")
                            .border_t_1()
                            .when(editor_is_long, |el| {
                                el.border_color(cx.theme().colors().border_variant)
                            })
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w_full()
                            .px_2()
                            .h(footer_size)
                            .flex_none()
                            .justify_between()
                            .child(
                                self.render_generate_commit_message_button(cx)
                                    .unwrap_or_else(|| div().into_any_element()),
                            )
                            .child(
                                h_flex()
                                    .gap_0p5()
                                    .children(enable_coauthors)
                                    .child(self.render_commit_button(cx)),
                            ),
                    )
                    .child(
                        div()
                            .when(self.commit_editor_expanded, |this| {
                                this.flex_1().min_h_0().pb(footer_size)
                            })
                            .pr_2p5()
                            .on_action(|&mav_actions::editor::MoveUp, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_action(|&mav_actions::editor::MoveDown, _, cx| {
                                cx.stop_propagation();
                            })
                            .child(EditorElement::new(&self.commit_editor, panel_editor_style)),
                    )
                    .child(
                        v_flex()
                            .absolute()
                            .top_2()
                            .right_2()
                            .gap_px()
                            .opacity(0.6)
                            .hover(|s| s.opacity(1.0))
                            .child(
                                IconButton::new("expand-commit-editor", IconName::MaximizeAlt)
                                    .icon_size(IconSize::Small)
                                    .tooltip({
                                        move |_window, cx| {
                                            Tooltip::for_action_in(
                                                "Open Commit Modal",
                                                &git::ExpandCommitEditor,
                                                &editor_focus_handle,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(cx.listener({
                                        move |_, _, window, cx| {
                                            window.dispatch_action(
                                                git::ExpandCommitEditor.boxed_clone(),
                                                cx,
                                            )
                                        }
                                    })),
                            )
                            .child({
                                let (icon, label) = if self.commit_editor_expanded {
                                    (IconName::Minimize, "Collapse Commit Editor")
                                } else {
                                    (IconName::Maximize, "Expand Commit Editor")
                                };
                                let focus_handle = self.focus_handle.clone();

                                IconButton::new("fill-commit-editor", icon)
                                    .icon_size(IconSize::Small)
                                    .tooltip({
                                        move |_window, cx| {
                                            Tooltip::for_action_in(
                                                label,
                                                &git::ToggleFillCommitEditor,
                                                &focus_handle,
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(cx.listener({
                                        move |_, _, window, cx| {
                                            window.dispatch_action(
                                                git::ToggleFillCommitEditor.boxed_clone(),
                                                cx,
                                            )
                                        }
                                    }))
                            }),
                    ),
            );

        Some(footer)
    }
}
