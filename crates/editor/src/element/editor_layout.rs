use super::*;

pub struct EditorLayout {
    pub(super) position_map: Rc<PositionMap>,
    pub(super) hitbox: Hitbox,
    pub(super) gutter_hitbox: Hitbox,
    pub(super) content_origin: gpui::Point<Pixels>,
    pub(super) scrollbars_layout: Option<EditorScrollbars>,
    pub(super) minimap: Option<MinimapLayout>,
    pub(super) mode: EditorMode,
    pub(super) wrap_guides: SmallVec<[(Pixels, bool); 2]>,
    pub(super) indent_guides: Option<Vec<IndentGuideLayout>>,
    pub(super) visible_display_row_range: Range<DisplayRow>,
    pub(super) active_rows: BTreeMap<DisplayRow, LineHighlightSpec>,
    pub(super) highlighted_rows: BTreeMap<DisplayRow, LineHighlight>,
    pub(super) line_elements: SmallVec<[AnyElement; 1]>,
    pub(super) line_numbers: Arc<HashMap<MultiBufferRow, LineNumberLayout>>,
    pub(super) display_hunks: Vec<(DisplayDiffHunk, Option<Hitbox>)>,
    pub(super) blamed_display_rows: Option<Vec<AnyElement>>,
    pub(super) inline_diagnostics: HashMap<DisplayRow, AnyElement>,
    pub(super) inline_blame_layout: Option<InlineBlameLayout>,
    pub(super) inline_code_actions: Option<AnyElement>,
    pub(super) blocks: Vec<BlockLayout>,
    pub(super) spacer_blocks: Vec<BlockLayout>,
    pub(super) highlighted_ranges: Vec<(Range<DisplayPoint>, Hsla)>,
    pub(super) highlighted_gutter_ranges: Vec<(Range<DisplayPoint>, Hsla)>,
    pub(super) redacted_ranges: Vec<Range<DisplayPoint>>,
    pub(super) cursors: Vec<(DisplayPoint, Hsla)>,
    pub(super) visible_cursors: Vec<CursorLayout>,
    pub(super) navigation_overlay_paint_commands: Vec<NavigationOverlayPaintCommand>,
    pub(super) selections: Vec<(PlayerColor, Vec<SelectionLayout>)>,
    pub(super) test_indicators: Vec<AnyElement>,
    pub(super) bookmarks: Vec<AnyElement>,
    pub(super) breakpoints: Vec<AnyElement>,
    pub(super) diff_review_button: Option<AnyElement>,
    pub(super) crease_toggles: Vec<Option<AnyElement>>,
    pub(super) expand_toggles: Vec<Option<(AnyElement, gpui::Point<Pixels>)>>,
    pub(super) diff_hunk_controls: Vec<AnyElement>,
    pub(super) crease_trailers: Vec<Option<CreaseTrailerLayout>>,
    pub(super) edit_prediction_popover: Option<AnyElement>,
    pub(super) mouse_context_menu: Option<AnyElement>,
    pub(super) tab_invisible: ShapedLine,
    pub(super) space_invisible: ShapedLine,
    pub(super) sticky_buffer_header: Option<AnyElement>,
    pub(super) sticky_headers: Option<header::StickyHeaders>,
    pub(super) document_colors:
        Option<(DocumentColorsRenderMode, Vec<(Range<DisplayPoint>, Hsla)>)>,
    pub(super) text_align: TextAlign,
    pub(super) content_width: Pixels,
}

impl EditorLayout {
    pub(super) fn line_end_overshoot(&self) -> Pixels {
        0.15 * self.position_map.line_height
    }
}

impl Along for ScrollbarAxes {
    type Unit = bool;

    fn along(&self, axis: ScrollbarAxis) -> Self::Unit {
        match axis {
            ScrollbarAxis::Horizontal => self.horizontal,
            ScrollbarAxis::Vertical => self.vertical,
        }
    }

    fn apply_along(&self, axis: ScrollbarAxis, f: impl FnOnce(Self::Unit) -> Self::Unit) -> Self {
        match axis {
            ScrollbarAxis::Horizontal => ScrollbarAxes {
                horizontal: f(self.horizontal),
                vertical: self.vertical,
            },
            ScrollbarAxis::Vertical => ScrollbarAxes {
                horizontal: self.horizontal,
                vertical: f(self.vertical),
            },
        }
    }
}

pub fn layout_line(
    row: DisplayRow,
    snapshot: &EditorSnapshot,
    style: &EditorStyle,
    text_width: Pixels,
    is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
    window: &mut Window,
    cx: &mut App,
) -> LineWithInvisibles {
    let use_tree_sitter =
        !snapshot.semantic_tokens_enabled || snapshot.use_tree_sitter_for_syntax(row, cx);
    let language_aware = LanguageAwareStyling {
        tree_sitter: use_tree_sitter,
        diagnostics: true,
    };
    let chunks = snapshot.highlighted_chunks(row..row + DisplayRow(1), language_aware, style);
    LineWithInvisibles::from_chunks(
        chunks,
        style,
        MAX_LINE_LEN,
        1,
        &snapshot.mode,
        text_width,
        is_row_soft_wrapped,
        &[],
        window,
        cx,
    )
    .pop()
    .unwrap()
}

#[derive(Debug, Clone)]
pub struct IndentGuideLayout {
    pub(super) origin: gpui::Point<Pixels>,
    pub(super) length: Pixels,
    pub(super) single_indent_width: Pixels,
    pub(super) display_row_range: Range<DisplayRow>,
    pub(super) depth: u32,
    pub(super) active: bool,
    pub(super) settings: IndentGuideSettings,
}

pub(super) enum CursorPopoverType {
    CodeContextMenu,
    EditPrediction,
}
