use super::*;

impl Editor {
    pub fn show_local_cursors(&self, window: &mut Window, cx: &mut App) -> bool {
        (self.read_only(cx) || self.blink_manager.read(cx).visible())
            && self.focus_handle.is_focused(window)
    }

    pub fn set_show_cursor_when_unfocused(&mut self, is_enabled: bool, cx: &mut Context<Self>) {
        self.show_cursor_when_unfocused = is_enabled;
        cx.notify();
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub(crate) fn handle_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::Focused);

        if let Some(descendant) = self
            .last_focused_descendant
            .take()
            .and_then(|descendant| descendant.upgrade())
        {
            window.focus(&descendant, cx);
        } else {
            if let Some(blame) = self.blame.as_ref() {
                blame.update(cx, GitBlame::focus)
            }

            self.blink_manager.update(cx, BlinkManager::enable);
            self.show_cursor_names(window, cx);
            self.buffer.update(cx, |buffer, cx| {
                buffer.finalize_last_transaction(cx);
                if self.leader_id.is_none() {
                    buffer.set_active_selections(
                        &self.selections.disjoint_anchors_arc(),
                        self.selections.line_mode(),
                        self.cursor_shape,
                        cx,
                    );
                }
            });

            if cx.is_cursor_visible()
                && let Some(position_map) = self.last_position_map.clone()
            {
                EditorElement::mouse_moved(
                    self,
                    &MouseMoveEvent {
                        position: window.mouse_position(),
                        pressed_button: None,
                        modifiers: window.modifiers(),
                    },
                    &position_map,
                    None,
                    window,
                    cx,
                );
            }
        }
    }

    pub(crate) fn handle_focus_in(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::FocusedIn)
    }

    pub(crate) fn handle_focus_out(
        &mut self,
        event: FocusOutEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.blurred != self.focus_handle {
            self.last_focused_descendant = Some(event.blurred);
        }
        self.selection_drag_state = SelectionDragState::None;
        self.refresh_inlay_hints(InlayHintRefreshReason::ModifiersChanged(false), cx);
    }

    pub(crate) fn handle_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.blink_manager.update(cx, BlinkManager::disable);
        self.buffer
            .update(cx, |buffer, cx| buffer.remove_active_selections(cx));

        if let Some(blame) = self.blame.as_ref() {
            blame.update(cx, GitBlame::blur)
        }
        if !self.hover_state.focused(window, cx) {
            hide_hover(self, cx);
        }
        if !self
            .context_menu
            .borrow()
            .as_ref()
            .is_some_and(|context_menu| context_menu.focused(window, cx))
        {
            self.hide_context_menu(window, cx);
        }
        self.take_active_edit_prediction(true, cx);
        cx.emit(EditorEvent::Blurred);
        cx.notify();
    }

    pub fn register_action_renderer(
        &mut self,
        listener: impl Fn(&Editor, &mut Window, &mut Context<Editor>) + 'static,
    ) -> Subscription {
        let id = self.next_editor_action_id.post_inc();
        self.editor_actions
            .borrow_mut()
            .insert(id, Box::new(listener));

        let editor_actions = self.editor_actions.clone();
        Subscription::new(move || {
            editor_actions.borrow_mut().remove(&id);
        })
    }

    pub fn register_action<A: Action>(
        &mut self,
        listener: impl Fn(&A, &mut Window, &mut App) + 'static,
    ) -> Subscription {
        let id = self.next_editor_action_id.post_inc();
        let listener = Arc::new(listener);
        self.editor_actions.borrow_mut().insert(
            id,
            Box::new(move |_, window, _| {
                let listener = listener.clone();
                window.on_action(TypeId::of::<A>(), move |action, phase, window, cx| {
                    let action = action.downcast_ref().unwrap();
                    if phase == DispatchPhase::Bubble {
                        listener(action, window, cx)
                    }
                })
            }),
        );

        let editor_actions = self.editor_actions.clone();
        Subscription::new(move || {
            editor_actions.borrow_mut().remove(&id);
        })
    }
}
