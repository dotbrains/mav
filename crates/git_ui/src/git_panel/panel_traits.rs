use super::*;

impl Render for GitPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project = self.project.read(cx);
        let has_entries = !self.entries.is_empty();
        let has_write_access = self.has_write_access(cx);

        #[cfg(feature = "call")]
        let has_co_authors = self
            .workspace
            .upgrade()
            .and_then(|_workspace| {
                call::ActiveCall::try_global(cx).and_then(|call| call.read(cx).room().cloned())
            })
            .is_some_and(|room| {
                self.load_local_committer(cx);
                let room = room.read(cx);
                room.remote_participants()
                    .values()
                    .any(|remote_participant| remote_participant.can_write())
            });
        #[cfg(not(feature = "call"))]
        let has_co_authors = false;

        v_flex()
            .id("git_panel")
            .key_context(self.dispatch_context(window, cx))
            .track_focus(&self.focus_handle)
            .when(has_write_access && !project.is_read_only(cx), |this| {
                this.on_action(cx.listener(Self::toggle_staged_for_selected))
                    .on_action(cx.listener(Self::stage_range))
                    .on_action(cx.listener(GitPanel::on_commit))
                    .on_action(cx.listener(GitPanel::on_amend))
                    .on_action(cx.listener(GitPanel::toggle_signoff_enabled))
                    .on_action(cx.listener(Self::stage_all))
                    .on_action(cx.listener(Self::unstage_all))
                    .on_action(cx.listener(Self::stage_selected))
                    .on_action(cx.listener(Self::unstage_selected))
                    .on_action(cx.listener(Self::restore_tracked_files))
                    .on_action(cx.listener(Self::revert_selected))
                    .on_action(cx.listener(Self::add_to_gitignore))
                    .on_action(cx.listener(Self::add_to_git_info_exclude))
                    .on_action(cx.listener(Self::clean_all))
                    .on_action(cx.listener(Self::generate_commit_message_action))
                    .on_action(cx.listener(Self::stash_all))
                    .on_action(cx.listener(Self::stash_pop))
            })
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::first_entry))
            .on_action(cx.listener(Self::next_entry))
            .on_action(cx.listener(Self::previous_entry))
            .on_action(cx.listener(Self::last_entry))
            .on_action(cx.listener(Self::close_panel))
            .on_action(cx.listener(Self::open_diff))
            .on_action(cx.listener(Self::open_solo_diff))
            .on_action(cx.listener(Self::view_file))
            .on_action(cx.listener(Self::focus_changes_list))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::expand_commit_editor))
            .when(has_write_access && has_co_authors, |git_panel| {
                git_panel.on_action(cx.listener(Self::toggle_fill_co_authors))
            })
            .on_action(cx.listener(Self::set_sort_by_path))
            .on_action(cx.listener(Self::set_sort_by_name))
            .on_action(cx.listener(Self::set_group_by_none))
            .on_action(cx.listener(Self::set_group_by_status))
            .on_action(cx.listener(Self::toggle_tree_view))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
            .on_action(cx.listener(Self::activate_changes_tab))
            .on_action(cx.listener(Self::activate_history_tab))
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .size_full()
                    .when(!self.commit_editor_expanded, |this| {
                        this.child(self.render_tab_bar(cx))
                    })
                    .map(|this| match self.active_tab {
                        GitPanelTab::Changes => this
                            .children(self.render_changes_header(window, cx))
                            .when(!self.commit_editor_expanded, |this| {
                                this.map(|this| {
                                    if let Some(repo) = self.active_repository.clone()
                                        && has_entries
                                    {
                                        this.child(self.render_entries(
                                            has_write_access,
                                            repo,
                                            window,
                                            cx,
                                        ))
                                    } else {
                                        this.child(self.render_empty_state(cx).into_any_element())
                                    }
                                })
                            })
                            .children(self.render_footer(window, cx))
                            .when(self.amend_pending, |this| {
                                this.child(self.render_pending_amend(cx))
                            })
                            .when(!self.amend_pending, |this| {
                                this.children(self.render_previous_commit(window, cx))
                            }),
                        GitPanelTab::History => this.child(self.render_history_tab(window, cx)),
                    })
                    .into_any_element(),
            )
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

impl Focusable for GitPanel {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        if self.entries.is_empty() || self.commit_editor_expanded {
            self.commit_editor.focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }
}

impl EventEmitter<Event> for GitPanel {}

impl EventEmitter<PanelEvent> for GitPanel {}

pub(crate) struct GitPanelAddon {
    pub(crate) workspace: WeakEntity<Workspace>,
}

impl editor::Addon for GitPanelAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn render_buffer_header_controls(
        &self,
        _excerpt_info: &ExcerptBoundaryInfo,
        buffer: &language::BufferSnapshot,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let file = buffer.file()?;
        let git_panel = self.workspace.upgrade()?.read(cx).panel::<GitPanel>(cx)?;

        git_panel
            .read(cx)
            .render_buffer_header_controls(&git_panel, file, window, cx)
    }
}

impl Panel for GitPanel {
    fn persistent_name() -> &'static str {
        "GitPanel"
    }

    fn panel_key() -> &'static str {
        GIT_PANEL_KEY
    }

    fn position(&self, _: &Window, cx: &App) -> DockPosition {
        GitPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.git_panel.get_or_insert_default().dock = Some(position.into())
        });
    }

    fn default_size(&self, _: &Window, cx: &App) -> Pixels {
        GitPanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::GitBranch)
    }

    fn button_visible(&self, cx: &App) -> bool {
        GitPanelSettings::get_global(cx).button
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Git Panel")
    }

    fn icon_label(&self, _: &Window, cx: &App) -> Option<String> {
        if !GitPanelSettings::get_global(cx).show_count_badge {
            return None;
        }
        let total = self.changes_count;
        (total > 0).then(|| total.to_string())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _: &Window, cx: &App) -> bool {
        GitPanelSettings::get_global(cx).starts_open
    }

    fn activation_priority(&self) -> u32 {
        3
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.git_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

impl PanelHeader for GitPanel {}

pub(crate) fn commit_title_exceeds_limit(title: &str, max_length: usize) -> bool {
    max_length > 0 && title.chars().count() > max_length
}
