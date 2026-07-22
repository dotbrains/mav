mod chunk;
mod offset_utf16;
mod point;
mod point_utf16;
mod unclipped;

use heapless::Vec as ArrayVec;
use rayon::iter::{IntoParallelIterator, ParallelIterator as _};
use std::{
    cmp, fmt, io, mem,
    ops::{self, AddAssign, Range},
    str,
};
use sum_tree::{Bias, Dimension, Dimensions, SumTree};
use ztracing::instrument;

pub use chunk::{Chunk, ChunkSlice};
pub use offset_utf16::OffsetUtf16;
pub use point::Point;
pub use point_utf16::PointUtf16;
pub use unclipped::Unclipped;

use crate::chunk::Bitmap;

mod conversions;
mod coordinate_methods;
mod cursor;
mod dimension_pair;
mod iterators;
mod rope_methods;
mod summary;

#[cfg(test)]
mod tests;

pub use cursor::Cursor;
pub use dimension_pair::DimensionPair;
pub use iterators::{Bytes, ChunkBitmaps, ChunkWithBitmaps, Chunks, Lines};
pub use summary::{ChunkSummary, TextDimension, TextSummary};

#[derive(Clone, Default)]
pub struct Rope {
    pub(crate) chunks: SumTree<Chunk>,
}
