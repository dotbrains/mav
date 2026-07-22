use super::*;

pub(crate) type Handler = Box<dyn FnMut(&mut App) -> bool + 'static>;
pub(crate) type Listener = Box<dyn FnMut(&dyn Any, &mut App) -> bool + 'static>;
pub(crate) type KeystrokeObserver =
    Box<dyn FnMut(&KeystrokeEvent, &mut Window, &mut App) -> bool + 'static>;
pub(crate) type QuitHandler = Box<dyn FnOnce(&mut App) -> LocalBoxFuture<'static, ()> + 'static>;
pub(crate) type WindowClosedHandler = Box<dyn FnMut(&mut App, WindowId)>;
pub(crate) type ReleaseListener = Box<dyn FnOnce(&mut dyn Any, &mut App) + 'static>;
pub(crate) type NewEntityListener =
    Box<dyn FnMut(AnyEntity, &mut Option<&mut Window>, &mut App) + 'static>;
/// Defines when the application should automatically quit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuitMode {
    /// Use [`QuitMode::Explicit`] on macOS and [`QuitMode::LastWindowClosed`] on other platforms.
    #[default]
    Default,
    /// Quit automatically when the last window is closed.
    LastWindowClosed,
    /// Quit only when requested via [`App::quit`].
    Explicit,
}

/// Controls when GPUI hides the mouse cursor in response to keyboard input.
///
/// Restoration on mouse motion is handled by the platform layer; this enum
/// only describes the policy for *triggering* a hide.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CursorHideMode {
    /// Never hide the cursor automatically.
    Never,
    /// Hide on character-producing key presses (typing).
    OnTyping,
    /// Hide on character-producing key presses, *and* when a key binding
    /// resolves to an action that consumes the keystroke.
    #[default]
    OnTypingAndAction,
}
