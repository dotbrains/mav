use super::*;

impl<T: 'static> Focusable for PromptEditor<T> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl<T: 'static> PromptEditor<T> {
    const MAX_LINES: u8 = 8;

    pub(super) fn codegen_status<'a>(&'a self, cx: &'a App) -> &'a CodegenStatus {
        match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => codegen.read(cx).status(cx),
            PromptEditorMode::Terminal { codegen, .. } => &codegen.read(cx).status,
        }
    }

    pub(super) fn subscribe_to_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_subscriptions.clear();
        self.editor_subscriptions.push(cx.subscribe_in(
            &self.editor,
            window,
            Self::handle_prompt_editor_events,
        ));
    }

    pub(super) fn assign_completion_provider(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_completion_provider(Some(Rc::new(PromptCompletionProvider::new(
                PromptEditorCompletionProviderDelegate,
                cx.weak_entity(),
                self.mention_set.clone(),
                self.workspace.clone(),
            ))));
        });
    }

    pub fn set_show_cursor_when_unfocused(
        &mut self,
        show_cursor_when_unfocused: bool,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.set_show_cursor_when_unfocused(show_cursor_when_unfocused, cx)
        });
    }

    pub fn unlink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.prompt(cx);
        let existing_creases = self.editor.update(cx, |editor, cx| {
            extract_message_creases(editor, &self.mention_set, window, cx)
        });
        let focus = self.editor.focus_handle(cx).contains_focused(window, cx);
        let mut creases = vec![];
        self.editor = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, Self::MAX_LINES as usize, window, cx);
            editor.set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
            editor.set_placeholder_text("Add a prompt…", window, cx);
            editor.set_text(prompt, window, cx);
            creases = insert_message_creases(&mut editor, &existing_creases, window, cx);

            if focus {
                window.focus(&editor.focus_handle(cx), cx);
            }
            editor
        });

        self.mention_set.update(cx, |mention_set, _cx| {
            debug_assert_eq!(
                creases.len(),
                mention_set.creases().len(),
                "Missing creases"
            );

            let mentions = mention_set
                .clear()
                .zip(creases)
                .map(|((_, value), id)| (id, value))
                .collect::<HashMap<_, _>>();
            mention_set.set_mentions(mentions);
        });

        self.assign_completion_provider(cx);
        self.subscribe_to_editor(window, cx);
    }

    pub fn placeholder_text(mode: &PromptEditorMode, window: &mut Window, cx: &mut App) -> String {
        let action = match mode {
            PromptEditorMode::Buffer { codegen, .. } => {
                if codegen.read(cx).is_insertion {
                    "Generate"
                } else {
                    "Transform"
                }
            }
            PromptEditorMode::Terminal { .. } => "Generate",
        };

        let agent_panel_keybinding =
            ui::text_for_action(&mav_actions::assistant::ToggleFocus, window, cx)
                .map(|keybinding| format!("{keybinding} to chat"))
                .unwrap_or_default();

        format!("{action}… ({agent_panel_keybinding} ― ↓↑ for history — @ to include context)")
    }

    pub fn prompt(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if inline_assistant_model_supports_images(cx)
            && let Some(task) = paste_images_as_context(
                self.editor.clone(),
                self.mention_set.clone(),
                self.workspace.clone(),
                window,
                cx,
            )
        {
            task.detach();
        }
    }

    fn handle_prompt_editor_events(
        &mut self,
        editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Edited { .. } => {
                let snapshot = editor.update(cx, |editor, cx| editor.snapshot(window, cx));

                self.mention_set
                    .update(cx, |mention_set, _cx| mention_set.remove_invalid(&snapshot));

                if let Some(workspace) = Workspace::for_window(window, cx) {
                    workspace.update(cx, |workspace, cx| {
                        let is_via_ssh = workspace.project().read(cx).is_via_remote_server();

                        workspace
                            .client()
                            .telemetry()
                            .log_edit_event("inline assist", is_via_ssh);
                    });
                }
                let prompt = snapshot.text();
                if self
                    .prompt_history_ix
                    .is_none_or(|ix| self.prompt_history[ix] != prompt)
                {
                    self.prompt_history_ix.take();
                    self.pending_prompt = prompt;
                }

                self.edited_since_done = true;
                self.session_state.completion = CompletionState::Pending;
                cx.notify();
            }
            EditorEvent::Blurred => {
                if self.show_rate_limit_notice {
                    self.show_rate_limit_notice = false;
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    pub fn is_completions_menu_visible(&self, cx: &App) -> bool {
        self.editor
            .read(cx)
            .context_menu()
            .borrow()
            .as_ref()
            .is_some_and(|menu| matches!(menu, CodeContextMenu::Completions(_)) && menu.visible())
    }

    pub fn trigger_completion_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let menu_is_open = editor.context_menu().borrow().as_ref().is_some_and(|menu| {
                matches!(menu, CodeContextMenu::Completions(_)) && menu.visible()
            });

            let has_at_sign = {
                let snapshot = editor.display_snapshot(cx);
                let cursor = editor.selections.newest::<text::Point>(&snapshot).head();
                let offset = cursor.to_offset(&snapshot);
                if offset.0 > 0 {
                    snapshot
                        .buffer_snapshot()
                        .reversed_chars_at(offset)
                        .next()
                        .map(|sign| sign == '@')
                        .unwrap_or(false)
                } else {
                    false
                }
            };

            if menu_is_open && has_at_sign {
                return;
            }

            editor.insert("@", window, cx);
            editor.show_completions(&editor::actions::ShowCompletions, window, cx);
        });
    }

    pub(super) fn cancel(
        &mut self,
        _: &editor::actions::Cancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.codegen_status(cx) {
            CodegenStatus::Idle | CodegenStatus::Done | CodegenStatus::Error(_) => {
                cx.emit(PromptEditorEvent::CancelRequested);
            }
            CodegenStatus::Pending => {
                cx.emit(PromptEditorEvent::StopRequested);
            }
        }
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_confirm(false, cx);
    }

    pub(super) fn secondary_confirm(
        &mut self,
        _: &menu::SecondaryConfirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let execute = matches!(self.mode, PromptEditorMode::Terminal { .. });
        self.handle_confirm(execute, cx);
    }

    pub(super) fn handle_confirm(&mut self, execute: bool, cx: &mut Context<Self>) {
        match self.codegen_status(cx) {
            CodegenStatus::Idle => {
                self.fire_started_telemetry(cx);
                cx.emit(PromptEditorEvent::StartRequested);
            }
            CodegenStatus::Pending => {}
            CodegenStatus::Done => {
                if self.edited_since_done {
                    self.fire_started_telemetry(cx);
                    cx.emit(PromptEditorEvent::StartRequested);
                } else {
                    cx.emit(PromptEditorEvent::ConfirmRequested { execute });
                }
            }
            CodegenStatus::Error(_) => {
                self.fire_started_telemetry(cx);
                cx.emit(PromptEditorEvent::StartRequested);
            }
        }
    }

    fn fire_started_telemetry(&self, cx: &Context<Self>) {
        let Some(model) = LanguageModelRegistry::read_global(cx).inline_assistant_model() else {
            return;
        };

        let model_telemetry_id = model.model.telemetry_id();
        let model_provider_id = model.provider.id().to_string();

        let (kind, language_name) = match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => {
                let codegen = codegen.read(cx);
                (
                    "inline",
                    codegen.language_name(cx).map(|name| name.to_string()),
                )
            }
            PromptEditorMode::Terminal { .. } => ("inline_terminal", None),
        };

        telemetry::event!(
            "Assistant Started",
            session_id = self.session_state.session_id.to_string(),
            kind = kind,
            phase = "started",
            model = model_telemetry_id,
            model_provider = model_provider_id,
            language_name = language_name,
        );
    }

    pub(super) fn move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.prompt_history_ix {
            if ix > 0 {
                self.prompt_history_ix = Some(ix - 1);
                let prompt = self.prompt_history[ix - 1].as_str();
                self.editor.update(cx, |editor, cx| {
                    editor.set_text(prompt, window, cx);
                    editor.move_to_beginning(&Default::default(), window, cx);
                });
            }
        } else if !self.prompt_history.is_empty() {
            self.prompt_history_ix = Some(self.prompt_history.len() - 1);
            let prompt = self.prompt_history[self.prompt_history.len() - 1].as_str();
            self.editor.update(cx, |editor, cx| {
                editor.set_text(prompt, window, cx);
                editor.move_to_beginning(&Default::default(), window, cx);
            });
        }
    }

    pub(super) fn move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.prompt_history_ix {
            if ix < self.prompt_history.len() - 1 {
                self.prompt_history_ix = Some(ix + 1);
                let prompt = self.prompt_history[ix + 1].as_str();
                self.editor.update(cx, |editor, cx| {
                    editor.set_text(prompt, window, cx);
                    editor.move_to_end(&Default::default(), window, cx)
                });
            } else {
                self.prompt_history_ix = None;
                let prompt = self.pending_prompt.as_str();
                self.editor.update(cx, |editor, cx| {
                    editor.set_text(prompt, window, cx);
                    editor.move_to_end(&Default::default(), window, cx)
                });
            }
        }
    }
}
