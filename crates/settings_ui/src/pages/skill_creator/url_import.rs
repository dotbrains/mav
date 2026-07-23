use super::*;

impl SkillCreatorPage {
    /// Pre-fill the form with a skill supplied inline (from a share link) so
    /// the recipient can review it before saving. Unlike URL import, this
    /// doesn't touch the URL editor or perform any network request.
    fn open_install_review(
        &mut self,
        content: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.url_import_debounce_task = None;
        self.url_import_task = None;
        self.url_import_status = UrlImportStatus::Idle;

        match parse_imported_skill(&content, "") {
            Ok(imported) => self.apply_imported_skill(imported, window, cx),
            Err(err) => {
                self.save_error = Some(SharedString::from(format!(
                    "Couldn't read shared skill: {err}"
                )));
                cx.notify();
            }
        }
    }

    fn open_url_import(
        &mut self,
        initial_url: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_error = None;
        self.url_import_debounce_task = None;
        self.url_import_task = None;
        self.url_import_status = UrlImportStatus::Idle;

        let text = initial_url.unwrap_or_default();
        let should_fetch = !text.is_empty();
        let needs_set_text = should_fetch || !self.current_url(cx).is_empty();
        if !needs_set_text {
            // No text to write and nothing to clear: just move focus.
            window.focus(&self.url_editor.focus_handle(cx), cx);
            cx.notify();
            return;
        }

        // Defer so the programmatic `set_text` runs before we move focus
        // to the URL editor. `handle_url_input_event` uses
        // `url_editor.is_focused(window)` to distinguish user edits from
        // programmatic ones, so writing while unfocused is what keeps the
        // synthesized `BufferEdited` from being treated as a user edit.
        let skill_creator = cx.weak_entity();
        let url_editor = self.url_editor.clone();
        window.defer(cx, move |window, cx| {
            url_editor.update(cx, |input, cx| {
                input.set_text(&text, window, cx);
            });
            window.focus(&url_editor.focus_handle(cx), cx);
            if should_fetch {
                skill_creator
                    .update(cx, |this, cx| {
                        this.start_url_import(window, cx);
                    })
                    .log_err();
            }
        });
        cx.notify();
    }

    fn schedule_url_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url_import_debounce_task = None;
        self.url_import_task = None;

        let url = self.current_url(cx).trim().to_string();
        if url.is_empty() {
            self.url_import_status = UrlImportStatus::Idle;
            cx.notify();
            return;
        }

        self.url_import_status = UrlImportStatus::Idle;
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(URL_IMPORT_DEBOUNCE).await;
            this.update_in(cx, |this, window, cx| {
                this.start_url_import(window, cx);
            })
            .log_err();
        });
        self.url_import_debounce_task = Some(task);
        cx.notify();
    }

    fn start_url_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Cancel any pending debounce so the explicit start supersedes it,
        // instead of racing with a timer that's about to fire.
        self.url_import_debounce_task = None;
        self.url_import_task = None;

        let url = self.current_url(cx).trim().to_string();
        if url.is_empty() {
            self.url_import_status = UrlImportStatus::Idle;
            cx.notify();
            return;
        }

        if let Err(err) = github_raw_url(&url) {
            self.url_import_status = UrlImportStatus::Error(SharedString::from(err.to_string()));
            cx.notify();
            return;
        }

        self.url_import_status = UrlImportStatus::Fetching;
        let http_client = self.http_client.clone();
        let fetch_task = cx.background_spawn(fetch_imported_skill_from_url(http_client, url));
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = fetch_task.await;
            this.update_in(cx, |this, window, cx| {
                this.url_import_debounce_task = None;
                this.url_import_task = None;
                match result {
                    Ok(imported) => {
                        this.apply_imported_skill(imported, window, cx);
                    }
                    Err(err) => {
                        this.url_import_status =
                            UrlImportStatus::Error(SharedString::from(err.to_string()));
                        cx.notify();
                    }
                }
            })
            .log_err();
        });
        self.url_import_task = Some(task);
        cx.notify();
    }

    fn apply_imported_skill(
        &mut self,
        imported: ImportedSkill,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.url_import_status = UrlImportStatus::Idle;
        self.save_error = None;

        let name_editor = self.name_editor.clone();
        let description_editor = self.description_editor.clone();
        let body_editor = self.body_editor.clone();
        let skill_creator = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            name_editor.update(cx, |input, cx| {
                input.set_text(&imported.name, window, cx);
            });
            description_editor.update(cx, |input, cx| {
                input.set_text(&imported.description, window, cx);
            });
            body_editor.update(cx, |editor, cx| {
                editor.set_text(imported.body.clone(), window, cx);
            });
            skill_creator
                .update(cx, |this, cx| {
                    this.disable_model_invocation = imported.disable_model_invocation;
                    this.url_import_status = UrlImportStatus::Idle;
                    this.url_import_debounce_task = None;
                    this.url_import_task = None;
                    this.save_error = None;
                    this.recompute_name_error(cx);
                    this.recompute_description_error(cx);
                    this.recompute_body_error(cx);
                    cx.notify();
                })
                .log_err();
            window.focus(&name_editor.focus_handle(cx), cx);
        });
    }
}
