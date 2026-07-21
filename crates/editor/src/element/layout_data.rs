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
