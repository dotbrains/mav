use super::*;
use crate::utils::capitalize;
use crate::{Icon, IconName, IconSize};
use gpui::{Modifiers, relative};
use itertools::Itertools;

fn render_key(
    key: &str,
    color: Option<Color>,
    platform_style: PlatformStyle,
    size: impl Into<Option<AbsoluteLength>>,
) -> AnyElement {
    let key_icon = icon_for_key(key, platform_style);
    match key_icon {
        Some(icon) => KeyIcon::new(icon, color).size(size).into_any_element(),
        None => {
            let key = capitalize(key);
            Key::new(&key, color).size(size).into_any_element()
        }
    }
}

pub fn render_keybinding_keystroke(
    keystroke: &KeybindingKeystroke,
    color: Option<Color>,
    size: impl Into<Option<AbsoluteLength>>,
    platform_style: PlatformStyle,
    vim_mode: bool,
) -> Vec<AnyElement> {
    let use_text = vim_mode
        || matches!(
            platform_style,
            PlatformStyle::Linux | PlatformStyle::Windows
        );
    let size = size.into();

    if use_text {
        let element = Key::new(
            keystroke_text(
                keystroke.modifiers(),
                keystroke.key(),
                platform_style,
                vim_mode,
            ),
            color,
        )
        .size(size)
        .into_any_element();
        vec![element]
    } else {
        let mut elements = Vec::new();
        elements.extend(render_modifiers(
            keystroke.modifiers(),
            platform_style,
            color,
            size,
            true,
        ));
        elements.push(render_key(keystroke.key(), color, platform_style, size));
        elements
    }
}

fn icon_for_key(key: &str, platform_style: PlatformStyle) -> Option<IconName> {
    match key {
        "left" => Some(IconName::ArrowLeft),
        "right" => Some(IconName::ArrowRight),
        "up" => Some(IconName::ArrowUp),
        "down" => Some(IconName::ArrowDown),
        "backspace" => Some(IconName::Backspace),
        "delete" => Some(IconName::Backspace),
        "return" => Some(IconName::Return),
        "enter" => Some(IconName::Return),
        "tab" => Some(IconName::Tab),
        "space" => Some(IconName::Space),
        "escape" => Some(IconName::Escape),
        "pagedown" => Some(IconName::PageDown),
        "pageup" => Some(IconName::PageUp),
        "shift" if platform_style == PlatformStyle::Mac => Some(IconName::Shift),
        "control" if platform_style == PlatformStyle::Mac => Some(IconName::Control),
        "platform" if platform_style == PlatformStyle::Mac => Some(IconName::Command),
        "function" if platform_style == PlatformStyle::Mac => Some(IconName::Control),
        "alt" if platform_style == PlatformStyle::Mac => Some(IconName::Option),
        _ => None,
    }
}

pub fn render_modifiers(
    modifiers: &Modifiers,
    platform_style: PlatformStyle,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
    trailing_separator: bool,
) -> impl Iterator<Item = AnyElement> {
    #[derive(Clone)]
    enum KeyOrIcon {
        Key(&'static str),
        Plus,
        Icon(IconName),
    }

    struct Modifier {
        enabled: bool,
        mac: KeyOrIcon,
        linux: KeyOrIcon,
        windows: KeyOrIcon,
    }

    let table = {
        use KeyOrIcon::*;

        [
            Modifier {
                enabled: modifiers.function,
                mac: Icon(IconName::Control),
                linux: Key("Fn"),
                windows: Key("Fn"),
            },
            Modifier {
                enabled: modifiers.control,
                mac: Icon(IconName::Control),
                linux: Key("Ctrl"),
                windows: Key("Ctrl"),
            },
            Modifier {
                enabled: modifiers.alt,
                mac: Icon(IconName::Option),
                linux: Key("Alt"),
                windows: Key("Alt"),
            },
            Modifier {
                enabled: modifiers.platform,
                mac: Icon(IconName::Command),
                linux: Key("Super"),
                windows: Key("Win"),
            },
            Modifier {
                enabled: modifiers.shift,
                mac: Icon(IconName::Shift),
                linux: Key("Shift"),
                windows: Key("Shift"),
            },
        ]
    };

    let filtered = table
        .into_iter()
        .filter(|modifier| modifier.enabled)
        .collect::<Vec<_>>();

    let platform_keys = filtered
        .into_iter()
        .map(move |modifier| match platform_style {
            PlatformStyle::Mac => Some(modifier.mac),
            PlatformStyle::Linux => Some(modifier.linux),
            PlatformStyle::Windows => Some(modifier.windows),
        });

    let separator = match platform_style {
        PlatformStyle::Mac => None,
        PlatformStyle::Linux => Some(KeyOrIcon::Plus),
        PlatformStyle::Windows => Some(KeyOrIcon::Plus),
    };

    let platform_keys = itertools::intersperse(platform_keys, separator.clone());

    platform_keys
        .chain(if modifiers.modified() && trailing_separator {
            Some(separator)
        } else {
            None
        })
        .flatten()
        .map(move |key_or_icon| match key_or_icon {
            KeyOrIcon::Key(key) => Key::new(key, color).size(size).into_any_element(),
            KeyOrIcon::Icon(icon) => KeyIcon::new(icon, color).size(size).into_any_element(),
            KeyOrIcon::Plus => "+".into_any_element(),
        })
}

#[derive(IntoElement)]
pub struct Key {
    key: SharedString,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
}

impl RenderOnce for Key {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let single_char = self.key.len() == 1;
        let size = self
            .size
            .unwrap_or_else(|| TextSize::default().rems(cx).into());

        div()
            .py_0()
            .map(|this| {
                if single_char {
                    this.w(size).flex().flex_none().justify_center()
                } else {
                    this.px_0p5()
                }
            })
            .h(size)
            .text_size(size)
            .line_height(relative(1.))
            .text_color(self.color.unwrap_or(Color::Muted).color(cx))
            .child(self.key)
    }
}

impl Key {
    pub fn new(key: impl Into<SharedString>, color: Option<Color>) -> Self {
        Self {
            key: key.into(),
            color,
            size: None,
        }
    }

    pub fn size(mut self, size: impl Into<Option<AbsoluteLength>>) -> Self {
        self.size = size.into();
        self
    }
}

#[derive(IntoElement)]
pub struct KeyIcon {
    icon: IconName,
    color: Option<Color>,
    size: Option<AbsoluteLength>,
}

impl RenderOnce for KeyIcon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let size = self.size.unwrap_or(IconSize::Small.rems().into());

        Icon::new(self.icon)
            .size(IconSize::Custom(size.to_rems(window.rem_size())))
            .color(self.color.unwrap_or(Color::Muted))
    }
}

impl KeyIcon {
    pub fn new(icon: IconName, color: Option<Color>) -> Self {
        Self {
            icon,
            color,
            size: None,
        }
    }

    pub fn size(mut self, size: impl Into<Option<AbsoluteLength>>) -> Self {
        self.size = size.into();
        self
    }
}

/// Returns a textual representation of the key binding for the given [`Action`].
pub fn text_for_action(action: &dyn Action, window: &Window, cx: &App) -> Option<String> {
    let key_binding = window.highest_precedence_binding_for_action(action)?;
    Some(text_for_keybinding_keystrokes(key_binding.keystrokes(), cx))
}

pub fn text_for_keystrokes(keystrokes: &[Keystroke], cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    let vim_enabled = KeyBinding::is_vim_mode(cx);
    keystrokes
        .iter()
        .map(|keystroke| {
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                platform_style,
                vim_enabled,
            )
        })
        .join(" ")
}

pub fn text_for_keybinding_keystrokes(keystrokes: &[KeybindingKeystroke], cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    let vim_enabled = KeyBinding::is_vim_mode(cx);
    keystrokes
        .iter()
        .map(|keystroke| {
            keystroke_text(
                keystroke.modifiers(),
                keystroke.key(),
                platform_style,
                vim_enabled,
            )
        })
        .join(" ")
}

pub fn text_for_keystroke(modifiers: &Modifiers, key: &str, cx: &App) -> String {
    let platform_style = PlatformStyle::platform();
    keystroke_text(modifiers, key, platform_style, KeyBinding::is_vim_mode(cx))
}

/// Returns a textual representation of the given [`Keystroke`].
fn keystroke_text(
    modifiers: &Modifiers,
    key: &str,
    platform_style: PlatformStyle,
    vim_mode: bool,
) -> String {
    let mut text = String::new();
    let delimiter = '-';

    if modifiers.function {
        match vim_mode {
            false => text.push_str("Fn"),
            true => text.push_str("fn"),
        }

        text.push(delimiter);
    }

    if modifiers.control {
        match (platform_style, vim_mode) {
            (PlatformStyle::Mac, false) => text.push_str("Control"),
            (PlatformStyle::Linux | PlatformStyle::Windows, false) => text.push_str("Ctrl"),
            (_, true) => text.push_str("ctrl"),
        }

        text.push(delimiter);
    }

    if modifiers.platform {
        match (platform_style, vim_mode) {
            (PlatformStyle::Mac, false) => text.push_str("Command"),
            (PlatformStyle::Mac, true) => text.push_str("cmd"),
            (PlatformStyle::Linux, false) => text.push_str("Super"),
            (PlatformStyle::Linux, true) => text.push_str("super"),
            (PlatformStyle::Windows, false) => text.push_str("Win"),
            (PlatformStyle::Windows, true) => text.push_str("win"),
        }

        text.push(delimiter);
    }

    if modifiers.alt {
        match (platform_style, vim_mode) {
            (PlatformStyle::Mac, false) => text.push_str("Option"),
            (PlatformStyle::Mac, true) => text.push_str("option"),
            (PlatformStyle::Linux | PlatformStyle::Windows, false) => text.push_str("Alt"),
            (_, true) => text.push_str("alt"),
        }

        text.push(delimiter);
    }

    if modifiers.shift {
        match (platform_style, vim_mode) {
            (_, false) => text.push_str("Shift"),
            (_, true) => text.push_str("shift"),
        }
        text.push(delimiter);
    }

    if vim_mode {
        text.push_str(key)
    } else {
        let key = match key {
            "pageup" => "PageUp",
            "pagedown" => "PageDown",
            key => &capitalize(key),
        };
        text.push_str(key);
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_for_keystroke() {
        let keystroke = Keystroke::parse("cmd-c").unwrap();
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Mac,
                false
            ),
            "Command-C".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Linux,
                false
            ),
            "Super-C".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Windows,
                false
            ),
            "Win-C".to_string()
        );

        let keystroke = Keystroke::parse("ctrl-alt-delete").unwrap();
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Mac,
                false
            ),
            "Control-Option-Delete".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Linux,
                false
            ),
            "Ctrl-Alt-Delete".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Windows,
                false
            ),
            "Ctrl-Alt-Delete".to_string()
        );

        let keystroke = Keystroke::parse("shift-pageup").unwrap();
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Mac,
                false
            ),
            "Shift-PageUp".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Linux,
                false,
            ),
            "Shift-PageUp".to_string()
        );
        assert_eq!(
            keystroke_text(
                &keystroke.modifiers,
                &keystroke.key,
                PlatformStyle::Windows,
                false
            ),
            "Shift-PageUp".to_string()
        );
    }
}
