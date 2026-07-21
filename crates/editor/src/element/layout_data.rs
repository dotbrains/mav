use super::*;

#[derive(Clone, Copy)]
pub struct ContextMenuLayout {
    pub(super) y_flipped: bool,
    pub(super) bounds: Bounds<Pixels>,
}

/// Holds information required for layouting the editor scrollbars.
pub struct ScrollbarLayoutInformation {
    /// The bounds of the editor area (excluding the content offset).
    pub(super) editor_bounds: Bounds<Pixels>,
    /// The available range to scroll within the document.
    pub(super) scroll_range: Size<Pixels>,
    /// The space available for one glyph in the editor.
    pub(super) glyph_grid_cell: Size<Pixels>,
}

impl ScrollbarLayoutInformation {
    pub fn new(
        editor_bounds: Bounds<Pixels>,
        glyph_grid_cell: Size<Pixels>,
        document_size: Size<Pixels>,
        longest_line_blame_width: Pixels,
        settings: &EditorSettings,
        scroll_beyond_last_line: ScrollBeyondLastLine,
    ) -> Self {
        let vertical_overscroll = match scroll_beyond_last_line {
            ScrollBeyondLastLine::OnePage => editor_bounds.size.height,
            ScrollBeyondLastLine::Off => glyph_grid_cell.height,
            ScrollBeyondLastLine::VerticalScrollMargin => {
                (1.0 + settings.vertical_scroll_margin) as f32 * glyph_grid_cell.height
            }
        };

        let overscroll = size(longest_line_blame_width, vertical_overscroll);

        ScrollbarLayoutInformation {
            editor_bounds,
            scroll_range: document_size + overscroll,
            glyph_grid_cell,
        }
    }
}

pub struct ColoredRange<T> {
    pub(super) start: T,
    pub(super) end: T,
    pub(super) color: Hsla,
}

pub struct CreaseTrailerLayout {
    pub(super) element: AnyElement,
    pub(super) bounds: Bounds<Pixels>,
}

pub(super) struct VisibleRows {
    pub(super) max_row: DisplayRow,
    pub(super) start_row: DisplayRow,
    pub(super) end_row: DisplayRow,
    pub(super) row_infos: Vec<RowInfo>,
    pub(super) start_anchor: Anchor,
    pub(super) end_anchor: Anchor,
}

pub(super) struct EditorSurface {
    pub(super) hitbox: Hitbox,
    pub(super) gutter_hitbox: Hitbox,
    pub(super) text_hitbox: Hitbox,
    pub(super) content_offset: gpui::Point<Pixels>,
    pub(super) content_origin: gpui::Point<Pixels>,
}

pub(super) struct EditorMetrics {
    pub(super) font_size: Pixels,
    pub(super) line_height: Pixels,
    pub(super) em_width: Pixels,
    pub(super) em_advance: Pixels,
    pub(super) em_layout_width: Pixels,
    pub(super) glyph_grid_cell: Size<Pixels>,
    pub(super) gutter_dimensions: GutterDimensions,
    pub(super) text_width: Pixels,
    pub(super) vertical_scrollbar_width: Pixels,
    pub(super) minimap_width: Pixels,
    pub(super) right_margin: Pixels,
    pub(super) editor_width: Pixels,
    pub(super) editor_margins: EditorMargins,
}

pub(super) struct RowActivity {
    pub(super) current_selection_head: Option<DisplayRow>,
    pub(super) run_indicator_rows: HashSet<DisplayRow>,
    pub(super) breakpoint_rows:
        HashMap<DisplayRow, (Anchor, Breakpoint, Option<BreakpointSessionState>)>,
}

pub(super) struct VerticalAutoscroll {
    pub(super) autoscroll_request: Option<(Autoscroll, bool)>,
    pub(super) autoscroll_containing_element: bool,
    pub(super) needs_horizontal_autoscroll: NeedsHorizontalAutoscroll,
}

pub(super) struct ScrollPositionLayout {
    pub(super) scroll_position: gpui::Point<ScrollOffset>,
    pub(super) scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
    pub(super) scroll_max: gpui::Point<ScrollPixelOffset>,
}

pub(super) struct InlineDecorationLayouts {
    pub(super) inline_diagnostics: HashMap<DisplayRow, AnyElement>,
    pub(super) inline_blame_layout: Option<InlineBlameLayout>,
    pub(super) inline_code_actions: Option<AnyElement>,
}

pub(super) struct GutterIndicatorLayouts {
    pub(super) test_indicators: Vec<AnyElement>,
    pub(super) bookmarks: Vec<AnyElement>,
    pub(super) breakpoints: Vec<AnyElement>,
    pub(super) diff_review_button: Option<AnyElement>,
}

pub(super) struct StickyHeaderLayouts {
    pub(super) sticky_headers: Option<header::StickyHeaders>,
    pub(super) indent_guides: Option<Vec<IndentGuideLayout>>,
}

pub(super) struct DiffHunkControlLayouts {
    pub(super) diff_hunk_controls: Vec<AnyElement>,
    pub(super) diff_hunk_control_bounds: Vec<(DisplayRow, Bounds<Pixels>)>,
}

pub(super) struct PositionMapLayout {
    pub(super) position_map: Rc<PositionMap>,
}

pub(super) struct BlockRenderPhase {
    pub(super) blocks_output: RenderBlocksOutput,
    pub(super) sticky_header_excerpt_id: Option<BufferId>,
    pub(super) start_buffer_row: MultiBufferRow,
    pub(super) end_buffer_row: MultiBufferRow,
    pub(super) preliminary_scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
    pub(super) indent_guides: Option<Vec<IndentGuideLayout>>,
}

pub(super) struct LineSetupLayouts {
    pub(super) line_numbers: Arc<HashMap<MultiBufferRow, LineNumberLayout>>,
    pub(super) expand_toggles: Vec<Option<(AnyElement, gpui::Point<Pixels>)>>,
    pub(super) crease_toggles: Vec<Option<AnyElement>>,
    pub(super) crease_trailers: Vec<Option<AnyElement>>,
    pub(super) display_hunks: Vec<(DisplayDiffHunk, Option<Hitbox>)>,
    pub(super) line_layouts: Vec<LineWithInvisibles>,
}

pub(super) struct CursorSurfaceLayouts {
    pub(super) cursors: Vec<(DisplayPoint, Hsla)>,
    pub(super) visible_cursors: Vec<CursorLayout>,
    pub(super) navigation_overlay_paint_commands: Vec<NavigationOverlayPaintCommand>,
    pub(super) scrollbars_layout: Option<EditorScrollbars>,
}

pub(crate) struct BlockLayout {
    pub(crate) id: BlockId,
    pub(crate) x_offset: Pixels,
    pub(crate) row: Option<DisplayRow>,
    pub(crate) element: AnyElement,
    pub(crate) available_space: Size<AvailableSpace>,
    pub(crate) style: BlockStyle,
    pub(crate) overlaps_gutter: bool,
    pub(crate) is_buffer_header: bool,
}

#[derive(Default)]
pub(super) struct RenderBlocksOutput {
    // We store spacer blocks separately because they paint in a different order
    // (spacers -> indent guides -> non-spacers)
    pub(super) non_spacer_blocks: Vec<BlockLayout>,
    pub(super) spacer_blocks: Vec<BlockLayout>,
    pub(super) row_block_types: HashMap<DisplayRow, bool>,
    pub(super) resized_blocks: Option<HashMap<CustomBlockId, u32>>,
}
