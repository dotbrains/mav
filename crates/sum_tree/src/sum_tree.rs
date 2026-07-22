#[path = "sum_tree/access.rs"]
mod access;
#[path = "sum_tree/append.rs"]
mod append;
#[path = "sum_tree/build.rs"]
mod build;
#[path = "cursor.rs"]
mod cursor;
#[path = "sum_tree/keyed.rs"]
mod keyed;
#[path = "sum_tree/node.rs"]
mod node;
#[cfg(any(test, feature = "test-support"))]
#[path = "property_test.rs"]
pub mod property_test;
#[path = "sum_tree/search.rs"]
mod search;
#[path = "tree_map.rs"]
mod tree_map;

#[cfg(test)]
mod tests;

pub use cursor::{Cursor, FilterCursor, Iter};
pub(crate) use node::sum;
pub use node::{Edit, Node};
pub use tree_map::{MapSeekTarget, TreeMap, TreeSet};

pub(crate) use heapless::Vec as ArrayVec;
pub(crate) use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, ParallelIterator as _,
};
pub(crate) use std::marker::PhantomData;
pub(crate) use std::mem;
pub(crate) use std::{cmp::Ordering, fmt, iter::FromIterator, sync::Arc};
pub(crate) use ztracing::instrument;

#[cfg(test)]
pub const TREE_BASE: usize = 2;
#[cfg(not(test))]
pub const TREE_BASE: usize = 6;

// Helper for when we cannot use ArrayVec::<T>::push().unwrap() as T doesn't impl Debug
pub(crate) trait CapacityResultExt {
    fn unwrap_oob(self);
}

impl<T> CapacityResultExt for Result<(), T> {
    fn unwrap_oob(self) {
        self.unwrap_or_else(|_| panic!("item should fit into fixed size ArrayVec"))
    }
}

/// An item that can be stored in a [`SumTree`]
///
/// Must be summarized by a type that implements [`Summary`]
pub trait Item: Clone {
    type Summary: Summary;

    fn summary(&self, cx: <Self::Summary as Summary>::Context<'_>) -> Self::Summary;
}

/// An [`Item`] whose summary has a specific key that can be used to identify it
pub trait KeyedItem: Item {
    type Key: for<'a> Dimension<'a, Self::Summary> + Ord;

    fn key(&self) -> Self::Key;
}

/// A type that describes the Sum of all [`Item`]s in a subtree of the [`SumTree`]
///
/// Each Summary type can have multiple [`Dimension`]s that it measures,
/// which can be used to navigate the tree
pub trait Summary: Clone {
    type Context<'a>: Copy;
    fn zero<'a>(cx: Self::Context<'a>) -> Self;
    fn add_summary<'a>(&mut self, summary: &Self, cx: Self::Context<'a>);
}

pub trait ContextLessSummary: Clone {
    fn zero() -> Self;
    fn add_summary(&mut self, summary: &Self);
}

impl<T: ContextLessSummary> Summary for T {
    type Context<'a> = ();

    fn zero<'a>((): ()) -> Self {
        T::zero()
    }

    fn add_summary<'a>(&mut self, summary: &Self, (): ()) {
        T::add_summary(self, summary)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoSummary;

/// Catch-all implementation for when you need something that implements [`Summary`] without a specific type.
/// We implement it on a `NoSummary` instead of re-using `()`, as that avoids blanket impl collisions with `impl<T: Summary> Dimension for T`
/// (as we also need unit type to be a fill-in dimension)
impl ContextLessSummary for NoSummary {
    fn zero() -> Self {
        NoSummary
    }

    fn add_summary(&mut self, _: &Self) {}
}

/// Each [`Summary`] type can have more than one [`Dimension`] type that it measures.
///
/// You can use dimensions to seek to a specific location in the [`SumTree`]
///
/// # Example:
/// Mav's rope has a `TextSummary` type that summarizes lines, characters, and bytes.
/// Each of these are different dimensions we may want to seek to
pub trait Dimension<'a, S: Summary>: Clone {
    fn zero(cx: S::Context<'_>) -> Self;

    fn add_summary(&mut self, summary: &'a S, cx: S::Context<'_>);
    #[must_use]
    fn with_added_summary(mut self, summary: &'a S, cx: S::Context<'_>) -> Self {
        self.add_summary(summary, cx);
        self
    }

    fn from_summary(summary: &'a S, cx: S::Context<'_>) -> Self {
        let mut dimension = Self::zero(cx);
        dimension.add_summary(summary, cx);
        dimension
    }
}

impl<'a, T: Summary> Dimension<'a, T> for T {
    fn zero(cx: T::Context<'_>) -> Self {
        Summary::zero(cx)
    }

    fn add_summary(&mut self, summary: &'a T, cx: T::Context<'_>) {
        Summary::add_summary(self, summary, cx);
    }
}

pub trait SeekTarget<'a, S: Summary, D: Dimension<'a, S>> {
    fn cmp(&self, cursor_location: &D, cx: S::Context<'_>) -> Ordering;
}

impl<'a, S: Summary, D: Dimension<'a, S> + Ord> SeekTarget<'a, S, D> for D {
    fn cmp(&self, cursor_location: &Self, _: S::Context<'_>) -> Ordering {
        Ord::cmp(self, cursor_location)
    }
}

impl<'a, T: Summary> Dimension<'a, T> for () {
    fn zero(_: T::Context<'_>) -> Self {}

    fn add_summary(&mut self, _: &'a T, _: T::Context<'_>) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dimensions<D1, D2, D3 = ()>(pub D1, pub D2, pub D3);

impl<'a, T: Summary, D1: Dimension<'a, T>, D2: Dimension<'a, T>, D3: Dimension<'a, T>>
    Dimension<'a, T> for Dimensions<D1, D2, D3>
{
    fn zero(cx: T::Context<'_>) -> Self {
        Dimensions(D1::zero(cx), D2::zero(cx), D3::zero(cx))
    }

    fn add_summary(&mut self, summary: &'a T, cx: T::Context<'_>) {
        self.0.add_summary(summary, cx);
        self.1.add_summary(summary, cx);
        self.2.add_summary(summary, cx);
    }
}

impl<'a, S, D1, D2, D3> SeekTarget<'a, S, Dimensions<D1, D2, D3>> for D1
where
    S: Summary,
    D1: SeekTarget<'a, S, D1> + Dimension<'a, S>,
    D2: Dimension<'a, S>,
    D3: Dimension<'a, S>,
{
    fn cmp(&self, cursor_location: &Dimensions<D1, D2, D3>, cx: S::Context<'_>) -> Ordering {
        self.cmp(&cursor_location.0, cx)
    }
}

/// Bias is used to settle ambiguities when determining positions in an ordered sequence.
///
/// The primary use case is for text, where Bias influences
/// which character an offset or anchor is associated with.
///
/// # Examples
/// Given the buffer `AˇBCD`:
/// - The offset of the cursor is 1
/// - [Bias::Left] would attach the cursor to the character `A`
/// - [Bias::Right] would attach the cursor to the character `B`
///
/// Given the buffer `A«BCˇ»D`:
/// - The offset of the cursor is 3, and the selection is from 1 to 3
/// - The left anchor of the selection has [Bias::Right], attaching it to the character `B`
/// - The right anchor of the selection has [Bias::Left], attaching it to the character `C`
///
/// Given the buffer `{ˇ<...>`, where `<...>` is a folded region:
/// - The display offset of the cursor is 1, but the offset in the buffer is determined by the bias
/// - [Bias::Left] would attach the cursor to the character `{`, with a buffer offset of 1
/// - [Bias::Right] would attach the cursor to the first character of the folded region,
///   and the buffer offset would be the offset of the first character of the folded region
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Debug, Hash, Default)]
pub enum Bias {
    /// Attach to the character on the left
    #[default]
    Left,
    /// Attach to the character on the right
    Right,
}

impl Bias {
    pub fn invert(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// A B+ tree in which each leaf node contains `Item`s of type `T` and a `Summary`s for each `Item`.
/// Each internal node contains a `Summary` of the items in its subtree.
///
/// The maximum number of items per node is `TREE_BASE * 2`.
///
/// Any [`Dimension`] supported by the [`Summary`] type can be used to seek to a specific location in the tree.

#[derive(Clone)]
pub struct SumTree<T: Item>(pub(crate) Arc<Node<T>>);

impl<T> fmt::Debug for SumTree<T>
where
    T: fmt::Debug + Item,
    T::Summary: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("SumTree").field(&self.0).finish()
    }
}
