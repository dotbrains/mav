use super::*;

impl GitPanel {
    pub(super) fn open_diff(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab == GitPanelTab::History {
            self.open_selected_history_commit(window, cx);
            return;
        }
        if let Some(GitListEntry::Directory(dir_entry)) = self
            .selected_entry
            .and_then(|i| self.entries.get(i))
            .cloned()
        {
            self.toggle_directory(&dir_entry.key, window, cx);
            return;
        }
        maybe!({
            let entry = self.entries.get(self.selected_entry?)?.status_entry()?;
            let workspace = self.workspace.upgrade()?;
            let git_repo = self.active_repository.as_ref()?;

            if let Some(project_diff) = workspace.read(cx).active_item_as::<ProjectDiff>(cx)
                && let Some(project_path) = project_diff.read(cx).active_project_path(cx)
                && Some(&entry.repo_path)
                    == git_repo
                        .read(cx)
                        .project_path_to_repo_path(&project_path, cx)
                        .as_ref()
            {
                project_diff.focus_handle(cx).focus(window, cx);
                project_diff.update(cx, |project_diff, cx| project_diff.autoscroll(cx));
                return None;
            };

            self.workspace
                .update(cx, |workspace, cx| {
                    ProjectDiff::deploy_at(workspace, Some(entry.clone()), window, cx);
                })
                .ok();
            self.focus_handle.focus(window, cx);

            Some(())
        });
    }

    pub(super) fn open_solo_diff(
        &mut self,
        _: &menu::SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        maybe!({
            let entry = self
                .entries
                .get(self.selected_entry?)?
                .status_entry()?
                .clone();
            let repository = self.active_repository.clone()?;

            SoloDiffView::open_or_focus(entry, repository, self.workspace.clone(), window, cx)
                .detach_and_notify_err(self.workspace.clone(), window, cx);

            Some(())
        });
    }

    pub(super) fn view_file(&mut self, _: &ViewFile, window: &mut Window, cx: &mut Context<Self>) {
        maybe!({
            let entry = self.entries.get(self.selected_entry?)?.status_entry()?;
            let project_path = self
                .active_repository
                .as_ref()?
                .read(cx)
                .repo_path_to_project_path(&entry.repo_path, cx)?;

            self.workspace
                .update(cx, |workspace, cx| {
                    workspace
                        .open_path_preview_in_tabbed_pane(
                            project_path,
                            None,
                            false,
                            false,
                            true,
                            window,
                            cx,
                        )
                        .detach_and_log_err(cx);
                })
                .ok()?;

            Some(())
        });
    }

    pub(super) fn open_selected_entry_on_click(
        &mut self,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry_primary_click_action =
            GitPanelSettings::get_global(cx).entry_primary_click_action;
        let action = match (entry_primary_click_action, secondary) {
            (GitPanelClickBehavior::ProjectDiff, false) => GitPanelClickBehavior::ProjectDiff,
            (GitPanelClickBehavior::ProjectDiff, true) => GitPanelClickBehavior::FileDiff,
            (GitPanelClickBehavior::FileDiff, false) => GitPanelClickBehavior::FileDiff,
            (GitPanelClickBehavior::FileDiff, true) => GitPanelClickBehavior::ProjectDiff,
            (GitPanelClickBehavior::ViewFile, false) => GitPanelClickBehavior::ViewFile,
            (GitPanelClickBehavior::ViewFile, true) => GitPanelClickBehavior::ProjectDiff,
        };
        match action {
            GitPanelClickBehavior::ProjectDiff => {
                self.open_diff(&Default::default(), window, cx);
                self.focus_handle.focus(window, cx);
            }
            GitPanelClickBehavior::FileDiff => {
                self.open_solo_diff(&Default::default(), window, cx);
            }
            GitPanelClickBehavior::ViewFile => {
                self.view_file(&Default::default(), window, cx);
            }
        }
    }
}
