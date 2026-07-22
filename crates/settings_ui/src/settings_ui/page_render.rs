use super::*;

impl SettingsWindow {
    fn render_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> impl IntoElement {
        let page_header;
        let page_content;

        if let Some(current_sub_page) = self.sub_page_stack.last() {
            let is_skills_page =
                current_sub_page.link.json_path == Some(AGENT_SKILLS_SETTINGS_PATH);
            page_header = h_flex()
                .w_full()
                .min_w_0()
                .justify_between()
                .child(
                    h_flex()
                        .min_w_0()
                        .ml_neg_1p5()
                        .gap_1()
                        .child(
                            IconButton::new("back-btn", IconName::ArrowLeft)
                                .icon_size(IconSize::Small)
                                .shape(IconButtonShape::Square)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.pop_sub_page(window, cx);
                                })),
                        )
                        .child(self.render_sub_page_breadcrumbs(window, cx)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .when(current_sub_page.link.in_json, |this| {
                            this.child(
                                Button::new("open-in-settings-file", "Edit in settings.json")
                                    .tab_index(0_isize)
                                    .style(ButtonStyle::OutlinedGhost)
                                    .tooltip(Tooltip::for_action_title_in(
                                        "Edit in settings.json",
                                        &OpenCurrentFile,
                                        &self.focus_handle,
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_current_settings_file(window, cx);
                                    })),
                            )
                        })
                        .when(is_skills_page, |this| {
                            this.child(
                                Button::new("open-skill-creator", "Create Skill")
                                    .tab_index(0_isize)
                                    .style(ButtonStyle::OutlinedGhost)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_skill_creator_sub_page(
                                            pages::SkillCreatorOpenMode::Form,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                        }),
                )
                .into_any_element();

            let active_page_render_fn = &current_sub_page.link.render;
            page_content =
                (active_page_render_fn)(self, &current_sub_page.scroll_handle, window, cx);
        } else {
            page_header = self.render_files_header(window, cx).into_any_element();

            page_content = self
                .render_current_page_items(window, cx)
                .into_any_element();
        }

        let current_sub_page = self.sub_page_stack.last();

        let mut warning_banner = gpui::Empty.into_any_element();
        if let Some(error) =
            SettingsStore::global(cx).error_for_file(self.current_file.to_settings())
        {
            fn banner(
                label: &'static str,
                error: String,
                shown_errors: &mut HashSet<String>,
                cx: &mut Context<SettingsWindow>,
            ) -> impl IntoElement {
                if shown_errors.insert(error.clone()) {
                    telemetry::event!("Settings Error Shown", label = label, error = &error);
                }
                Banner::new()
                    .severity(Severity::Warning)
                    .child(
                        v_flex()
                            .my_0p5()
                            .gap_0p5()
                            .child(Label::new(label))
                            .child(Label::new(error).size(LabelSize::Small).color(Color::Muted)),
                    )
                    .action_slot(
                        div().pr_1().pb_1().child(
                            Button::new("fix-in-json", "Fix in settings.json")
                                .tab_index(0_isize)
                                .style(ButtonStyle::Tinted(ui::TintColor::Warning))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_current_settings_file(window, cx);
                                })),
                        ),
                    )
            }

            let parse_error = error.parse_error();
            let parse_failed = parse_error.is_some();

            warning_banner = v_flex()
                .gap_2()
                .when_some(parse_error, |this, err| {
                    this.child(banner(
                        "Failed to load your settings. Some values may be incorrect and changes may be lost.",
                        err,
                        &mut self.shown_errors,
                        cx,
                    ))
                })
                .map(|this| match &error.migration_status {
                    settings::MigrationStatus::Succeeded => this.child(banner(
                        "Your settings are out of date, and need to be updated.",
                        match &self.current_file {
                            SettingsUiFile::User => "They can be automatically migrated to the latest version.",
                            SettingsUiFile::Server(_) | SettingsUiFile::Project(_)  => "They must be manually migrated to the latest version."
                        }.to_string(),
                        &mut self.shown_errors,
                        cx,
                    )),
                    settings::MigrationStatus::Failed { error: err } if !parse_failed => this
                        .child(banner(
                            "Your settings file is out of date, automatic migration failed",
                            err.clone(),
                            &mut self.shown_errors,
                            cx,
                        )),
                    _ => this,
                })
                .into_any_element()
        }

        let mut restricted_banner = gpui::Empty.into_any_element();
        if let SettingsUiFile::Project((worktree_id, _)) = &self.current_file {
            let worktree_id = *worktree_id;
            let is_restricted = all_projects(self.original_window.as_ref(), cx)
                .find(|project| project.read(cx).worktree_for_id(worktree_id, cx).is_some())
                .map(|project| {
                    let worktree_store = project.read(cx).worktree_store();
                    project::trusted_worktrees::TrustedWorktrees::has_restricted_worktrees(
                        &worktree_store,
                        cx,
                    )
                })
                .unwrap_or(false);

            if is_restricted {
                let original_window = self.original_window;
                restricted_banner = Banner::new()
                    .severity(Severity::Warning)
                    .child(
                        v_flex()
                            .my_0p5()
                            .gap_0p5()
                            .child(Label::new("Restricted Mode"))
                            .child(
                                Label::new(
                                    "This project is in restricted mode. Some project settings may not apply.",
                                )
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            ),
                    )
                    .action_slot(
                        div().pr_2().pb_1().child(
                            Button::new("manage-trust", "Manage Trust")
                                .style(ButtonStyle::Tinted(ui::TintColor::Warning))
                                .on_click(cx.listener(move |_this, _, window, cx| {
                                    if let Some(original_window) = original_window {
                                        original_window
                                            .update(cx, |multi_workspace, window, cx| {
                                                multi_workspace
                                                    .workspace()
                                                    .update(cx, |workspace, cx| {
                                                        workspace
                                                            .show_worktree_trust_security_modal(
                                                                true, window, cx,
                                                            );
                                                    });
                                            })
                                            .log_err();
                                    }
                                    // Close the settings window
                                    window.remove_window();
                                })),
                        ),
                    )
                    .into_any_element();
            }
        }

        v_flex()
            .id("settings-ui-page")
            .on_action(cx.listener(|this, _: &menu::SelectNext, window, cx| {
                if !this.sub_page_stack.is_empty() {
                    // Keep Tab navigation within the sub-page content. Global
                    // `focus_next` would otherwise wrap past the last control to
                    // the navbar; instead, when focus leaves the content region we
                    // wrap back to the first content tab stop.
                    let content_handle = this.content_focus_handle.focus_handle(cx);
                    window.focus_next(cx);
                    if !content_handle.contains_focused(window, cx) {
                        content_handle.focus(window, cx);
                        window.focus_next(cx);
                    }
                    return;
                }
                for (logical_index, (actual_index, _)) in this.visible_page_items().enumerate() {
                    let handle = this.content_handles[this.current_page_index()][actual_index]
                        .focus_handle(cx);
                    let mut offset = 1; // for page header

                    if let Some((_, next_item)) = this.visible_page_items().nth(logical_index + 1)
                        && matches!(next_item, SettingsPageItem::SectionHeader(_))
                    {
                        offset += 1;
                    }
                    if handle.contains_focused(window, cx) {
                        let next_logical_index = logical_index + offset + 1;
                        this.list_state.scroll_to_reveal_item(next_logical_index);
                        // We need to render the next item to ensure it's focus handle is in the element tree
                        cx.on_next_frame(window, |_, window, cx| {
                            cx.notify();
                            cx.on_next_frame(window, |_, window, cx| {
                                window.focus_next(cx);
                                cx.notify();
                            });
                        });
                        cx.notify();
                        return;
                    }
                }
                window.focus_next(cx);
            }))
            .on_action(cx.listener(|this, _: &menu::SelectPrevious, window, cx| {
                if !this.sub_page_stack.is_empty() {
                    window.focus_prev(cx);
                    return;
                }
                let mut prev_was_header = false;
                for (logical_index, (actual_index, item)) in this.visible_page_items().enumerate() {
                    let is_header = matches!(item, SettingsPageItem::SectionHeader(_));
                    let handle = this.content_handles[this.current_page_index()][actual_index]
                        .focus_handle(cx);
                    let mut offset = 1; // for page header

                    if prev_was_header {
                        offset -= 1;
                    }
                    if handle.contains_focused(window, cx) {
                        let next_logical_index = logical_index + offset - 1;
                        this.list_state.scroll_to_reveal_item(next_logical_index);
                        // We need to render the next item to ensure it's focus handle is in the element tree
                        cx.on_next_frame(window, |_, window, cx| {
                            cx.notify();
                            cx.on_next_frame(window, |_, window, cx| {
                                window.focus_prev(cx);
                                cx.notify();
                            });
                        });
                        cx.notify();
                        return;
                    }
                    prev_was_header = is_header;
                }
                window.focus_prev(cx);
            }))
            .when(current_sub_page.is_none(), |this| {
                this.vertical_scrollbar_for(&self.list_state, window, cx)
            })
            .when_some(current_sub_page, |this, current_sub_page| {
                this.custom_scrollbars(
                    Scrollbars::new(ui::ScrollAxes::Vertical)
                        .tracked_scroll_handle(&current_sub_page.scroll_handle)
                        .id((current_sub_page.link.title.clone(), 42)),
                    window,
                    cx,
                )
            })
            .track_focus(&self.content_focus_handle.focus_handle(cx))
            .pt_6()
            .gap_4()
            .flex_1()
            .min_w_0()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .px_8()
                    .gap_2()
                    .child(page_header)
                    .child(warning_banner)
                    .child(restricted_banner),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .size_full()
                    .tab_group()
                    .tab_index(CONTENT_GROUP_TAB_INDEX)
                    .child(page_content),
            )
    }

    /// This function will create a new settings file if one doesn't exist
    /// if the current file is a project settings with a valid worktree id
    /// We do this because the settings ui allows initializing project settings
    fn open_current_settings_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.current_file {
            SettingsUiFile::User => {
                let Some(original_window) = self.original_window else {
                    return;
                };
                original_window
                    .update(cx, |multi_workspace, window, cx| {
                        multi_workspace
                            .workspace()
                            .clone()
                            .update(cx, |workspace, cx| {
                                workspace
                                    .with_local_or_wsl_workspace(
                                        window,
                                        cx,
                                        open_user_settings_in_workspace,
                                    )
                                    .detach();
                            });
                    })
                    .ok();

                window.remove_window();
            }
            SettingsUiFile::Project((worktree_id, path)) => {
                let settings_path = path.join(paths::local_settings_file_relative_path());
                let app_state = workspace::AppState::global(cx);

                let Some((workspace_window, worktree, corresponding_workspace)) = app_state
                    .workspace_store
                    .read(cx)
                    .workspaces_with_windows()
                    .filter_map(|(window_handle, weak)| {
                        let workspace = weak.upgrade()?;
                        let window = window_handle.downcast::<MultiWorkspace>()?;
                        Some((window, workspace))
                    })
                    .find_map(|(window, workspace): (_, Entity<Workspace>)| {
                        workspace
                            .read(cx)
                            .project()
                            .read(cx)
                            .worktree_for_id(*worktree_id, cx)
                            .map(|worktree| (window, worktree, workspace))
                    })
                else {
                    log::error!(
                        "No corresponding workspace contains worktree id: {}",
                        worktree_id
                    );

                    return;
                };

                let create_task = if worktree.read(cx).entry_for_path(&settings_path).is_some() {
                    None
                } else {
                    Some(worktree.update(cx, |tree, cx| {
                        tree.create_entry(
                            settings_path.clone(),
                            false,
                            Some(initial_project_settings_content().as_bytes().to_vec()),
                            cx,
                        )
                    }))
                };

                let worktree_id = *worktree_id;

                // TODO: move mav::open_local_file() APIs to this crate, and
                // re-implement the "initial_contents" behavior
                let workspace_weak = corresponding_workspace.downgrade();
                workspace_window
                    .update(cx, |_, window, cx| {
                        cx.spawn_in(window, async move |_, cx| {
                            if let Some(create_task) = create_task {
                                create_task.await.ok()?;
                            };

                            workspace_weak
                                .update_in(cx, |workspace, window, cx| {
                                    workspace.open_path(
                                        (worktree_id, settings_path.clone()),
                                        None,
                                        true,
                                        window,
                                        cx,
                                    )
                                })
                                .ok()?
                                .await
                                .log_err()?;

                            workspace_weak
                                .update_in(cx, |_, window, cx| {
                                    window.activate_window();
                                    cx.notify();
                                })
                                .ok();

                            Some(())
                        })
                        .detach();
                    })
                    .ok();

                window.remove_window();
            }
            SettingsUiFile::Server(_) => {
                // Server files are not editable
                return;
            }
        };
    }
}
