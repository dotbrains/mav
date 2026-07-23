use super::*;

impl CommitModal {
    fn commit_editor_element(&self, _window: &mut Window, cx: &mut Context<Self>) -> EditorElement {
        let settings = theme_settings::ThemeSettings::get_global(cx);
        let editor_style = git_commit_editor_style(settings.git_commit_buffer_font_size(cx), cx);
        EditorElement::new(&self.commit_editor, editor_style)
    }

    pub fn render_commit_editor(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let properties = self.properties;
        let padding_t = 3.0;
        let padding_b = 6.0;
        // magic number for editor not to overflow the container??
        let extra_space_hack = 1.5 * window.line_height();

        v_flex()
            .h(px(properties.editor_height + padding_b + padding_t) + extra_space_hack)
            .w_full()
            .flex_none()
            .rounded(properties.editor_border_radius())
            .overflow_hidden()
            .px_1p5()
            .pt(px(padding_t))
            .pb(px(padding_b))
            .child(
                div()
                    .h(px(properties.editor_height))
                    .w_full()
                    .child(self.commit_editor_element(window, cx)),
            )
    }

    fn render_git_commit_menu(
        &self,
        id: impl Into<ElementId>,
        keybinding_target: Option<FocusHandle>,
    ) -> impl IntoElement {
        PopoverMenu::new(id.into())
            .trigger(
                ui::ButtonLike::new_rounded_right("commit-split-button-right")
                    .layer(ui::ElevationIndex::ModalSurface)
                    .size(ui::ButtonSize::None)
                    .child(
                        div()
                            .px_1()
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    ),
            )
            .menu({
                let git_panel_entity = self.git_panel.clone();
                move |window, cx| {
                    let git_panel = git_panel_entity.read(cx);
                    let amend_enabled = git_panel.amend_pending();
                    let signoff_enabled = git_panel.signoff_enabled();
                    let has_previous_commit = git_panel.head_commit(cx).is_some();

                    Some(ContextMenu::build(window, cx, |context_menu, _, _| {
                        context_menu
                            .when_some(keybinding_target.clone(), |el, keybinding_target| {
                                el.context(keybinding_target)
                            })
                            .when(has_previous_commit, |this| {
                                this.toggleable_entry(
                                    "Amend",
                                    amend_enabled,
                                    IconPosition::Start,
                                    Some(Box::new(Amend)),
                                    {
                                        let git_panel = git_panel_entity.downgrade();
                                        move |_, cx| {
                                            git_panel
                                                .update(cx, |git_panel, cx| {
                                                    git_panel.toggle_amend_pending(cx);
                                                })
                                                .ok();
                                        }
                                    },
                                )
                            })
                            .toggleable_entry(
                                "Signoff",
                                signoff_enabled,
                                IconPosition::Start,
                                Some(Box::new(Signoff)),
                                {
                                    let git_panel = git_panel_entity.clone();
                                    move |window, cx| {
                                        git_panel.update(cx, |git_panel, cx| {
                                            git_panel.toggle_signoff_enabled(&Signoff, window, cx);
                                        })
                                    }
                                },
                            )
                    }))
                }
            })
            .with_handle(self.commit_menu_handle.clone())
            .anchor(Anchor::TopRight)
    }

    pub fn render_footer(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            can_commit,
            tooltip,
            commit_label,
            co_authors,
            generate_commit_message,
            active_repo,
            is_amend_pending,
            is_signoff_enabled,
            workspace,
        ) = self.git_panel.update(cx, |git_panel, cx| {
            let (can_commit, tooltip) = git_panel.configure_commit_button(cx);
            let title = git_panel.commit_button_title();
            let co_authors = git_panel.render_co_authors(cx);
            let generate_commit_message = git_panel.render_generate_commit_message_button(cx);
            let active_repo = git_panel.active_repository.clone();
            let is_amend_pending = git_panel.amend_pending();
            let is_signoff_enabled = git_panel.signoff_enabled();
            (
                can_commit,
                tooltip,
                title,
                co_authors,
                generate_commit_message,
                active_repo,
                is_amend_pending,
                is_signoff_enabled,
                git_panel.workspace.clone(),
            )
        });

        let branch = active_repo
            .as_ref()
            .and_then(|repo| repo.read(cx).branch.as_ref())
            .map(|b| b.name().to_owned())
            .unwrap_or_else(|| "<no branch>".to_owned());

        let branch_picker_button = Button::new("branch_picker_button", branch)
            .start_icon(
                Icon::new(IconName::GitBranch)
                    .size(IconSize::Small)
                    .color(Color::Placeholder),
            )
            .style(ButtonStyle::Transparent)
            .color(Color::Muted)
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(mav_actions::git::Branch.boxed_clone(), cx);
            }));

        let branch_picker = PopoverMenu::new("popover-button")
            .menu(move |window, cx| {
                Some(branch_picker::popover(
                    workspace.clone(),
                    false,
                    active_repo.clone(),
                    window,
                    cx,
                ))
            })
            .with_handle(self.branch_list_handle.clone())
            .trigger_with_tooltip(
                branch_picker_button,
                Tooltip::for_action_title("Switch Branch", &mav_actions::git::Branch),
            )
            .anchor(Anchor::BottomLeft)
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            });
        let focus_handle = self.focus_handle(cx);

        let close_kb_hint = ui::KeyBinding::for_action(&menu::Cancel, cx).map(|close_kb| {
            KeybindingHint::new(close_kb, cx.theme().colors().editor_background).suffix("Cancel")
        });

        h_flex()
            .group("commit_editor_footer")
            .flex_none()
            .w_full()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(self.properties.footer_height))
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .flex_shrink_1()
                    .overflow_x_hidden()
                    .child(
                        h_flex()
                            .flex_shrink_1()
                            .overflow_x_hidden()
                            .child(branch_picker),
                    )
                    .children(generate_commit_message)
                    .children(co_authors),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .flex_none()
                    .px_1()
                    .gap_4()
                    .child(close_kb_hint)
                    .child(SplitButton::new(
                        ui::ButtonLike::new_rounded_left(ElementId::Name(
                            format!("split-button-left-{}", commit_label).into(),
                        ))
                        .layer(ui::ElevationIndex::ModalSurface)
                        .size(ui::ButtonSize::Compact)
                        .child(
                            div()
                                .child(Label::new(commit_label).size(LabelSize::Small))
                                .mr_0p5(),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            telemetry::event!("Git Committed", source = "Git Modal");
                            this.git_panel.update(cx, |git_panel, cx| {
                                git_panel.commit_changes(
                                    CommitOptions {
                                        amend: is_amend_pending,
                                        signoff: is_signoff_enabled,
                                        allow_empty: false,
                                    },
                                    window,
                                    cx,
                                )
                            });
                            cx.emit(DismissEvent);
                        }))
                        .disabled(!can_commit)
                        .tooltip({
                            let focus_handle = focus_handle.clone();
                            move |_window, cx| {
                                if can_commit {
                                    Tooltip::with_meta_in(
                                        tooltip,
                                        Some(&git::Commit),
                                        format!(
                                            "git commit{}{}",
                                            if is_amend_pending { " --amend" } else { "" },
                                            if is_signoff_enabled { " --signoff" } else { "" }
                                        ),
                                        &focus_handle.clone(),
                                        cx,
                                    )
                                } else {
                                    Tooltip::simple(tooltip, cx)
                                }
                            }
                        }),
                        self.render_git_commit_menu(
                            ElementId::Name(format!("split-button-right-{}", commit_label).into()),
                            Some(focus_handle),
                        )
                        .into_any_element(),
                    )),
            )
    }
}

impl Render for CommitModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let properties = self.properties;
        let width = px(properties.modal_width);
        let container_padding = px(properties.container_padding);
        let border_radius = properties.modal_border_radius;
        let editor_focus_handle = self.commit_editor.focus_handle(cx);

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

        v_flex()
            .id("commit-modal")
            .key_context("GitCommit")
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::on_commit))
            .on_action(cx.listener(Self::on_amend))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
            .when(!DisableAiSettings::get_global(cx).disable_ai, |this| {
                this.on_action(cx.listener(|this, _: &GenerateCommitMessage, _, cx| {
                    this.git_panel.update(cx, |panel, cx| {
                        panel.generate_commit_message(cx);
                    })
                }))
            })
            .on_action(
                cx.listener(|this, _: &mav_actions::git::Branch, window, cx| {
                    this.toggle_branch_selector(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &mav_actions::git::CheckoutBranch, window, cx| {
                    this.toggle_branch_selector(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &mav_actions::git::Switch, window, cx| {
                    this.toggle_branch_selector(window, cx);
                }),
            )
            .w(width)
            .min_h_112()
            .p(container_padding)
            .elevation_3(cx)
            .overflow_hidden()
            .flex_none()
            .relative()
            .bg(cx.theme().colors().elevated_surface_background)
            .rounded(px(border_radius))
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(
                v_flex()
                    .id("editor-container")
                    .cursor_text()
                    .p_2()
                    .size_full()
                    .gap_2()
                    .justify_between()
                    .rounded(properties.editor_border_radius())
                    .overflow_hidden()
                    .bg(cx.theme().colors().editor_background)
                    .border_1()
                    .border_color(if title_exceeds_limit {
                        cx.theme().status().warning_border
                    } else {
                        cx.theme().colors().border_variant
                    })
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        window.focus(&editor_focus_handle, cx);
                    }))
                    .child(self.render_commit_editor(window, cx))
                    .when(title_exceeds_limit, |this| {
                        this.child(
                            h_flex()
                                .absolute()
                                .bottom_12()
                                .w_full()
                                .py_1()
                                .px_2()
                                .gap_1()
                                .justify_center()
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
                    .child(self.render_footer(window, cx)),
            )
    }
}
