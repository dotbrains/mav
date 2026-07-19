//! UI-related utilities

use gpui::{AnyElement, App, IntoElement, px};
use theme::ActiveTheme;

use crate::{Divider, DividerColor, prelude::*};

mod apca_contrast;
mod color_contrast;
mod constants;
mod corner_solver;
mod format_distance;
mod search_input;
mod with_rem_size;

pub use apca_contrast::*;
pub use color_contrast::*;
pub use constants::*;
pub use corner_solver::{CornerSolver, inner_corner_radius};
pub use format_distance::*;
pub use search_input::*;
pub use with_rem_size::*;

/// Returns true if the current theme is light or vibrant light.
pub fn is_light(cx: &mut App) -> bool {
    cx.theme().appearance.is_light()
}

/// Returns the platform-appropriate label for the "reveal in file manager" action.
pub fn reveal_in_file_manager_label(is_remote: bool) -> &'static str {
    if cfg!(target_os = "macos") && !is_remote {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") && !is_remote {
        "Reveal in File Explorer"
    } else {
        "Reveal in File Manager"
    }
}

pub fn traffic_light_spacer(cx: &mut App, include_bottom_border: bool) -> impl IntoElement {
    traffic_light_spacer_with_child(cx, include_bottom_border, None)
}

pub fn traffic_light_spacer_with_child(
    cx: &mut App,
    include_bottom_border: bool,
    child: Option<AnyElement>,
) -> impl IntoElement {
    const CHILD_GAP: f32 = 6.;
    const SDK_26_EXTRA_TRAFFIC_LIGHT_PADDING: f32 = 2.;

    let padding_left = if child.is_some() && MACOS_SDK_26_OR_LATER {
        TRAFFIC_LIGHT_PADDING - SDK_26_EXTRA_TRAFFIC_LIGHT_PADDING
    } else {
        TRAFFIC_LIGHT_PADDING
    };

    h_flex()
        .flex_none()
        .h_full()
        .pl(px(padding_left))
        .border_color(cx.theme().colors().border)
        .when(include_bottom_border, |this| this.border_b_1())
        .when_some(child, |this, child| {
            this.child(h_flex().h_full().pr(px(CHILD_GAP)).child(child))
        })
        .child(Divider::vertical().color(DividerColor::Border))
}

/// Capitalizes the first character of a string.
///
/// This function takes a string slice as input and returns a new `String` with the first character
/// capitalized.
///
/// # Examples
///
/// ```
/// use ui::utils::capitalize;
///
/// assert_eq!(capitalize("hello"), "Hello");
/// assert_eq!(capitalize("WORLD"), "WORLD");
/// assert_eq!(capitalize(""), "");
/// ```
pub fn capitalize(str: &str) -> String {
    let mut chars = str.chars();
    match chars.next() {
        None => String::new(),
        Some(first_char) => first_char.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
