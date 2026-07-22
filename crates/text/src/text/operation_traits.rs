use super::*;

impl Operation {
    pub(crate) fn replica_id(&self) -> ReplicaId {
        operation_queue::Operation::lamport_timestamp(self).replica_id
    }

    pub fn timestamp(&self) -> clock::Lamport {
        match self {
            Operation::Edit(edit) => edit.timestamp,
            Operation::Undo(undo) => undo.timestamp,
        }
    }

    pub fn as_edit(&self) -> Option<&EditOperation> {
        match self {
            Operation::Edit(edit) => Some(edit),
            _ => None,
        }
    }

    pub fn is_edit(&self) -> bool {
        matches!(self, Operation::Edit { .. })
    }
}

impl operation_queue::Operation for Operation {
    fn lamport_timestamp(&self) -> clock::Lamport {
        match self {
            Operation::Edit(edit) => edit.timestamp,
            Operation::Undo(undo) => undo.timestamp,
        }
    }
}

pub trait ToOffset {
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize;
    /// Turns this point into the next offset in the buffer that comes after this, respecting utf8 boundaries.
    fn to_next_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot
            .visible_text
            .ceil_char_boundary(self.to_offset(snapshot) + 1)
    }
    /// Turns this point into the previous offset in the buffer that comes before this, respecting utf8 boundaries.
    fn to_previous_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot
            .visible_text
            .floor_char_boundary(self.to_offset(snapshot).saturating_sub(1))
    }
}

impl ToOffset for Point {
    #[inline]
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot.point_to_offset(*self)
    }
}

impl ToOffset for usize {
    #[track_caller]
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize {
        if !snapshot
            .as_rope()
            .assert_char_boundary::<{ cfg!(debug_assertions) }>(*self)
        {
            snapshot.as_rope().floor_char_boundary(*self)
        } else {
            *self
        }
    }
}

impl ToOffset for Anchor {
    #[inline]
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot.summary_for_anchor(self)
    }
}

impl<T: ToOffset> ToOffset for &T {
    #[inline]
    fn to_offset(&self, content: &BufferSnapshot) -> usize {
        (*self).to_offset(content)
    }
}

impl ToOffset for PointUtf16 {
    #[inline]
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot.point_utf16_to_offset(*self)
    }
}

impl ToOffset for Unclipped<PointUtf16> {
    #[inline]
    fn to_offset(&self, snapshot: &BufferSnapshot) -> usize {
        snapshot.unclipped_point_utf16_to_offset(*self)
    }
}

pub trait ToPoint {
    fn to_point(&self, snapshot: &BufferSnapshot) -> Point;
}

impl ToPoint for Anchor {
    #[inline]
    fn to_point(&self, snapshot: &BufferSnapshot) -> Point {
        snapshot.summary_for_anchor(self)
    }
}

impl ToPoint for usize {
    #[inline]
    fn to_point(&self, snapshot: &BufferSnapshot) -> Point {
        snapshot.offset_to_point(*self)
    }
}

impl ToPoint for Point {
    #[inline]
    fn to_point(&self, _: &BufferSnapshot) -> Point {
        *self
    }
}

impl ToPoint for Unclipped<PointUtf16> {
    #[inline]
    fn to_point(&self, snapshot: &BufferSnapshot) -> Point {
        snapshot.unclipped_point_utf16_to_point(*self)
    }
}

pub trait ToPointUtf16 {
    fn to_point_utf16(&self, snapshot: &BufferSnapshot) -> PointUtf16;
}

impl ToPointUtf16 for Anchor {
    #[inline]
    fn to_point_utf16(&self, snapshot: &BufferSnapshot) -> PointUtf16 {
        snapshot.summary_for_anchor(self)
    }
}

impl ToPointUtf16 for usize {
    #[inline]
    fn to_point_utf16(&self, snapshot: &BufferSnapshot) -> PointUtf16 {
        snapshot.offset_to_point_utf16(*self)
    }
}

impl ToPointUtf16 for PointUtf16 {
    #[inline]
    fn to_point_utf16(&self, _: &BufferSnapshot) -> PointUtf16 {
        *self
    }
}

impl ToPointUtf16 for Point {
    #[inline]
    fn to_point_utf16(&self, snapshot: &BufferSnapshot) -> PointUtf16 {
        snapshot.point_to_point_utf16(*self)
    }
}

pub trait ToOffsetUtf16 {
    fn to_offset_utf16(&self, snapshot: &BufferSnapshot) -> OffsetUtf16;
}

impl ToOffsetUtf16 for Anchor {
    #[inline]
    fn to_offset_utf16(&self, snapshot: &BufferSnapshot) -> OffsetUtf16 {
        snapshot.summary_for_anchor(self)
    }
}

impl ToOffsetUtf16 for usize {
    #[inline]
    fn to_offset_utf16(&self, snapshot: &BufferSnapshot) -> OffsetUtf16 {
        snapshot.offset_to_offset_utf16(*self)
    }
}

impl ToOffsetUtf16 for OffsetUtf16 {
    #[inline]
    fn to_offset_utf16(&self, _snapshot: &BufferSnapshot) -> OffsetUtf16 {
        *self
    }
}

pub trait FromAnchor {
    fn from_anchor(anchor: &Anchor, snapshot: &BufferSnapshot) -> Self;
}

impl FromAnchor for Anchor {
    #[inline]
    fn from_anchor(anchor: &Anchor, _snapshot: &BufferSnapshot) -> Self {
        *anchor
    }
}

impl FromAnchor for Point {
    #[inline]
    fn from_anchor(anchor: &Anchor, snapshot: &BufferSnapshot) -> Self {
        snapshot.summary_for_anchor(anchor)
    }
}

impl FromAnchor for PointUtf16 {
    #[inline]
    fn from_anchor(anchor: &Anchor, snapshot: &BufferSnapshot) -> Self {
        snapshot.summary_for_anchor(anchor)
    }
}

impl FromAnchor for usize {
    #[inline]
    fn from_anchor(anchor: &Anchor, snapshot: &BufferSnapshot) -> Self {
        snapshot.summary_for_anchor(anchor)
    }
}
