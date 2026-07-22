use super::*;

impl Window {
    /// Returns whether accessibility features are active for this frame,
    /// i.e. whether assistive technology (such as a screen reader) is
    /// connected and an accessibility tree is being built.
    ///
    /// Use this to skip computing data during rendering that is only
    /// observable through the accessibility tree. When accessibility is
    /// activated, a redraw is forced, so gated work is recomputed before the
    /// next tree update is sent to the platform.
    ///
    /// See the [accessibility guide](crate::_accessibility) for an overview.
    pub fn is_a11y_active(&self) -> bool {
        self.a11y.is_active()
    }

    /// Register a listener for an accessibility action on a specific node.
    /// The listener will be called when a screen reader requests the given
    /// action on the node identified by `node_id`.
    ///
    /// See the [accessibility guide](crate::_accessibility) for an overview.
    pub fn on_a11y_action(
        &mut self,
        node_id: accesskit::NodeId,
        action: accesskit::Action,
        listener: impl FnMut(Option<&accesskit::ActionData>, &mut Window, &mut App) + 'static,
    ) {
        self.a11y
            .action_listeners
            .entry(node_id)
            .or_default()
            .push((action, Box::new(listener)));
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn handle_a11y_action(&mut self, request: accesskit::ActionRequest, cx: &mut App) {
        // Take listeners out temporarily so the closures can borrow Window
        // mutably, then restore them afterward.
        if let Some(mut listeners) = self.a11y.action_listeners.remove(&request.target_node) {
            let extra_data = request.data.as_ref();
            let mut matched = false;
            for (action, listener) in &mut listeners {
                if *action == request.action {
                    listener(extra_data, self, cx);
                    matched = true;
                }
            }
            self.a11y
                .action_listeners
                .insert(request.target_node, listeners);
            if matched {
                return;
            }
        }

        // Fall back to built-in action handling.
        match request.action {
            accesskit::Action::Click => {
                if let Some(bounds) = self.a11y.node_bounds.get(&request.target_node).copied() {
                    let center = bounds.center();
                    let mouse_down = PlatformInput::MouseDown(crate::MouseDownEvent {
                        button: MouseButton::Left,
                        position: center,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    });
                    let mouse_up = PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position: center,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    });
                    self.dispatch_event(mouse_down, cx);
                    self.dispatch_event(mouse_up, cx);
                }
            }
            accesskit::Action::Focus => {
                if let Some(focus_id) = self.a11y.focus_ids.get(&request.target_node).copied()
                    && let Some(handle) = FocusHandle::for_id(focus_id, &cx.focus_handles)
                {
                    self.focus(&handle, cx);
                }
            }
            accesskit::Action::Blur => {
                self.blur();
            }
            _ => {
                log::debug!(
                    "Unhandled a11y action: {:?} on {:?}",
                    request.action,
                    request.target_node
                );
            }
        }
    }

    /// For testing: set the current modifier keys state.
    /// This does not generate any events.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    /// For testing: simulate a mouse move event to the given position.
    /// This dispatches the event through the normal event handling path,
    /// which will trigger hover states and tooltips.
    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_mouse_move(&mut self, position: Point<Pixels>, cx: &mut App) {
        let event = PlatformInput::MouseMove(MouseMoveEvent {
            position,
            modifiers: self.modifiers,
            pressed_button: None,
        });
        let _ = self.dispatch_event(event, cx);
    }
}
