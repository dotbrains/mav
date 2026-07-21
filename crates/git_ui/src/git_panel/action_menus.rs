use super::*;

impl GitPanel {
    pub(super) fn render_view_options_menu(&self, id: impl Into<ElementId>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();

        PopoverMenu::new(id.into())
            .trigger_with_tooltip(
                IconButton::new("view-options-menu-trigger", IconName::Sliders)
                    .icon_size(IconSize::Small),
                Tooltip::text("View Options"),
            )
            .menu(move |window, cx| {
                Some(git_panel_view_options_menu(
                    focus_handle.clone(),
                    window,
                    cx,
                ))
            })
            .anchor(Anchor::TopRight)
    }

    pub(crate) fn render_generate_commit_message_button(
        &self,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !agent_settings::AgentSettings::get_global(cx).enabled(cx) {
            return None;
        }

        if self.generate_commit_message_task.is_some() {
            return Some(
                h_flex()
                    .gap_1()
                    .child(
                        IconButton::new("cancel-generate-commit-message", IconName::Stop)
                            .icon_color(Color::Error)
                            .icon_size(IconSize::Small)
                            .style(ButtonStyle::Tinted(TintColor::Error))
                            .tooltip(Tooltip::text("Cancel Commit Message Generation"))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.generate_commit_message_task.take();
                                cx.notify();
                            })),
                    )
                    .child(
                        Label::new("Generating Commit…")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            );
        }

        let model_registry = LanguageModelRegistry::read_global(cx);
        let has_commit_model_configuration_error = model_registry
            .configuration_error(model_registry.commit_message_model(cx), cx)
            .is_some();
        let can_commit = self.can_commit();

        let editor_focus_handle = self.commit_editor.focus_handle(cx);

        Some(
            IconButton::new("generate-commit-message", IconName::AiEdit)
                .shape(ui::IconButtonShape::Square)
                .icon_color(if has_commit_model_configuration_error {
                    Color::Disabled
                } else {
                    Color::Muted
                })
                .tooltip(move |_window, cx| {
                    if !can_commit {
                        Tooltip::simple("No Changes to Commit", cx)
                    } else if has_commit_model_configuration_error {
                        Tooltip::simple("Configure an LLM provider to generate commit messages", cx)
                    } else {
                        Tooltip::for_action_in(
                            "Generate Commit Message",
                            &git::GenerateCommitMessage,
                            &editor_focus_handle,
                            cx,
                        )
                    }
                })
                .disabled(!can_commit || has_commit_model_configuration_error)
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.generate_commit_message(cx);
                }))
                .into_any_element(),
        )
    }

    pub(crate) fn render_co_authors(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let potential_co_authors = self.potential_co_authors(cx);

        let (tooltip_label, icon) = if self.add_coauthors {
            ("Remove co-authored-by", IconName::Person)
        } else {
            ("Add co-authored-by", IconName::UserCheck)
        };

        if potential_co_authors.is_empty() {
            None
        } else {
            Some(
                IconButton::new("co-authors", icon)
                    .shape(ui::IconButtonShape::Square)
                    .icon_color(Color::Disabled)
                    .selected_icon_color(Color::Selected)
                    .toggle_state(self.add_coauthors)
                    .tooltip(move |_, cx| {
                        let title = format!(
                            "{}:{}{}",
                            tooltip_label,
                            if potential_co_authors.len() == 1 {
                                ""
                            } else {
                                "\n"
                            },
                            potential_co_authors
                                .iter()
                                .map(|(name, email)| format!(" {} <{}>", name, email))
                                .join("\n")
                        );
                        Tooltip::simple(title, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_coauthors = !this.add_coauthors;
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        }
    }

    pub(super) fn render_git_commit_menu(
        &self,
        id: impl Into<ElementId>,
        keybinding_target: Option<FocusHandle>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        PopoverMenu::new(id.into())
            .trigger(
                ui::ButtonLike::new_rounded_right("commit-split-button-right")
                    .layer(ui::ElevationIndex::ModalSurface)
                    .size(ButtonSize::None)
                    .child(
                        h_flex()
                            .px_1()
                            .h_full()
                            .justify_center()
                            .border_l_1()
                            .border_color(cx.theme().colors().border)
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    ),
            )
            .menu({
                let git_panel = cx.entity();
                let has_previous_commit = self.head_commit(cx).is_some();
                let amend = self.amend_pending();
                let signoff = self.signoff_enabled;

                move |window, cx| {
                    Some(ContextMenu::build(window, cx, |context_menu, _, _| {
                        context_menu
                            .when_some(keybinding_target.clone(), |el, keybinding_target| {
                                el.context(keybinding_target)
                            })
                            .when(has_previous_commit, |this| {
                                this.toggleable_entry(
                                    "Amend",
                                    amend,
                                    IconPosition::Start,
                                    Some(Box::new(Amend)),
                                    {
                                        let git_panel = git_panel.downgrade();
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
                                signoff,
                                IconPosition::Start,
                                Some(Box::new(Signoff)),
                                move |window, cx| window.dispatch_action(Box::new(Signoff), cx),
                            )
                    }))
                }
            })
            .anchor(Anchor::TopRight)
    }

    pub fn configure_commit_button(&self, cx: &mut Context<Self>) -> (bool, &'static str) {
        if self.has_unstaged_conflicts() {
            (false, "You must resolve conflicts before committing")
        } else if !self.has_staged_changes() && !self.has_tracked_changes() && !self.amend_pending {
            (false, "No changes to commit")
        } else if self.pending_commit.is_some() {
            (false, "Commit in progress")
        } else if !self.has_commit_message(cx) {
            (false, "No commit message")
        } else if !self.has_write_access(cx) {
            (false, "You do not have write access to this project")
        } else {
            (true, self.commit_button_title())
        }
    }

    pub fn commit_button_title(&self) -> &'static str {
        if self.amend_pending {
            if self.has_staged_changes() {
                "Amend"
            } else if self.has_tracked_changes() {
                "Amend Tracked"
            } else {
                "Amend"
            }
        } else if self.has_staged_changes() {
            "Commit"
        } else {
            "Commit Tracked"
        }
    }

    pub(super) fn toggle_fill_commit_editor(
        &mut self,
        _: &ToggleFillCommitEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_editor_expanded = !self.commit_editor_expanded;
        self.commit_editor.update(cx, |editor, _cx| {
            if self.commit_editor_expanded {
                editor.set_mode(EditorMode::Full {
                    scale_ui_elements_with_buffer_font_size: false,
                    show_active_line_background: false,
                    sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
                })
            } else {
                editor.set_mode(EditorMode::AutoHeight {
                    min_lines: MAX_PANEL_EDITOR_LINES,
                    max_lines: Some(MAX_PANEL_EDITOR_LINES),
                })
            }
        });

        cx.notify();
    }

    pub(super) fn expand_commit_editor(
        &mut self,
        _: &ExpandCommitEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        window.defer(cx, move |window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    CommitModal::toggle(workspace, None, window, cx)
                })
                .ok();
        })
    }

    pub(super) fn render_git_changes_actions_menu(
        &self,
        id: impl Into<ElementId>,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_tracked_changes = self.has_tracked_changes();
        let has_staged_changes = self.has_staged_changes();
        let has_unstaged_changes = self.has_unstaged_changes();
        let has_new_changes = self.new_count > 0;
        let has_stash_items = self.stash_entries.entries.len() > 0;
        let focus_handle = self.focus_handle.clone();

        PopoverMenu::new(id.into())
            .trigger(
                ui::ButtonLike::new_rounded_right("git-changes-actions-split-button-right")
                    .layer(ui::ElevationIndex::ModalSurface)
                    .size(ButtonSize::None)
                    .child(
                        div()
                            .px_1()
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    ),
            )
            .menu(move |window, cx| {
                Some(git_panel_context_menu(
                    has_tracked_changes,
                    has_staged_changes,
                    has_unstaged_changes,
                    has_new_changes,
                    has_stash_items,
                    focus_handle.clone(),
                    window,
                    cx,
                ))
            })
            .anchor(Anchor::TopRight)
    }

    pub(super) fn render_git_changes_actions_button(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (text, action, stage, tooltip) =
            if self.total_staged_count() == self.entry_count && self.entry_count > 0 {
                ("Unstage All", UnstageAll.boxed_clone(), false, "git reset")
            } else {
                ("Stage All", StageAll.boxed_clone(), true, "git add --all")
            };

        SplitButton::new(
            ButtonLike::new_rounded_left("git-changes-actions-split-button-left")
                .layer(ElevationIndex::ModalSurface)
                .size(ButtonSize::Compact)
                .child(Label::new(text).size(LabelSize::Small).mr_0p5())
                .tooltip(Tooltip::for_action_title_in(
                    tooltip,
                    action.as_ref(),
                    &self.focus_handle,
                ))
                .disabled(self.entry_count == 0)
                .on_click({
                    let git_panel = cx.weak_entity();
                    move |_, _, cx| {
                        git_panel
                            .update(cx, |git_panel, cx| {
                                git_panel.change_all_files_stage(stage, cx);
                            })
                            .ok();
                    }
                }),
            self.render_git_changes_actions_menu("git-changes-actions-split-button-menu", cx)
                .into_any_element(),
        )
    }
}
