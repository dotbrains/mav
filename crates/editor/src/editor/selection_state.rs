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

#[derive(Debug)]
/// SelectionEffects controls the side-effects of updating the selection.
///
/// The default behaviour does "what you mostly want":
/// - it pushes to the nav history if the cursor moved by >10 lines
/// - it re-triggers completion requests
/// - it scrolls to fit
///
/// You might want to modify these behaviours. For example when doing a "jump"
/// like go to definition, we always want to add to nav history; but when scrolling
/// in vim mode we never do.
///
/// Similarly, you might want to disable scrolling if you don't want the viewport to
/// move.
#[derive(Clone)]
pub struct SelectionEffects {
    pub(crate) nav_history: Option<bool>,
    pub(crate) completions: bool,
    pub(crate) scroll: Option<Autoscroll>,
    pub(crate) from_search: bool,
}

impl Default for SelectionEffects {
    fn default() -> Self {
        Self {
            nav_history: None,
            completions: true,
            scroll: Some(Autoscroll::fit()),
            from_search: false,
        }
    }
}

impl SelectionEffects {
    pub fn scroll(scroll: Autoscroll) -> Self {
        Self {
            scroll: Some(scroll),
            ..Default::default()
        }
    }

    pub fn no_scroll() -> Self {
        Self {
            scroll: None,
            ..Default::default()
        }
    }

    pub fn completions(self, completions: bool) -> Self {
        Self {
            completions,
            ..self
        }
    }

    pub fn nav_history(self, nav_history: bool) -> Self {
        Self {
            nav_history: Some(nav_history),
            ..self
        }
    }

    pub fn from_search(self, from_search: bool) -> Self {
        Self {
            from_search,
            ..self
        }
    }
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

pub(crate) struct LineManipulationResult {
    pub(crate) new_text: String,
    pub(crate) line_count_before: usize,
    pub(crate) line_count_after: usize,
}

pub(crate) fn consume_contiguous_rows(
    contiguous_row_selections: &mut Vec<Selection<Point>>,
    selection: &Selection<Point>,
    display_map: &DisplaySnapshot,
    selections: &mut Peekable<std::slice::Iter<Selection<Point>>>,
) -> (MultiBufferRow, MultiBufferRow) {
    contiguous_row_selections.push(selection.clone());
    let start_row = starting_row(selection, display_map);
    let mut end_row = ending_row(selection, display_map);

    while let Some(next_selection) = selections.peek() {
        if next_selection.start.row <= end_row.0 {
            end_row = ending_row(next_selection, display_map);
            contiguous_row_selections.push(selections.next().unwrap().clone());
        } else {
            break;
        }
    }
    (start_row, end_row)
}

pub(crate) fn starting_row(
    selection: &Selection<Point>,
    display_map: &DisplaySnapshot,
) -> MultiBufferRow {
    if selection.start.column > 0 {
        MultiBufferRow(display_map.prev_line_boundary(selection.start).0.row)
    } else {
        MultiBufferRow(selection.start.row)
    }
}

pub(crate) fn ending_row(
    next_selection: &Selection<Point>,
    display_map: &DisplaySnapshot,
) -> MultiBufferRow {
    if next_selection.end.column > 0 || next_selection.is_empty() {
        MultiBufferRow(display_map.next_line_boundary(next_selection.end).0.row + 1)
    } else {
        MultiBufferRow(next_selection.end.row)
    }
}

pub(crate) struct InvalidationStack<T>(Vec<T>);

pub(crate) trait InvalidationRegion {
    fn ranges(&self) -> &[Range<Anchor>];
}

impl<T: InvalidationRegion> InvalidationStack<T> {
    pub(crate) fn invalidate<S>(
        &mut self,
        selections: &[Selection<S>],
        buffer: &MultiBufferSnapshot,
    ) where
        S: Clone + ToOffset,
    {
        while let Some(region) = self.last() {
            let all_selections_inside_invalidation_ranges =
                if selections.len() == region.ranges().len() {
                    selections
                        .iter()
                        .zip(region.ranges().iter().map(|r| r.to_offset(buffer)))
                        .all(|(selection, invalidation_range)| {
                            let head = selection.head().to_offset(buffer);
                            invalidation_range.start <= head && invalidation_range.end >= head
                        })
                } else {
                    false
                };

            if all_selections_inside_invalidation_ranges {
                break;
            } else {
                self.pop();
            }
        }
    }
}

impl<T> Default for InvalidationStack<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> Deref for InvalidationStack<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for InvalidationStack<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl InvalidationRegion for SnippetState {
    fn ranges(&self) -> &[Range<Anchor>] {
        &self.ranges[self.active_index]
    }
}

// selections, scroll behavior, was newest selection reversed
pub(crate) type SelectSyntaxNodeHistoryState = (
    Box<[Selection<Anchor>]>,
    SelectSyntaxNodeScrollBehavior,
    bool,
);

#[derive(Default)]
pub(crate) struct SelectSyntaxNodeHistory {
    stack: Vec<SelectSyntaxNodeHistoryState>,
    // disable temporarily to allow changing selections without losing the stack
    pub(crate) disable_clearing: bool,
}

impl SelectSyntaxNodeHistory {
    pub(crate) fn try_clear(&mut self) {
        if !self.disable_clearing {
            self.stack.clear();
        }
    }

    pub(crate) fn push(&mut self, selection: SelectSyntaxNodeHistoryState) {
        self.stack.push(selection);
    }

    pub(crate) fn pop(&mut self) -> Option<SelectSyntaxNodeHistoryState> {
        self.stack.pop()
    }
}

pub(crate) enum SelectSyntaxNodeScrollBehavior {
    CursorTop,
    FitSelection,
    CursorBottom,
}
