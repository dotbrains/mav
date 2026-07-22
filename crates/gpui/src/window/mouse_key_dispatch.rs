use super::*;

impl Window {
    pub(super) fn dispatch_mouse_event(&mut self, event: &dyn Any, cx: &mut App) {
        let hit_test = self.rendered_frame.hit_test(self.mouse_position());
        if hit_test != self.mouse_hit_test {
            self.mouse_hit_test = hit_test;
            self.reset_cursor_style(cx);
        }

        #[cfg(any(feature = "inspector", debug_assertions))]
        if self.is_inspector_picking(cx) {
            self.handle_inspector_mouse_event(event, cx);
            // When inspector is picking, all other mouse handling is skipped.
            return;
        }

        let mut mouse_listeners = mem::take(&mut self.rendered_frame.mouse_listeners);

        // Capture phase, events bubble from back to front. Handlers for this phase are used for
        // special purposes, such as detecting events outside of a given Bounds.
        for listener in &mut mouse_listeners {
            let listener = listener.as_mut().unwrap();
            listener(event, DispatchPhase::Capture, self, cx);
            if !cx.propagate_event {
                break;
            }
        }

        // Bubble phase, where most normal handlers do their work.
        if cx.propagate_event {
            for listener in mouse_listeners.iter_mut().rev() {
                let listener = listener.as_mut().unwrap();
                listener(event, DispatchPhase::Bubble, self, cx);
                if !cx.propagate_event {
                    break;
                }
            }
        }

        self.rendered_frame.mouse_listeners = mouse_listeners;

        if cx.has_active_drag() {
            if event.is::<MouseMoveEvent>() {
                // If this was a mouse move event, redraw the window so that the
                // active drag can follow the mouse cursor.
                self.refresh();
            } else if event.is::<MouseUpEvent>() {
                // If this was a mouse up event, cancel the active drag and redraw
                // the window.
                cx.active_drag = None;
                self.refresh();
            }
        }

        // Auto-release pointer capture on mouse up
        if event.is::<MouseUpEvent>() && self.captured_hitbox.is_some() {
            self.captured_hitbox = None;
        }
    }

    pub(super) fn dispatch_key_event(&mut self, event: &dyn Any, cx: &mut App) {
        if self.invalidator.is_dirty() {
            self.draw(cx).clear();
        }

        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let dispatch_path = self.rendered_frame.dispatch_tree.dispatch_path(node_id);

        let mut keystroke: Option<Keystroke> = None;

        if let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() {
            if event.modifiers.number_of_modifiers() == 0
                && self.pending_modifier.modifiers.number_of_modifiers() == 1
                && !self.pending_modifier.saw_keystroke
            {
                let key = match self.pending_modifier.modifiers {
                    modifiers if modifiers.shift => Some("shift"),
                    modifiers if modifiers.control => Some("control"),
                    modifiers if modifiers.alt => Some("alt"),
                    modifiers if modifiers.platform => Some("platform"),
                    modifiers if modifiers.function => Some("function"),
                    _ => None,
                };
                if let Some(key) = key {
                    keystroke = Some(Keystroke {
                        key: key.to_string(),
                        key_char: None,
                        modifiers: Modifiers::default(),
                    });
                }
            }

            if self.pending_modifier.modifiers.number_of_modifiers() == 0
                && event.modifiers.number_of_modifiers() == 1
            {
                self.pending_modifier.saw_keystroke = false
            }
            self.pending_modifier.modifiers = event.modifiers
        } else if let Some(key_down_event) = event.downcast_ref::<KeyDownEvent>() {
            self.pending_modifier.saw_keystroke = true;
            keystroke = Some(key_down_event.keystroke.clone());
            if key_down_event.keystroke.key_char.is_some()
                && matches!(
                    cx.cursor_hide_mode,
                    CursorHideMode::OnTyping | CursorHideMode::OnTypingAndAction
                )
            {
                cx.platform.hide_cursor_until_mouse_moves();
            }
        }

        let Some(keystroke) = keystroke else {
            self.finish_dispatch_key_event(event, dispatch_path, self.context_stack(), cx);
            return;
        };

        cx.propagate_event = true;
        self.dispatch_keystroke_interceptors(event, self.context_stack(), cx);
        if !cx.propagate_event {
            self.finish_dispatch_key_event(event, dispatch_path, self.context_stack(), cx);
            return;
        }

        let mut currently_pending = self.pending_input.take().unwrap_or_default();
        if currently_pending.focus.is_some() && currently_pending.focus != self.focus {
            currently_pending = PendingInput::default();
        }

        let match_result = self.rendered_frame.dispatch_tree.dispatch_key(
            currently_pending.keystrokes,
            keystroke,
            &dispatch_path,
        );

        if !match_result.to_replay.is_empty() {
            self.replay_pending_input(match_result.to_replay, cx);
            cx.propagate_event = true;
        }

        if !match_result.pending.is_empty() {
            currently_pending.timer.take();
            currently_pending.keystrokes = match_result.pending;
            currently_pending.focus = self.focus;

            let text_input_requires_timeout = event
                .downcast_ref::<KeyDownEvent>()
                .filter(|key_down| key_down.keystroke.key_char.is_some())
                .and_then(|_| self.platform_window.take_input_handler())
                .map_or(false, |mut input_handler| {
                    let accepts = input_handler.accepts_text_input(self, cx);
                    self.platform_window.set_input_handler(input_handler);
                    accepts
                });

            currently_pending.needs_timeout |=
                match_result.pending_has_binding || text_input_requires_timeout;

            if currently_pending.needs_timeout {
                currently_pending.timer = Some(self.spawn(cx, async move |cx| {
                    cx.background_executor.timer(Duration::from_secs(1)).await;
                    cx.update(move |window, cx| {
                        let Some(currently_pending) = window
                            .pending_input
                            .take()
                            .filter(|pending| pending.focus == window.focus)
                        else {
                            return;
                        };

                        let node_id = window.focus_node_id_in_rendered_frame(window.focus);
                        let dispatch_path =
                            window.rendered_frame.dispatch_tree.dispatch_path(node_id);

                        let to_replay = window
                            .rendered_frame
                            .dispatch_tree
                            .flush_dispatch(currently_pending.keystrokes, &dispatch_path);

                        window.pending_input_changed(cx);
                        window.replay_pending_input(to_replay, cx)
                    })
                    .log_err();
                }));
            } else {
                currently_pending.timer = None;
            }
            self.pending_input = Some(currently_pending);
            self.pending_input_changed(cx);
            cx.propagate_event = false;
            return;
        }

        let skip_bindings = event
            .downcast_ref::<KeyDownEvent>()
            .filter(|key_down_event| key_down_event.prefer_character_input)
            .map(|_| {
                self.platform_window
                    .take_input_handler()
                    .map_or(false, |mut input_handler| {
                        let accepts = input_handler.accepts_text_input(self, cx);
                        self.platform_window.set_input_handler(input_handler);
                        // If modifiers are not excessive (e.g. AltGr), and the input handler is accepting text input,
                        // we prefer the text input over bindings.
                        accepts
                    })
            })
            .unwrap_or(false);

        if !skip_bindings {
            for binding in match_result.bindings {
                self.dispatch_action_on_node(node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    self.dispatch_keystroke_observers(
                        event,
                        Some(binding.action),
                        match_result.context_stack,
                        cx,
                    );
                    self.pending_input_changed(cx);
                    return;
                }
            }
        }

        self.finish_dispatch_key_event(event, dispatch_path, match_result.context_stack, cx);
        self.pending_input_changed(cx);
    }

    pub(super) fn finish_dispatch_key_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: SmallVec<[DispatchNodeId; 32]>,
        context_stack: Vec<KeyContext>,
        cx: &mut App,
    ) {
        self.dispatch_key_down_up_event(event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        self.dispatch_modifiers_changed_event(event, &dispatch_path, cx);
        if !cx.propagate_event {
            return;
        }

        self.dispatch_keystroke_observers(event, None, context_stack, cx);
    }

    pub(crate) fn pending_input_changed(&mut self, cx: &mut App) {
        self.pending_input_observers
            .clone()
            .retain(&(), |callback| callback(self, cx));
    }

    pub(super) fn dispatch_key_down_up_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        // Capture phase
        for node_id in dispatch_path {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);

            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Capture, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }

        // Bubble phase
        for node_id in dispatch_path.iter().rev() {
            // Handle low level key events
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for key_listener in node.key_listeners.clone() {
                key_listener(event, DispatchPhase::Bubble, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    pub(super) fn dispatch_modifiers_changed_event(
        &mut self,
        event: &dyn Any,
        dispatch_path: &SmallVec<[DispatchNodeId; 32]>,
        cx: &mut App,
    ) {
        let Some(event) = event.downcast_ref::<ModifiersChangedEvent>() else {
            return;
        };
        for node_id in dispatch_path.iter().rev() {
            let node = self.rendered_frame.dispatch_tree.node(*node_id);
            for listener in node.modifiers_changed_listeners.clone() {
                listener(event, self, cx);
                if !cx.propagate_event {
                    return;
                }
            }
        }
    }

    /// Determine whether a potential multi-stroke key binding is in progress on this window.
    pub fn has_pending_keystrokes(&self) -> bool {
        self.pending_input.is_some()
    }

    pub(crate) fn clear_pending_keystrokes(&mut self) {
        self.pending_input.take();
    }

    /// Returns the currently pending input keystrokes that might result in a multi-stroke key binding.
    pub fn pending_input_keystrokes(&self) -> Option<&[Keystroke]> {
        self.pending_input
            .as_ref()
            .map(|pending_input| pending_input.keystrokes.as_slice())
    }

    pub(super) fn replay_pending_input(&mut self, replays: SmallVec<[Replay; 1]>, cx: &mut App) {
        let node_id = self.focus_node_id_in_rendered_frame(self.focus);
        let dispatch_path = self.rendered_frame.dispatch_tree.dispatch_path(node_id);

        'replay: for replay in replays {
            let event = KeyDownEvent {
                keystroke: replay.keystroke.clone(),
                is_held: false,
                prefer_character_input: true,
            };

            cx.propagate_event = true;
            for binding in replay.bindings {
                self.dispatch_action_on_node(node_id, binding.action.as_ref(), cx);
                if !cx.propagate_event {
                    self.dispatch_keystroke_observers(
                        &event,
                        Some(binding.action),
                        Vec::default(),
                        cx,
                    );
                    continue 'replay;
                }
            }

            self.dispatch_key_down_up_event(&event, &dispatch_path, cx);
            if !cx.propagate_event {
                continue 'replay;
            }
            if let Some(input) = replay.keystroke.key_char.as_ref().cloned()
                && let Some(mut input_handler) = self.platform_window.take_input_handler()
            {
                input_handler.dispatch_input(&input, self, cx);
                self.platform_window.set_input_handler(input_handler)
            }
        }
    }
}
