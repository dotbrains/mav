use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum SelectPhase {
    Begin {
        position: DisplayPoint,
        add: bool,
        click_count: usize,
    },
    BeginColumnar {
        position: DisplayPoint,
        reset: bool,
        mode: ColumnarMode,
        goal_column: u32,
    },
    Extend {
        position: DisplayPoint,
        click_count: usize,
    },
    Update {
        position: DisplayPoint,
        goal_column: u32,
        scroll_delta: gpui::Point<f32>,
    },
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColumnarMode {
    FromMouse,
    FromSelection,
}

#[derive(Clone, Debug)]
pub enum SelectMode {
    Character,
    Word(Range<Anchor>),
    Line(Range<Anchor>),
    All,
}

pub(crate) enum SelectionDragState {
    /// State when no drag related activity is detected.
    None,
    /// State when the mouse is down on a selection that is about to be dragged.
    ReadyToDrag {
        selection: Selection<Anchor>,
        click_position: gpui::Point<Pixels>,
        mouse_down_time: Instant,
    },
    /// State when the mouse is dragging the selection in the editor.
    Dragging {
        selection: Selection<Anchor>,
        drop_cursor: Selection<Anchor>,
        hide_drop_cursor: bool,
    },
}

pub(crate) enum ColumnarSelectionState {
    FromMouse {
        selection_tail: Anchor,
        display_point: Option<DisplayPoint>,
    },
    FromSelection {
        selection_tail: Anchor,
    },
}

/// Represents a button that shows up when hovering over lines in the gutter that don't have
/// any button on them already (like a bookmark, breakpoint or run indicator).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GutterHoverButton {
    pub(crate) display_row: DisplayRow,
    /// There's a small debounce between hovering over the line and showing the indicator.
    /// We don't want to show the indicator when moving the mouse from editor to e.g. project panel.
    pub(crate) is_active: bool,
}

#[derive(Clone, Copy)]
pub struct RowHighlightOptions {
    pub autoscroll: bool,
    pub include_gutter: bool,
}

impl Default for RowHighlightOptions {
    fn default() -> Self {
        Self {
            autoscroll: Default::default(),
            include_gutter: true,
        }
    }
}

pub(crate) struct RowHighlight {
    pub(crate) index: usize,
    pub(crate) range: Range<Anchor>,
    pub(crate) color: fn(&App) -> Hsla,
    pub(crate) options: RowHighlightOptions,
    pub(crate) type_id: TypeId,
}

#[derive(Clone, Debug)]
pub(crate) struct AddSelectionsState {
    pub(crate) groups: Vec<AddSelectionsGroup>,
}

#[derive(Clone, Debug)]
pub(crate) struct AddSelectionsGroup {
    pub(crate) above: bool,
    pub(crate) stack: Vec<usize>,
}

#[derive(Clone)]
pub(crate) struct SelectNextState {
    pub(crate) query: AhoCorasick,
    pub(crate) wordwise: bool,
    pub(crate) done: bool,
}

impl std::fmt::Debug for SelectNextState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("wordwise", &self.wordwise)
            .field("done", &self.done)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct AutocloseRegion {
    pub(crate) selection_id: usize,
    pub(crate) range: Range<Anchor>,
    pub(crate) pair: BracketPair,
}

#[derive(Debug)]
pub(crate) struct SnippetState {
    pub(crate) ranges: Vec<Vec<Range<Anchor>>>,
    pub(crate) active_index: usize,
    pub(crate) choices: Vec<Option<Vec<String>>>,
}
