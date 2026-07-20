use gpui::{AbsoluteLength, Rems, px, rems};
use std::time::Duration;

pub const FILE_HEADER_HEIGHT: u32 = 2;
pub const BUFFER_HEADER_PADDING: Rems = rems(0.25);
pub const MULTI_BUFFER_EXCERPT_HEADER_HEIGHT: u32 = 1;
pub(crate) const MAX_LINE_LEN: usize = 1024;
pub(crate) const CURSORS_VISIBLE_FOR: Duration = Duration::from_millis(2000);
#[doc(hidden)]
pub const CODE_ACTIONS_DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(250);
pub const SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(100);

pub(crate) const CODE_ACTION_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const FORMAT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const SCROLL_CENTER_TOP_BOTTOM_DEBOUNCE_TIMEOUT: Duration = Duration::from_secs(1);
pub const LSP_REQUEST_DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(50);

pub(crate) const EDIT_PREDICTION_KEY_CONTEXT: &str = "edit_prediction";
pub(crate) const MINIMAP_FONT_SIZE: AbsoluteLength = AbsoluteLength::Pixels(px(2.));
