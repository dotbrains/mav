use super::*;

pub struct CustomBlock {
    pub id: CustomBlockId,
    pub placement: BlockPlacement<Anchor>,
    pub height: Option<u32>,
    pub(super) style: BlockStyle,
    pub(super) render: Arc<Mutex<RenderBlock>>,
    pub(super) priority: usize,
}

#[derive(Clone)]
pub struct BlockProperties<P> {
    pub placement: BlockPlacement<P>,
    // None if the block takes up no space
    // (e.g. a horizontal line)
    pub height: Option<u32>,
    pub style: BlockStyle,
    pub render: RenderBlock,
    pub priority: usize,
}

impl<P: Debug> Debug for BlockProperties<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockProperties")
            .field("placement", &self.placement)
            .field("height", &self.height)
            .field("style", &self.style)
            .finish()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum BlockStyle {
    Fixed,
    Flex,
    /// Like `Flex` but doesn't use the gutter:
    /// - block content scrolls with buffer content
    /// - doesn't paint in gutter
    Spacer,
    Sticky,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct EditorMargins {
    pub gutter: GutterDimensions,
    pub right: Pixels,
    pub extended_right: Pixels,
}

#[derive(gpui::AppContext, gpui::VisualContext)]
pub struct BlockContext<'a, 'b> {
    #[window]
    pub window: &'a mut Window,
    #[app]
    pub app: &'b mut App,
    pub anchor_x: Pixels,
    pub max_width: Pixels,
    pub margins: &'b EditorMargins,
    pub em_width: Pixels,
    pub line_height: Pixels,
    pub block_id: BlockId,
    pub height: u32,
    pub selected: bool,
    pub editor_style: &'b EditorStyle,
    pub indent_guide_padding: Pixels,
}

#[derive(Clone, Debug)]
pub(super) struct Transform {
    pub(super) summary: TransformSummary,
    /// When `block` is `None`, the transform is isomorphic and passes input
    /// wrap rows through as normal text.
    pub(super) block: Option<Block>,
}

#[derive(Clone)]
pub enum Block {
    Custom(Arc<CustomBlock>),
    FoldedBuffer {
        first_excerpt: ExcerptBoundaryInfo,
        height: u32,
    },
    ExcerptBoundary {
        excerpt: ExcerptBoundaryInfo,
        height: u32,
    },
    BufferHeader {
        excerpt: ExcerptBoundaryInfo,
        height: u32,
    },
    Spacer {
        id: SpacerId,
        height: u32,
        is_below: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub(super) struct TransformSummary {
    pub(super) input_rows: WrapRow,
    pub(super) output_rows: BlockRow,
    pub(super) longest_row: BlockRow,
    pub(super) longest_row_chars: u32,
    pub(super) has_replacement_blocks: bool,
}

pub struct BlockChunks<'a> {
    pub(super) transforms: sum_tree::Cursor<'a, 'static, Transform, Dimensions<BlockRow, WrapRow>>,
    pub(super) input_chunks: wrap_map::WrapChunks<'a>,
    pub(super) input_chunk: Chunk<'a>,
    pub(super) output_row: BlockRow,
    pub(super) max_output_row: BlockRow,
    pub(super) line_count_overflow: RowDelta,
    pub(super) masked: bool,
}

#[derive(Clone)]
pub struct BlockRows<'a> {
    pub(super) transforms: sum_tree::Cursor<'a, 'static, Transform, Dimensions<BlockRow, WrapRow>>,
    pub(super) input_rows: wrap_map::WrapRows<'a>,
    pub(super) output_row: BlockRow,
    pub(super) started: bool,
}

#[derive(Clone, Copy)]
pub struct CompanionView<'a> {
    pub(super) display_map_id: EntityId,
    pub(super) companion_wrap_snapshot: &'a WrapSnapshot,
    pub(super) companion_wrap_edits: &'a WrapPatch,
    pub(super) companion: &'a Companion,
}

impl<'a> CompanionView<'a> {
    pub(crate) fn new(
        display_map_id: EntityId,
        companion_wrap_snapshot: &'a WrapSnapshot,
        companion_wrap_edits: &'a WrapPatch,
        companion: &'a Companion,
    ) -> Self {
        Self {
            display_map_id,
            companion_wrap_snapshot,
            companion_wrap_edits,
            companion,
        }
    }
}

impl<'a> From<CompanionViewMut<'a>> for CompanionView<'a> {
    fn from(view_mut: CompanionViewMut<'a>) -> Self {
        Self {
            display_map_id: view_mut.display_map_id,
            companion_wrap_snapshot: view_mut.companion_wrap_snapshot,
            companion_wrap_edits: view_mut.companion_wrap_edits,
            companion: view_mut.companion,
        }
    }
}

impl<'a> From<&'a CompanionViewMut<'a>> for CompanionView<'a> {
    fn from(view_mut: &'a CompanionViewMut<'a>) -> Self {
        Self {
            display_map_id: view_mut.display_map_id,
            companion_wrap_snapshot: view_mut.companion_wrap_snapshot,
            companion_wrap_edits: view_mut.companion_wrap_edits,
            companion: view_mut.companion,
        }
    }
}

pub struct CompanionViewMut<'a> {
    pub(super) display_map_id: EntityId,
    pub(super) companion_display_map_id: EntityId,
    pub(super) companion_wrap_snapshot: &'a WrapSnapshot,
    pub(super) companion_wrap_edits: &'a WrapPatch,
    pub(super) companion_multibuffer: &'a MultiBuffer,
    pub(super) companion_block_map: &'a mut BlockMap,
    pub(super) companion: &'a Companion,
}

impl<'a> CompanionViewMut<'a> {
    pub(crate) fn new(
        display_map_id: EntityId,
        companion_display_map_id: EntityId,
        companion_wrap_snapshot: &'a WrapSnapshot,
        companion_wrap_edits: &'a WrapPatch,
        companion_multibuffer: &'a MultiBuffer,
        companion: &'a Companion,
        companion_block_map: &'a mut BlockMap,
    ) -> Self {
        Self {
            display_map_id,
            companion_display_map_id,
            companion_wrap_snapshot,
            companion_wrap_edits,
            companion_multibuffer,
            companion,
            companion_block_map,
        }
    }
}
