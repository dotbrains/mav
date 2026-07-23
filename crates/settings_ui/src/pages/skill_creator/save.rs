use super::*;

impl SkillCreatorPage {
    fn save_skill(&mut self, _: &SaveSkill, window: &mut Window, cx: &mut Context<Self>) {
        self.recompute_name_error(cx);
        self.recompute_description_error(cx);
        self.recompute_body_error(cx);

        if !self.is_valid(cx) || self.saving {
            cx.notify();
            return;
        }

        // Resolve the scope at save time so the skill is written to whichever
        // settings file is selected at the moment the user clicks Save.
        let scope = self
            .settings_window
            .read_with(cx, |settings_window, cx| {
                scope_for_settings_file(
                    &settings_window.current_file,
                    settings_window.original_window.as_ref(),
                    cx,
                )
            })
            .unwrap_or(ScopeChoice::Global);
        let name = self.current_name(cx);
        let description = self.current_description(cx);
        let body = self.current_body(cx);
        let disable_model_invocation = self.disable_model_invocation;
        let fs = self.fs.clone();

        self.saving = true;
        self.save_error = None;
        cx.notify();

        let task = cx.spawn_in(window, async move |this, cx| {
            let result = write_skill_to_disk(
                fs.as_ref(),
                &scope.skills_dir(),
                &name,
                &description,
                &body,
                disable_model_invocation,
            )
            .await;

            this.update_in(cx, |this, _window, cx| {
                this.saving = false;
                this.save_task = None;
                match result {
                    Ok(_) => {
                        // Rescan skill directories so new skills show up in Settings page right away
                        if let Some(hook) = cx.try_global::<SkillsUpdatedHook>() {
                            let hook = hook.0.clone();
                            hook(cx);
                        }

                        cx.emit(SkillCreatorEvent::Saved);
                    }
                    Err(err) => {
                        this.save_error = Some(SharedString::from(err.to_string()));
                        cx.notify();
                    }
                }
            })
            .log_err();
        });
        self.save_task = Some(task);
    }

    fn cancel(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        // Block dismissal while a save is in flight
        if self.saving {
            return;
        }
        cx.emit(SkillCreatorEvent::Dismissed);
    }

    fn toggle_disable_model_invocation(&mut self, cx: &mut Context<Self>) {
        self.disable_model_invocation = !self.disable_model_invocation;
        cx.notify();
    }

    fn focus_next_field(
        &mut self,
        _: &FocusNextField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
    }

    fn focus_previous_field(
        &mut self,
        _: &FocusPreviousField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
    }

    fn on_menu_next(&mut self, _: &menu::SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_next(cx);
    }

    fn on_menu_prev(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
    }
}
