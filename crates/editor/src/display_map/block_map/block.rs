use super::*;
use std::fmt::Debug;

impl Block {
    pub fn id(&self) -> BlockId {
        match self {
            Block::Custom(block) => BlockId::Custom(block.id),
            Block::ExcerptBoundary {
                excerpt: next_excerpt,
                ..
            } => BlockId::ExcerptBoundary(next_excerpt.start_anchor),
            Block::FoldedBuffer { first_excerpt, .. } => {
                BlockId::FoldedBuffer(first_excerpt.buffer_id())
            }
            Block::BufferHeader {
                excerpt: next_excerpt,
                ..
            } => BlockId::ExcerptBoundary(next_excerpt.start_anchor),
            Block::Spacer { id, .. } => BlockId::Spacer(*id),
        }
    }

    pub fn has_height(&self) -> bool {
        match self {
            Block::Custom(block) => block.height.is_some(),
            Block::ExcerptBoundary { .. }
            | Block::FoldedBuffer { .. }
            | Block::BufferHeader { .. }
            | Block::Spacer { .. } => true,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Block::Custom(block) => block.height.unwrap_or(0),
            Block::ExcerptBoundary { height, .. }
            | Block::FoldedBuffer { height, .. }
            | Block::BufferHeader { height, .. }
            | Block::Spacer { height, .. } => *height,
        }
    }

    pub fn style(&self) -> BlockStyle {
        match self {
            Block::Custom(block) => block.style,
            Block::ExcerptBoundary { .. }
            | Block::FoldedBuffer { .. }
            | Block::BufferHeader { .. } => BlockStyle::Sticky,
            Block::Spacer { .. } => BlockStyle::Spacer,
        }
    }

    pub(super) fn place_above(&self) -> bool {
        match self {
            Block::Custom(block) => matches!(block.placement, BlockPlacement::Above(_)),
            Block::FoldedBuffer { .. } => false,
            Block::ExcerptBoundary { .. } => true,
            Block::BufferHeader { .. } => true,
            Block::Spacer { is_below, .. } => !*is_below,
        }
    }

    pub fn place_near(&self) -> bool {
        match self {
            Block::Custom(block) => matches!(block.placement, BlockPlacement::Near(_)),
            Block::FoldedBuffer { .. } => false,
            Block::ExcerptBoundary { .. } => false,
            Block::BufferHeader { .. } => false,
            Block::Spacer { .. } => false,
        }
    }

    pub(super) fn place_below(&self) -> bool {
        match self {
            Block::Custom(block) => matches!(
                block.placement,
                BlockPlacement::Below(_) | BlockPlacement::Near(_)
            ),
            Block::FoldedBuffer { .. } => false,
            Block::ExcerptBoundary { .. } => false,
            Block::BufferHeader { .. } => false,
            Block::Spacer { is_below, .. } => *is_below,
        }
    }

    pub(super) fn is_replacement(&self) -> bool {
        match self {
            Block::Custom(block) => matches!(block.placement, BlockPlacement::Replace(_)),
            Block::FoldedBuffer { .. } => true,
            Block::ExcerptBoundary { .. } => false,
            Block::BufferHeader { .. } => false,
            Block::Spacer { .. } => false,
        }
    }

    pub(super) fn is_header(&self) -> bool {
        match self {
            Block::Custom(_) => false,
            Block::FoldedBuffer { .. } => true,
            Block::ExcerptBoundary { .. } => true,
            Block::BufferHeader { .. } => true,
            Block::Spacer { .. } => false,
        }
    }

    pub fn is_buffer_header(&self) -> bool {
        match self {
            Block::Custom(_) => false,
            Block::FoldedBuffer { .. } => true,
            Block::ExcerptBoundary { .. } => false,
            Block::BufferHeader { .. } => true,
            Block::Spacer { .. } => false,
        }
    }
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(block) => f.debug_struct("Custom").field("block", block).finish(),
            Self::FoldedBuffer {
                first_excerpt,
                height,
            } => f
                .debug_struct("FoldedBuffer")
                .field("first_excerpt", &first_excerpt)
                .field("height", height)
                .finish(),
            Self::ExcerptBoundary { excerpt, height } => f
                .debug_struct("ExcerptBoundary")
                .field("excerpt", excerpt)
                .field("height", height)
                .finish(),
            Self::BufferHeader { excerpt, height } => f
                .debug_struct("BufferHeader")
                .field("excerpt", excerpt)
                .field("height", height)
                .finish(),
            Self::Spacer {
                id,
                height,
                is_below: _,
            } => f
                .debug_struct("Spacer")
                .field("id", id)
                .field("height", height)
                .finish(),
        }
    }
}
