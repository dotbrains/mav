use std::rc::Rc;

use crate::PlatformStyle;
use crate::{h_flex, prelude::*};
use gpui::{
    Action, AnyElement, App, FocusHandle, Global, IntoElement, KeybindingKeystroke, Keystroke,
    Window,
};

#[path = "keybinding/render.rs"]
mod render;
pub use render::{
    Key, KeyIcon, render_keybinding_keystroke, render_modifiers, text_for_action,
    text_for_keybinding_keystrokes, text_for_keystroke, text_for_keystrokes,
};

#[derive(Debug)]
enum Source {
    Action {
        action: Box<dyn Action>,
        focus_handle: Option<FocusHandle>,
    },
    Keystrokes {
        /// A keybinding consists of a set of keystrokes,
        /// where each keystroke is a key and a set of modifier keys.
        /// More than one keystroke produces a chord.
        ///
        /// This should always contain at least one keystroke.
        keystrokes: Rc<[KeybindingKeystroke]>,
    },
}

impl Clone for Source {
    fn clone(&self) -> Self {
        match self {
            Source::Action {
                action,
                focus_handle,
            } => Source::Action {
                action: action.boxed_clone(),
                focus_handle: focus_handle.clone(),
            },
            Source::Keystrokes { keystrokes } => Source::Keystrokes {
                keystrokes: keystrokes.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, IntoElement, RegisterComponent)]
pub struct KeyBinding {
    source: Source,
    size: Option<AbsoluteLength>,
    /// The [`PlatformStyle`] to use when displaying this keybinding.
    platform_style: PlatformStyle,
    /// Determines whether the keybinding is meant for vim mode.
    vim_mode: bool,
    /// Indicates whether the keybinding is currently disabled.
    disabled: bool,
}

struct VimStyle(bool);
impl Global for VimStyle {}

impl KeyBinding {
    /// Returns the highest precedence keybinding for an action. This is the last binding added to
    /// the keymap. User bindings are added after built-in bindings so that they take precedence.
    pub fn for_action(action: &dyn Action, cx: &App) -> Self {
        Self::new(action, None, cx)
    }

    /// Like `for_action`, but lets you specify the context from which keybindings are matched.
    pub fn for_action_in(action: &dyn Action, focus: &FocusHandle, cx: &App) -> Self {
        Self::new(action, Some(focus.clone()), cx)
    }
    pub fn has_binding(&self, window: &Window) -> bool {
        match &self.source {
            Source::Action {
                action,
                focus_handle: Some(focus),
            } => window
                .highest_precedence_binding_for_action_in(action.as_ref(), focus)
                .or_else(|| window.highest_precedence_binding_for_action(action.as_ref()))
                .is_some(),
            _ => false,
        }
    }

    pub fn set_vim_mode(cx: &mut App, enabled: bool) {
        cx.set_global(VimStyle(enabled));
    }

    fn is_vim_mode(cx: &App) -> bool {
        cx.try_global::<VimStyle>().is_some_and(|g| g.0)
    }

    pub fn new(action: &dyn Action, focus_handle: Option<FocusHandle>, cx: &App) -> Self {
        Self {
            source: Source::Action {
                action: action.boxed_clone(),
                focus_handle,
            },
            size: None,
            vim_mode: KeyBinding::is_vim_mode(cx),
            platform_style: PlatformStyle::platform(),
            disabled: false,
        }
    }

    pub fn from_keystrokes(keystrokes: Rc<[KeybindingKeystroke]>, vim_mode: bool) -> Self {
        Self {
            source: Source::Keystrokes { keystrokes },
            size: None,
            vim_mode,
            platform_style: PlatformStyle::platform(),
            disabled: false,
        }
    }

    /// Sets the [`PlatformStyle`] for this [`KeyBinding`].
    pub fn platform_style(mut self, platform_style: PlatformStyle) -> Self {
        self.platform_style = platform_style;
        self
    }

    /// Sets the size for this [`KeyBinding`].
    pub fn size(mut self, size: impl Into<AbsoluteLength>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets whether this keybinding is currently disabled.
    /// Disabled keybinds will be rendered in a dimmed state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn vim_mode(mut self, vim_mode: bool) -> Self {
        self.vim_mode = vim_mode;
        self
    }
}

impl RenderOnce for KeyBinding {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let render_keybinding = |keystrokes: &[KeybindingKeystroke]| {
            let color = self.disabled.then_some(Color::Disabled);

            h_flex()
                .debug_selector(|| {
                    format!(
                        "KEY_BINDING-{}",
                        keystrokes
                            .iter()
                            .map(|k| k.key().to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                })
                .gap(DynamicSpacing::Base04.rems(cx))
                .flex_none()
                .children(keystrokes.iter().map(|keystroke| {
                    h_flex()
                        .flex_none()
                        .py_0p5()
                        .rounded_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .children(render_keybinding_keystroke(
                            keystroke,
                            color,
                            self.size,
                            PlatformStyle::platform(),
                            self.vim_mode,
                        ))
                }))
                .into_any_element()
        };

        match self.source {
            Source::Action {
                action,
                focus_handle,
            } => focus_handle
                .or_else(|| window.focused(cx))
                .and_then(|focus| {
                    window.highest_precedence_binding_for_action_in(action.as_ref(), &focus)
                })
                .or_else(|| window.highest_precedence_binding_for_action(action.as_ref()))
                .map(|binding| render_keybinding(binding.keystrokes())),
            Source::Keystrokes { keystrokes } => Some(render_keybinding(keystrokes.as_ref())),
        }
        .unwrap_or_else(|| gpui::Empty.into_any_element())
    }
}

impl Component for KeyBinding {
    fn scope() -> ComponentScope {
        ComponentScope::Typography
    }

    fn name() -> &'static str {
        "KeyBinding"
    }

    fn description() -> &'static str {
        "A component that displays a key binding, \
        supporting different platform styles and vim mode."
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> AnyElement {
        fn keybinding(input: &str) -> KeyBinding {
            let keystrokes: Rc<[KeybindingKeystroke]> = input
                .split_whitespace()
                .filter_map(|chunk| Keystroke::parse(chunk).ok())
                .map(KeybindingKeystroke::from_keystroke)
                .collect::<Vec<_>>()
                .into();
            KeyBinding::from_keystrokes(keystrokes, false)
        }

        v_flex()
            .gap_6()
            .children(vec![
                example_group_with_title(
                    "Platform Styles",
                    vec![
                        single_example(
                            "Mac Style",
                            keybinding("cmd-s")
                                .platform_style(PlatformStyle::Mac)
                                .into_any_element(),
                        ),
                        single_example(
                            "Linux Style",
                            keybinding("ctrl-s")
                                .platform_style(PlatformStyle::Linux)
                                .into_any_element(),
                        ),
                        single_example(
                            "Windows Style",
                            keybinding("ctrl-s")
                                .platform_style(PlatformStyle::Windows)
                                .into_any_element(),
                        ),
                    ],
                ),
                example_group_with_title(
                    "Vim Mode Style",
                    vec![
                        single_example(
                            "Simple",
                            keybinding("s")
                                .platform_style(PlatformStyle::Mac)
                                .vim_mode(true)
                                .into_any_element(),
                        ),
                        single_example(
                            "With Modifiers",
                            keybinding("ctrl-s")
                                .platform_style(PlatformStyle::Linux)
                                .vim_mode(true)
                                .into_any_element(),
                        ),
                        single_example(
                            "With other special key",
                            keybinding("ctrl-escape")
                                .platform_style(PlatformStyle::Windows)
                                .vim_mode(true)
                                .into_any_element(),
                        ),
                    ],
                ),
            ])
            .into_any_element()
    }
}
