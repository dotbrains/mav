use super::*;

#[derive(Debug, PartialEq)]
pub(crate) struct AccentData {
    pub(crate) colors: AccentColors,
    pub(crate) overrides: Vec<SharedString>,
}

pub(crate) fn debounce_value(debounce_ms: u64) -> Option<Duration> {
    if debounce_ms > 0 {
        Some(Duration::from_millis(debounce_ms))
    } else {
        None
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum NextScrollCursorCenterTopBottom {
    #[default]
    Center,
    Top,
    Bottom,
}

impl NextScrollCursorCenterTopBottom {
    pub(crate) fn next(&self) -> Self {
        match self {
            Self::Center => Self::Top,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Center,
        }
    }
}

pub(crate) struct CharacterDimensions {
    pub(crate) em_width: Pixels,
    pub(crate) em_advance: Pixels,
    pub(crate) line_height: Pixels,
}

#[doc(hidden)]
pub struct RenameState {
    pub range: Range<Anchor>,
    pub old_name: Arc<str>,
    pub editor: Entity<Editor>,
    pub(crate) block_id: CustomBlockId,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NavigationData {
    pub(crate) cursor_anchor: Anchor,
    pub(crate) cursor_position: Point,
    pub(crate) scroll_anchor: ScrollAnchor,
    pub(crate) scroll_top_row: u32,
}

pub(crate) struct FocusedBlock {
    pub(crate) id: BlockId,
    pub(crate) focus_handle: WeakFocusHandle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHighlight {
    pub background: Background,
    pub border: Option<gpui::Hsla>,
    pub include_gutter: bool,
    pub type_id: Option<TypeId>,
}

pub fn multibuffer_context_lines(cx: &App) -> u32 {
    EditorSettings::try_get(cx)
        .map(|settings| settings.excerpt_context_lines)
        .unwrap_or(2)
        .min(32)
}
