use super::*;

impl AgentPanel {
    pub(super) fn edit_terminal_title(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.terminals.get_mut(&terminal_id) else {
            return;
        };

        if let Some(title_editor) = terminal.title_editor.as_ref() {
            title_editor.focus_handle(cx).focus(window, cx);
            return;
        }

        let title = terminal.editable_title(cx).to_string();
        let title_editor_initial_title = title.clone();
        let title_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(title, window, cx);
            editor
        });
        let title_editor_subscription = cx.subscribe_in(
            &title_editor,
            window,
            move |this, title_editor, event: &editor::EditorEvent, window, cx| {
                this.handle_terminal_title_editor_event(
                    terminal_id,
                    title_editor,
                    event,
                    window,
                    cx,
                );
            },
        );
        title_editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor.focus_handle(cx).focus(window, cx);
        });
        terminal.title_editor = Some(title_editor);
        terminal.title_editor_initial_title = Some(title_editor_initial_title);
        terminal.title_editor_subscription = Some(title_editor_subscription);
        cx.notify();
    }

    pub(super) fn stop_editing_terminal_title(
        &mut self,
        terminal_id: TerminalId,
        focus_terminal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.terminals.get_mut(&terminal_id) else {
            return;
        };
        let terminal_view = terminal.view.clone();
        terminal.title_editor = None;
        terminal.title_editor_initial_title = None;
        terminal.title_editor_subscription = None;
        let title_changed = terminal.refresh_title(cx);

        if focus_terminal {
            terminal_view.focus_handle(cx).focus(window, cx);
        }
        if title_changed {
            cx.emit(AgentPanelEvent::EntryChanged);
        }
        cx.notify();
    }

    pub(super) fn handle_terminal_title_editor_event(
        &mut self,
        terminal_id: TerminalId,
        title_editor: &Entity<Editor>,
        event: &editor::EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            editor::EditorEvent::BufferEdited => {
                if !title_editor.read(cx).is_focused(window) {
                    return;
                }
                let Some((terminal_view, initial_title, terminal_title)) =
                    self.terminals.get(&terminal_id).and_then(|terminal| {
                        terminal
                            .title_editor
                            .as_ref()
                            .is_some_and(|current_editor| current_editor == title_editor)
                            .then(|| {
                                (
                                    terminal.view.clone(),
                                    terminal.title_editor_initial_title.clone(),
                                    terminal.terminal_title(cx),
                                )
                            })
                    })
                else {
                    return;
                };
                let new_title = title_editor.read(cx).text(cx);
                if initial_title.as_deref() == Some(new_title.as_str()) {
                    return;
                }
                let label = if new_title.trim().is_empty()
                    || new_title == terminal_title_without_prefix(terminal_title.as_ref())
                {
                    None
                } else {
                    Some(new_title)
                };

                cx.defer(move |cx| {
                    terminal_view.update(cx, |terminal_view, cx| {
                        terminal_view.set_custom_title(label, cx);
                    });
                });
            }
            editor::EditorEvent::Blurred => {
                if self
                    .terminals
                    .get(&terminal_id)
                    .and_then(|terminal| terminal.title_editor.as_ref())
                    .is_some_and(|current_editor| current_editor == title_editor)
                {
                    self.stop_editing_terminal_title(terminal_id, false, window, cx);
                }
            }
            _ => {}
        }
    }
}
