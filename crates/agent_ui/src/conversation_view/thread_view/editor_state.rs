use super::*;

impl ThreadView {
    pub fn expand_message_editor(
        &mut self,
        _: &ExpandMessageEditor,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.list_state.item_count() == 0 {
            return;
        }
        self.set_editor_is_expanded(!self.editor_expanded, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub fn set_editor_is_expanded(&mut self, is_expanded: bool, cx: &mut Context<Self>) {
        self.editor_expanded = is_expanded;
        self.message_editor.update(cx, |editor, cx| {
            if is_expanded {
                editor.set_mode(
                    EditorMode::Full {
                        scale_ui_elements_with_buffer_font_size: false,
                        show_active_line_background: false,
                        sizing_behavior: SizingBehavior::ExcludeOverscrollMargin,
                    },
                    cx,
                )
            } else {
                let agent_settings = AgentSettings::get_global(cx);
                editor.set_mode(
                    EditorMode::AutoHeight {
                        min_lines: agent_settings.message_editor_min_lines,
                        max_lines: Some(agent_settings.set_message_editor_max_lines()),
                    },
                    cx,
                )
            }
        });
        cx.notify();
    }

    pub fn handle_title_editor_event(
        &mut self,
        title_editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::BufferEdited => {
                if !title_editor.read(cx).is_focused(window) {
                    return;
                }

                let new_title = title_editor.read(cx).text(cx);
                if new_title.is_empty() {
                    return;
                }
                self.apply_renamed_title(SharedString::from(new_title), cx);
            }
            EditorEvent::Blurred => {
                if title_editor.read(cx).text(cx).is_empty() {
                    title_editor.update(cx, |editor, cx| {
                        editor.set_text(DEFAULT_THREAD_TITLE, window, cx);
                    });
                }
            }
            _ => {}
        }
    }

    pub fn rename(&mut self, title: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        if self.title_editor.read(cx).text(cx) != title.as_ref() {
            self.title_editor.update(cx, |editor, cx| {
                editor.set_text(title.clone(), window, cx);
            });
        }
        self.apply_renamed_title(title, cx);
    }

    fn apply_renamed_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        if let Some(store) = ThreadMetadataStore::try_global(cx)
            && !self.is_subagent()
        {
            let thread_id = self.root_thread_id;
            store.update(cx, |store, cx| {
                store.set_title_override(thread_id, title.clone(), cx);
            });
        }
        self.thread.update(cx, |thread, cx| {
            if thread.can_set_title(cx) {
                thread.set_title(title, cx).detach_and_log_err(cx);
            }
        });
    }

    pub fn cancel_editing(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.editing_message.take()
            && let Some(editor) = &self
                .entry_view_state
                .read(cx)
                .entry(index)
                .and_then(|e| e.message_editor())
                .cloned()
        {
            editor.update(cx, |editor, cx| {
                if let Some(user_message) = self
                    .thread
                    .read(cx)
                    .entries()
                    .get(index)
                    .and_then(|e| e.user_message())
                {
                    editor.set_message(user_message.chunks.clone(), window, cx);
                }
            })
        };
        self.message_editor.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
}
