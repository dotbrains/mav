use super::*;

impl GitPanel {
    pub(super) fn set_sort_by_path(
        &mut self,
        _: &SetSortByPath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            let fs = workspace.app_state().fs.clone();
            cx.update_global::<SettingsStore, _>(|store, _cx| {
                store.update_settings_file(fs, move |settings, _cx| {
                    settings.git_panel.get_or_insert_default().sort_by = Some(GitPanelSortBy::Path);
                });
            });
        }
    }

    pub(super) fn set_sort_by_name(
        &mut self,
        _: &SetSortByName,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            let fs = workspace.app_state().fs.clone();
            cx.update_global::<SettingsStore, _>(|store, _cx| {
                store.update_settings_file(fs, move |settings, _cx| {
                    settings.git_panel.get_or_insert_default().sort_by = Some(GitPanelSortBy::Name);
                });
            });
        }
    }

    pub(super) fn set_group_by_none(
        &mut self,
        _: &SetGroupByNone,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            let fs = workspace.app_state().fs.clone();
            cx.update_global::<SettingsStore, _>(|store, _cx| {
                store.update_settings_file(fs, move |settings, _cx| {
                    settings.git_panel.get_or_insert_default().group_by =
                        Some(GitPanelGroupBy::None);
                });
            });
        }
    }

    pub(super) fn set_group_by_status(
        &mut self,
        _: &SetGroupByStatus,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            let fs = workspace.app_state().fs.clone();
            cx.update_global::<SettingsStore, _>(|store, _cx| {
                store.update_settings_file(fs, move |settings, _cx| {
                    settings.git_panel.get_or_insert_default().group_by =
                        Some(GitPanelGroupBy::Status);
                });
            });
        }
    }

    pub(super) fn toggle_tree_view(
        &mut self,
        _: &ToggleTreeView,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_setting = GitPanelSettings::get_global(cx).tree_view;
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            let fs = workspace.app_state().fs.clone();
            cx.update_global::<SettingsStore, _>(|store, _cx| {
                store.update_settings_file(fs, move |settings, _cx| {
                    settings.git_panel.get_or_insert_default().tree_view = Some(!current_setting);
                });
            })
        }
    }

    pub(crate) fn increase_font_size(
        &mut self,
        action: &IncreaseBufferFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_font_size_action(action.persist, px(1.0), cx);
    }

    pub(crate) fn decrease_font_size(
        &mut self,
        action: &DecreaseBufferFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_font_size_action(action.persist, px(-1.0), cx);
    }

    pub(super) fn handle_font_size_action(
        &mut self,
        persist: bool,
        delta: Pixels,
        cx: &mut Context<Self>,
    ) {
        if persist {
            update_settings_file(self.fs.clone(), cx, move |settings, cx| {
                let git_commit_buffer_font_size =
                    ThemeSettings::get_global(cx).git_commit_buffer_font_size(cx) + delta;

                let _ = settings.theme.git_commit_buffer_font_size.insert(
                    f32::from(theme_settings::clamp_font_size(git_commit_buffer_font_size)).into(),
                );
            });
        } else {
            theme_settings::adjust_git_commit_buffer_font_size(cx, |size| size + delta);
        }
    }

    pub(crate) fn reset_font_size(
        &mut self,
        action: &ResetBufferFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.persist {
            update_settings_file(self.fs.clone(), cx, move |settings, _| {
                settings.theme.git_commit_buffer_font_size = None;
            });
        } else {
            theme_settings::reset_git_commit_buffer_font_size(cx);
        }
    }

    pub(super) fn toggle_directory(
        &mut self,
        key: &TreeKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.view_mode.tree_state_mut() {
            let expanded = state.expanded_dirs.entry(key.path.clone()).or_insert(true);
            *expanded = !*expanded;
            self.tree_expanded_dirs = state.expanded_dirs.clone();
            self.update_visible_entries(window, cx);
        } else {
            util::debug_panic!("Attempted to toggle directory in flat Git Panel state");
        }
    }
}
