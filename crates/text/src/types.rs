use crate::FullOffset;
use anyhow::Context as _;
use collections::HashMap;
use std::{
    fmt::Display,
    num::NonZeroU64,
    ops::{Range, Sub},
    sync::Arc,
};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct BufferId(NonZeroU64);

impl Display for BufferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<NonZeroU64> for BufferId {
    fn from(id: NonZeroU64) -> Self {
        BufferId(id)
    }
}

impl BufferId {
    /// Returns Err if `id` is outside of BufferId domain.
    pub fn new(id: u64) -> anyhow::Result<Self> {
        let id = NonZeroU64::new(id).context("Buffer id cannot be 0.")?;
        Ok(Self(id))
    }

    /// Increments this buffer id, returning the old value.
    /// So that's a post-increment operator in disguise.
    pub fn next(&mut self) -> Self {
        let old = *self;
        self.0 = self.0.saturating_add(1);
        old
    }

    pub fn to_proto(self) -> u64 {
        self.into()
    }
}

impl From<BufferId> for u64 {
    fn from(id: BufferId) -> Self {
        id.0.get()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Edit<D> {
    pub old: Range<D>,
    pub new: Range<D>,
}

impl<D> Edit<D>
where
    D: PartialEq,
{
    pub fn is_empty(&self) -> bool {
        self.old.start == self.old.end && self.new.start == self.new.end
    }
}

impl<D, DDelta> Edit<D>
where
    D: Sub<D, Output = DDelta> + Copy,
{
    pub fn old_len(&self) -> DDelta {
        self.old.end - self.old.start
    }

    pub fn new_len(&self) -> DDelta {
        self.new.end - self.new.start
    }
}

impl<D1, D2> Edit<(D1, D2)> {
    pub fn flatten(self) -> (Edit<D1>, Edit<D2>) {
        (
            Edit {
                old: self.old.start.0..self.old.end.0,
                new: self.new.start.0..self.new.end.0,
            },
            Edit {
                old: self.old.start.1..self.old.end.1,
                new: self.new.start.1..self.new.end.1,
            },
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    Edit(EditOperation),
    Undo(UndoOperation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditOperation {
    pub timestamp: clock::Lamport,
    pub version: clock::Global,
    pub ranges: Vec<Range<FullOffset>>,
    pub new_text: Vec<Arc<str>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoOperation {
    pub timestamp: clock::Lamport,
    pub version: clock::Global,
    pub counts: HashMap<clock::Lamport, u32>,
}
