use super::*;

impl SkillCreatorPage {
    pub(crate) fn name_editor_focus_handle(&self, cx: &App) -> FocusHandle {
        self.name_editor.focus_handle(cx)
    }

    fn handle_url_input_event(
        &mut self,
        event: &ErasedEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, ErasedEditorEvent::BufferEdited) {
            return;
        }

        // Convention from `thread_view::handle_title_editor_event` and
        // `agent_panel::handle_terminal_title_editor_event`: programmatic
        // `set_text` is performed while the editor is unfocused, so the
        // focus check filters synthesized `BufferEdited` events out of
        // the user-edit path without needing a one-shot suppression flag.
        if !self.url_editor.focus_handle(cx).is_focused(window) {
            return;
        }

        self.save_error = None;
        self.schedule_url_import(window, cx);
    }

    fn handle_name_input_event(
        &mut self,
        event: &ErasedEditorEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ErasedEditorEvent::BufferEdited) {
            self.recompute_name_error(cx);
            self.save_error = None;
            cx.notify();
        }
    }

    fn handle_description_input_event(
        &mut self,
        event: &ErasedEditorEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ErasedEditorEvent::BufferEdited) {
            self.recompute_description_error(cx);
            self.save_error = None;
            cx.notify();
        }
    }

    fn handle_body_editor_event(
        &mut self,
        _: &Entity<Editor>,
        event: &EditorEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, EditorEvent::BufferEdited) {
            self.recompute_body_error(cx);
            self.save_error = None;
            cx.notify();
        }
    }

    fn current_name(&self, cx: &App) -> String {
        self.name_editor.read(cx).text(cx)
    }

    fn current_description(&self, cx: &App) -> String {
        self.description_editor.read(cx).text(cx)
    }

    fn current_body(&self, cx: &App) -> String {
        self.body_editor.read(cx).text(cx)
    }

    fn current_url(&self, cx: &App) -> String {
        self.url_editor.read(cx).text(cx)
    }

    fn recompute_name_error(&mut self, cx: &mut Context<Self>) {
        let name = self.current_name(cx);
        let error = validate_name(&name).err();
        self.name_error = error;
        self.name_editor
            .update(cx, |field, cx| field.set_error(error, cx));
    }

    fn recompute_description_error(&mut self, cx: &mut Context<Self>) {
        let description = self.current_description(cx);
        self.description_length = description.len();
        let error = validate_description(&description).err();
        self.description_error = error;
        self.description_editor
            .update(cx, |field, cx| field.set_error(error, cx));
    }

    fn recompute_body_error(&mut self, cx: &App) {
        let body = self.current_body(cx);
        self.body_error = if body.trim().is_empty() {
            Some("Body is required.")
        } else {
            None
        };
    }

    fn is_valid(&self, cx: &App) -> bool {
        validate_name(&self.current_name(cx)).is_ok()
            && validate_description(&self.current_description(cx)).is_ok()
            && !self.current_body(cx).trim().is_empty()
    }

    pub(crate) fn apply_open_mode(
        &mut self,
        open_mode: SkillCreatorOpenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match open_mode {
            SkillCreatorOpenMode::Form => {}
            SkillCreatorOpenMode::Url { initial_url } => {
                self.open_url_import(initial_url, window, cx);
            }
            SkillCreatorOpenMode::Install { content } => {
                self.open_install_review(content, window, cx);
            }
        }
    }
}
