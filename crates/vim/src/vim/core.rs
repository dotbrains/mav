use super::*;

impl Vim {
    pub fn new(window: &mut Window, cx: &mut Context<Editor>) -> Entity<Self> {
        let editor = cx.entity();

        let initial_vim_mode = VimSettings::get_global(cx).default_mode;
        let (mode, last_mode) = if HelixModeSetting::get_global(cx).0 {
            let initial_helix_mode = match initial_vim_mode {
                Mode::Normal => Mode::HelixNormal,
                Mode::Insert => Mode::Insert,
                // Otherwise, we panic with a note that we should never get there due to the
                // possible values of VimSettings::get_global(cx).default_mode being either Mode::Normal or Mode::Insert.
                _ => unreachable!("Invalid default mode"),
            };
            (initial_helix_mode, Mode::HelixNormal)
        } else {
            (initial_vim_mode, Mode::Normal)
        };

        cx.new(|cx| Vim {
            mode,
            last_mode,
            temp_mode: false,
            exit_temporary_mode: false,
            operator_stack: Vec::new(),
            replacements: Vec::new(),

            stored_visual_mode: None,
            current_tx: None,
            undo_last_line_tx: None,
            current_anchor: None,
            extended_pending_selection_id: None,
            undo_modes: HashMap::default(),

            status_label: None,
            selected_register: None,
            search: SearchState::default(),

            last_command: None,
            running_command: None,

            editor: editor.downgrade(),
            _subscriptions: vec![
                cx.observe_keystrokes(Self::observe_keystrokes),
                cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
                    this.handle_editor_event(event, window, cx)
                }),
            ],
        })
    }

    pub fn action<A: Action>(
        editor: &mut Editor,
        cx: &mut Context<Vim>,
        f: impl Fn(&mut Vim, &A, &mut Window, &mut Context<Vim>) + 'static,
    ) {
        let subscription = editor.register_action(cx.listener(move |vim, action, window, cx| {
            if !Vim::globals(cx).dot_replaying {
                if vim.status_label.take().is_some() {
                    cx.notify();
                }
            }
            f(vim, action, window, cx);
        }));
        cx.on_release(|_, _| drop(subscription)).detach();
    }

    pub fn editor(&self) -> Option<Entity<Editor>> {
        self.editor.upgrade()
    }

    pub fn workspace(&self, window: &Window, cx: &App) -> Option<Entity<Workspace>> {
        Workspace::for_window(window, cx)
    }

    pub fn pane(&self, window: &Window, cx: &Context<Self>) -> Option<Entity<Pane>> {
        let pane = self
            .workspace(window, cx)
            .map(|workspace| workspace.read(cx).focused_pane(window, cx))?;
        // `focused_pane` falls back to the center pane when a dock panel
        // without its own pane (e.g. the Agent panel) has focus. Guard
        // against that so vim search/match commands don't steal focus.
        if pane.read(cx).focus_handle(cx).contains_focused(window, cx) {
            Some(pane)
        } else {
            None
        }
    }

    pub fn enabled(cx: &mut App) -> bool {
        VimModeSetting::get_global(cx).0 || HelixModeSetting::get_global(cx).0
    }

    /// Called whenever an keystroke is typed so vim can observe all actions
    /// and keystrokes accordingly.
    pub(super) fn observe_keystrokes(
        &mut self,
        keystroke_event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.exit_temporary_mode {
            self.exit_temporary_mode = false;
            // Don't switch to insert mode if the action is temporary_normal.
            if let Some(action) = keystroke_event.action.as_ref()
                && action.as_any().downcast_ref::<TemporaryNormal>().is_some()
            {
                return;
            }
            self.switch_mode(Mode::Insert, false, window, cx)
        }
        if let Some(action) = keystroke_event.action.as_ref() {
            // Keystroke is handled by the vim system, so continue forward
            if action.name().starts_with("vim::") {
                return;
            }
        } else if window.has_pending_keystrokes() || keystroke_event.keystroke.is_ime_in_progress()
        {
            return;
        }

        if let Some(operator) = self.active_operator() {
            match operator {
                Operator::Literal { prefix } => {
                    self.handle_literal_keystroke(
                        keystroke_event,
                        prefix.unwrap_or_default(),
                        window,
                        cx,
                    );
                }
                _ if !operator.is_waiting(self.mode) => {
                    self.clear_operator(window, cx);
                    self.stop_recording_immediately(Box::new(ClearOperators), cx)
                }
                _ => {}
            }
        }
    }

    pub(super) fn handle_editor_event(
        &mut self,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Focused => self.focused(true, window, cx),
            EditorEvent::Blurred => self.blurred(window, cx),
            EditorEvent::SelectionsChanged { local: true } => {
                self.local_selections_changed(window, cx);
            }
            EditorEvent::InputIgnored { text } => {
                self.input_ignored(text.clone(), window, cx);
                Vim::globals(cx).observe_insertion(text, None)
            }
            EditorEvent::InputHandled {
                text,
                utf16_range_to_replace: range_to_replace,
            } => Vim::globals(cx).observe_insertion(text, range_to_replace.clone()),
            EditorEvent::TransactionBegun { transaction_id } => {
                self.transaction_begun(*transaction_id, window, cx)
            }
            EditorEvent::TransactionUndone { transaction_id } => {
                self.transaction_undone(transaction_id, window, cx)
            }
            EditorEvent::Edited { .. } => self.push_to_change_list(window, cx),
            EditorEvent::FocusedIn => self.sync_vim_settings(window, cx),
            EditorEvent::CursorShapeChanged => self.cursor_shape_changed(window, cx),
            EditorEvent::PushedToNavHistory {
                anchor,
                is_deactivate,
            } => {
                self.update_editor(cx, |vim, editor, cx| {
                    let mark = if *is_deactivate {
                        "\"".to_string()
                    } else {
                        "'".to_string()
                    };
                    vim.set_mark(mark, vec![*anchor], editor.buffer(), window, cx);
                });
            }
            _ => {}
        }
    }
}
