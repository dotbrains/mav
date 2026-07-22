use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomBlockId(pub usize);

impl From<CustomBlockId> for ElementId {
    fn from(val: CustomBlockId) -> Self {
        val.0.into()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpacerId(pub usize);

/// A zero-indexed point in a text buffer consisting of a row and column
/// adjusted for inserted blocks, wrapped rows, tabs, folds and inlays.
#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct BlockPoint(pub Point);

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct BlockRow(pub u32);

impl_for_row_types! {
    BlockRow => RowDelta
}

impl BlockPoint {
    pub fn row(&self) -> BlockRow {
        BlockRow(self.0.row)
    }

    pub fn new(row: BlockRow, column: u32) -> Self {
        Self(Point::new(row.0, column))
    }
}

impl Deref for BlockPoint {
    type Target = Point;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for BlockPoint {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockId {
    ExcerptBoundary(Anchor),
    FoldedBuffer(BufferId),
    Custom(CustomBlockId),
    Spacer(SpacerId),
}

impl From<BlockId> for ElementId {
    fn from(value: BlockId) -> Self {
        match value {
            BlockId::Custom(CustomBlockId(id)) => ("Block", id).into(),
            BlockId::ExcerptBoundary(anchor) => anchor.opaque_id().unwrap().into(),
            BlockId::FoldedBuffer(id) => ("FoldedBuffer", EntityId::from(id.to_proto())).into(),
            BlockId::Spacer(SpacerId(id)) => ("Spacer", id).into(),
        }
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(id) => write!(f, "Block({id:?})"),
            Self::ExcerptBoundary(id) => write!(f, "ExcerptBoundary({id:?})"),
            Self::FoldedBuffer(id) => write!(f, "FoldedBuffer({id:?})"),
            Self::Spacer(id) => write!(f, "Spacer({id:?})"),
        }
    }
}
