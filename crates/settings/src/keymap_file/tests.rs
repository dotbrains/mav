use gpui::{Action, App, DummyKeyboardMapper, KeybindingKeystroke, Keystroke, Unbind};
use serde_json::Value;
use unindent::Unindent;

use super::*;

fn check_keymap_update(
    input: impl ToString,
    operation: KeybindUpdateOperation,
    expected: impl ToString,
) {
    let result =
        KeymapFile::update_keybinding(operation, input.to_string(), 4, &gpui::DummyKeyboardMapper)
            .expect("Update succeeded");
    pretty_assertions::assert_eq!(expected.to_string(), result);
}

#[track_caller]
fn parse_keystrokes(keystrokes: &str) -> Vec<KeybindingKeystroke> {
    keystrokes
        .split(' ')
        .map(|s| {
            KeybindingKeystroke::new_with_mapper(
                Keystroke::parse(s).expect("Keystrokes valid"),
                false,
                &DummyKeyboardMapper,
            )
        })
        .collect()
}

mod load;
mod remove;
mod schema;
mod update_basic;
mod update_context;
