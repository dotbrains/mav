use super::*;

impl Workspace {
    pub fn register_action<A: Action>(
        &mut self,
        callback: impl Fn(&mut Self, &A, &mut Window, &mut Context<Self>) + 'static,
    ) -> &mut Self {
        let callback = Arc::new(callback);

        self.workspace_actions.push(Box::new(move |div, _, _, cx| {
            let callback = callback.clone();
            div.on_action(cx.listener(move |workspace, event, window, cx| {
                (callback)(workspace, event, window, cx)
            }))
        }));
        self
    }

    pub fn register_action_renderer(
        &mut self,
        callback: impl Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div + 'static,
    ) -> &mut Self {
        self.workspace_actions.push(Box::new(callback));
        self
    }

    pub(crate) fn add_workspace_actions_listeners(
        &self,
        mut div: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        for action in self.workspace_actions.iter() {
            div = (action)(div, self, window, cx)
        }
        div
    }

    pub fn has_active_modal(&self, _: &mut Window, cx: &mut App) -> bool {
        self.modal_layer.read(cx).has_active_modal()
    }

    pub fn active_modal<V: ManagedView + 'static>(&self, cx: &App) -> Option<Entity<V>> {
        self.modal_layer.read(cx).active_modal()
    }

    pub fn toggle_modal<V: ModalView, B>(&mut self, window: &mut Window, cx: &mut App, build: B)
    where
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        self.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.toggle_modal(window, cx, build)
        })
    }

    pub fn hide_modal(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx))
    }

    pub(crate) fn reopen_last_picker(
        &mut self,
        _: &ReopenLastPicker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // When triggered from within another modal (e.g. the command palette), that
        // modal's dismissal is asynchronous, so defer the reveal until it has closed;
        // otherwise a modal would still be active and the reveal would be a no-op.
        cx.defer_in(window, |workspace, window, cx| {
            workspace.modal_layer.update(cx, |modal_layer, cx| {
                modal_layer.reveal_stashed_modal(window, cx);
            });
        });
    }

    pub fn toggle_status_toast<V: ToastView>(&mut self, entity: Entity<V>, cx: &mut App) {
        self.toast_layer
            .update(cx, |toast_layer, cx| toast_layer.toggle_toast(cx, entity))
    }

    pub fn toggle_centered_layout(
        &mut self,
        _: &ToggleCenteredLayout,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.centered_layout = !self.centered_layout;
        if let Some(database_id) = self.database_id() {
            let db = WorkspaceDb::global(cx);
            let centered_layout = self.centered_layout;
            cx.background_spawn(async move {
                db.set_centered_layout(database_id, centered_layout).await
            })
            .detach_and_log_err(cx);
        }
        cx.notify();
    }

    pub fn clear_bookmarks(&mut self, _: &ClearBookmarks, _: &mut Window, cx: &mut Context<Self>) {
        self.project()
            .read(cx)
            .bookmark_store()
            .update(cx, |bookmark_store, cx| {
                bookmark_store.clear_bookmarks(cx);
            });
    }

    pub fn cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if cx.stop_active_drag(window) {
        } else if let Some((notification_id, _)) = self.notifications.pop() {
            dismiss_app_notification(&notification_id, cx);
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn toggle_edit_predictions_all_files(
        &mut self,
        _: &ToggleEditPrediction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fs = self.project().read(cx).fs().clone();
        let show_edit_predictions = all_language_settings(None, cx).show_edit_predictions(None, cx);
        update_settings_file(fs, cx, move |file, _| {
            file.project.all_languages.defaults.show_edit_predictions = Some(!show_edit_predictions)
        });
    }

    pub(crate) fn toggle_theme_mode(
        &mut self,
        _: &ToggleMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_mode = ThemeSettings::get_global(cx).theme.mode();
        let next_mode = match current_mode {
            Some(theme_settings::ThemeAppearanceMode::Light) => {
                theme_settings::ThemeAppearanceMode::Dark
            }
            Some(theme_settings::ThemeAppearanceMode::Dark) => {
                theme_settings::ThemeAppearanceMode::Light
            }
            Some(theme_settings::ThemeAppearanceMode::System) | None => {
                match cx.theme().appearance() {
                    theme::Appearance::Light => theme_settings::ThemeAppearanceMode::Dark,
                    theme::Appearance::Dark => theme_settings::ThemeAppearanceMode::Light,
                }
            }
        };

        let fs = self.project().read(cx).fs().clone();
        settings::update_settings_file(fs, cx, move |settings, _cx| {
            theme_settings::set_mode(settings, next_mode);
        });
    }

    pub fn show_worktree_trust_security_modal(
        &mut self,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(security_modal) = self.active_modal::<SecurityModal>(cx) {
            if toggle {
                security_modal.update(cx, |security_modal, cx| {
                    security_modal.dismiss(cx);
                })
            } else {
                security_modal.update(cx, |security_modal, cx| {
                    security_modal.refresh_restricted_paths(cx);
                });
            }
        } else {
            let has_restricted_worktrees = TrustedWorktrees::has_restricted_worktrees(
                &self.project().read(cx).worktree_store(),
                cx,
            );
            if has_restricted_worktrees {
                let project = self.project().read(cx);
                let remote_host = project
                    .remote_connection_options(cx)
                    .map(RemoteHostLocation::from);
                let worktree_store = project.worktree_store().downgrade();
                self.toggle_modal(window, cx, |window, cx| {
                    SecurityModal::new(worktree_store, remote_host, window, cx)
                });
            }
        }
    }
}
