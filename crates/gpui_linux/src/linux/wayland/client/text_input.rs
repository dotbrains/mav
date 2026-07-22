use super::*;

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for WaylandClientStatePtr {
    fn event(
        this: &mut Self,
        text_input: &zwp_text_input_v3::ZwpTextInputV3,
        event: <zwp_text_input_v3::ZwpTextInputV3 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let client = this.get_client();
        let mut state = client.borrow_mut();
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                drop(state);
                this.enable_ime();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                drop(state);
                this.disable_ime();
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.composing = false;
                let Some(window) = state.keyboard_focused_window.clone() else {
                    return;
                };

                if let Some(commit_text) = text {
                    drop(state);
                    // IBus Intercepts keys like `a`, `b`, but those keys are needed for vim mode.
                    // We should only send ASCII characters to Mav, otherwise a user could remap a letter like `か` or `相`.
                    if commit_text.len() == 1 {
                        window.handle_input(PlatformInput::KeyDown(KeyDownEvent {
                            keystroke: Keystroke {
                                modifiers: Modifiers::default(),
                                key: commit_text.clone(),
                                key_char: Some(commit_text),
                            },
                            is_held: false,
                            prefer_character_input: false,
                        }));
                    } else {
                        window.handle_ime(ImeInput::InsertText(commit_text));
                    }
                }
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                state.composing = true;
                state.ime_pre_edit = text;
            }
            zwp_text_input_v3::Event::Done { serial } => {
                let last_serial = state.serial_tracker.get(SerialKind::InputMethod);
                state.serial_tracker.update(SerialKind::InputMethod, serial);
                let Some(window) = state.keyboard_focused_window.clone() else {
                    return;
                };

                if let Some(text) = state.ime_pre_edit.take() {
                    drop(state);
                    window.handle_ime(ImeInput::SetMarkedText(text));
                    if let Some(area) = window.get_ime_area() {
                        text_input.set_cursor_rectangle(
                            f32::from(area.origin.x) as i32,
                            f32::from(area.origin.y) as i32,
                            f32::from(area.size.width) as i32,
                            f32::from(area.size.height) as i32,
                        );
                        if last_serial == serial {
                            text_input.commit();
                        }
                    }
                } else {
                    state.composing = false;
                    drop(state);
                    window.handle_ime(ImeInput::DeleteText);
                }
            }
            _ => {}
        }
    }
}
