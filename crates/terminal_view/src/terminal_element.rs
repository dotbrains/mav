use editor::{CursorLayout, EditorSettings, HighlightedRange, HighlightedRangeLine};
use gpui::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, ContentMask, Context, DispatchPhase,
    Element, ElementId, Entity, FocusHandle, Font, FontFeatures, FontStyle, FontWeight,
    GlobalElementId, HighlightStyle, Hitbox, Hsla, InputHandler, InteractiveElement, Interactivity,
    IntoElement, LayoutId, Length, ModifiersChangedEvent, MouseButton, MouseMoveEvent, Pixels,
    Point as GpuiPoint, StatefulInteractiveElement, StrikethroughStyle, Styled, TextRun, TextStyle,
    UTF16Selection, UnderlineStyle, WeakEntity, WhiteSpace, Window, div, fill, point, px, relative,
    size,
};
use itertools::Itertools;
use language::CursorShape as EditorCursorShape;
use settings::Settings;
use std::time::Instant;
use terminal::{
    Cell, Color, Content, CursorShape, IndexedCell, Modes, NamedColor, Point, Range, Terminal,
    TerminalBounds, is_app_chosen_exact_color as terminal_is_app_chosen_exact_color,
    is_default_background_color, terminal_settings::TerminalSettings,
};
use theme::{ActiveTheme, Theme};
use theme_settings::ThemeSettings;
use ui::utils::ensure_minimum_contrast;
use ui::{ParentElement, Tooltip};
use util::ResultExt;
use workspace::Workspace;

use std::mem;
use std::{fmt::Debug, rc::Rc};

use crate::{BlockContext, BlockProperties, ContentMode, TerminalMode, TerminalView};

mod element_impl;
mod grid;
mod input_handler;
mod layout;
mod mouse;
mod paint;
mod prepaint;
mod request_layout;
mod style;
mod utils;

#[cfg(test)]
mod tests;

pub(crate) use input_handler::TerminalInputHandler;
pub(crate) use layout::{BackgroundRegion, DisplayCursor, merge_background_regions};
pub use layout::{BatchedTextRun, LayoutPoint, LayoutRect, LayoutState, TerminalLayoutCell};
pub use utils::{convert_color, is_blank};
pub(crate) use utils::{terminal_content_reaches_bottom, to_highlighted_range_lines};

pub struct TerminalElement {
    terminal: Entity<Terminal>,
    terminal_view: Entity<TerminalView>,
    workspace: WeakEntity<Workspace>,
    focus: FocusHandle,
    focused: bool,
    cursor_visible: bool,
    interactivity: Interactivity,
    mode: TerminalMode,
    block_below_cursor: Option<Rc<BlockProperties>>,
}

impl InteractiveElement for TerminalElement {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl StatefulInteractiveElement for TerminalElement {}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
