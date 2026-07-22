use super::*;
use std::{
    cmp::Ordering,
    fmt,
    ops::{Deref, DerefMut, Range},
};
use sum_tree::SeekTarget;
use text::{Anchor, BufferSnapshot, OffsetRangeExt, Point, ToPoint};
use tree_sitter::QueryCursor;

use super::types::*;

impl sum_tree::Summary for SyntaxLayerSummary {
    type Context<'a> = &'a BufferSnapshot;

    fn zero(buffer: &BufferSnapshot) -> Self {
        Self {
            max_depth: 0,
            min_depth: 0,
            range: Anchor::max_for_buffer(buffer.remote_id())
                ..Anchor::min_for_buffer(buffer.remote_id()),
            last_layer_range: Anchor::min_for_buffer(buffer.remote_id())
                ..Anchor::max_for_buffer(buffer.remote_id()),
            last_layer_language: None,
            contains_unknown_injections: false,
        }
    }

    fn add_summary(&mut self, other: &Self, buffer: Self::Context<'_>) {
        if other.max_depth > self.max_depth {
            self.max_depth = other.max_depth;
            self.range = other.range.clone();
        } else {
            if self.range.start.is_max() && self.range.end.is_max() {
                self.range.start = other.range.start;
            }
            if other.range.end.cmp(&self.range.end, buffer).is_gt() {
                self.range.end = other.range.end;
            }
        }
        self.last_layer_range = other.last_layer_range.clone();
        self.last_layer_language = other.last_layer_language;
        self.contains_unknown_injections |= other.contains_unknown_injections;
    }
}

impl SeekTarget<'_, SyntaxLayerSummary, SyntaxLayerSummary> for SyntaxLayerPosition {
    fn cmp(&self, cursor_location: &SyntaxLayerSummary, buffer: &BufferSnapshot) -> Ordering {
        Ord::cmp(&self.depth, &cursor_location.max_depth)
            .then_with(|| {
                self.range
                    .start
                    .cmp(&cursor_location.last_layer_range.start, buffer)
            })
            .then_with(|| {
                cursor_location
                    .last_layer_range
                    .end
                    .cmp(&self.range.end, buffer)
            })
            .then_with(|| self.language.cmp(&cursor_location.last_layer_language))
    }
}

impl SeekTarget<'_, SyntaxLayerSummary, SyntaxLayerSummary> for ChangeStartPosition {
    fn cmp(&self, cursor_location: &SyntaxLayerSummary, text: &BufferSnapshot) -> Ordering {
        Ord::cmp(&self.depth, &cursor_location.max_depth)
            .then_with(|| self.position.cmp(&cursor_location.range.end, text))
    }
}

impl SeekTarget<'_, SyntaxLayerSummary, SyntaxLayerSummary> for SyntaxLayerPositionBeforeChange {
    fn cmp(&self, cursor_location: &SyntaxLayerSummary, buffer: &BufferSnapshot) -> Ordering {
        if self.change.cmp(cursor_location, buffer).is_le() {
            Ordering::Less
        } else {
            self.position.cmp(cursor_location, buffer)
        }
    }
}

impl sum_tree::Item for SyntaxLayerEntry {
    type Summary = SyntaxLayerSummary;

    fn summary(&self, _cx: &BufferSnapshot) -> Self::Summary {
        SyntaxLayerSummary {
            min_depth: self.depth,
            max_depth: self.depth,
            range: self.range.clone(),
            last_layer_range: self.range.clone(),
            last_layer_language: self.content.language_id(),
            contains_unknown_injections: matches!(self.content, SyntaxLayerContent::Pending { .. }),
        }
    }
}

impl std::fmt::Debug for SyntaxLayerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxLayer")
            .field("depth", &self.depth)
            .field("range", &self.range)
            .field("tree", &self.content.tree())
            .finish()
    }
}

impl<'a> tree_sitter::TextProvider<&'a [u8]> for TextProvider<'a> {
    type I = ByteChunks<'a>;

    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        ByteChunks(self.0.chunks_in_range(node.byte_range()))
    }
}

impl<'a> Iterator for ByteChunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(str::as_bytes)
    }
}

impl QueryCursorHandle {
    pub fn new() -> Self {
        let mut cursor = QUERY_CURSORS.lock().pop().unwrap_or_default();
        cursor.set_match_limit(64);
        QueryCursorHandle(Some(cursor))
    }
}

impl Deref for QueryCursorHandle {
    type Target = QueryCursor;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl DerefMut for QueryCursorHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

impl Drop for QueryCursorHandle {
    fn drop(&mut self) {
        let mut cursor = self.0.take().unwrap();
        cursor.set_byte_range(0..usize::MAX);
        cursor.set_point_range(Point::zero().to_ts_point()..Point::MAX.to_ts_point());
        cursor.set_containing_byte_range(0..usize::MAX);
        cursor.set_containing_point_range(Point::zero().to_ts_point()..Point::MAX.to_ts_point());
        QUERY_CURSORS.lock().push(cursor)
    }
}

pub trait ToTreeSitterPoint {
    fn to_ts_point(self) -> tree_sitter::Point;
    fn from_ts_point(point: tree_sitter::Point) -> Self;
}

impl ToTreeSitterPoint for Point {
    fn to_ts_point(self) -> tree_sitter::Point {
        tree_sitter::Point::new(self.row as usize, self.column as usize)
    }

    fn from_ts_point(point: tree_sitter::Point) -> Self {
        Point::new(point.row as u32, point.column as u32)
    }
}

pub(super) struct LogIncludedRanges<'a>(pub(super) &'a [tree_sitter::Range]);
pub(super) struct LogPoint(pub(super) Point);
pub(super) struct LogAnchorRange<'a>(
    pub(super) &'a Range<Anchor>,
    pub(super) &'a text::BufferSnapshot,
);
pub(super) struct LogOffsetRanges<'a>(
    pub(super) &'a [Range<usize>],
    pub(super) &'a text::BufferSnapshot,
);
pub(super) struct LogChangedRegions<'a>(
    pub(super) &'a ChangeRegionSet,
    pub(super) &'a text::BufferSnapshot,
);

impl fmt::Debug for LogIncludedRanges<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|range| {
                let start = range.start_point;
                let end = range.end_point;
                (start.row, start.column)..(end.row, end.column)
            }))
            .finish()
    }
}

impl fmt::Debug for LogAnchorRange<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let range = self.0.to_point(self.1);
        (LogPoint(range.start)..LogPoint(range.end)).fmt(f)
    }
}

impl fmt::Debug for LogOffsetRanges<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|range| {
                LogPoint(range.start.to_point(self.1))..LogPoint(range.end.to_point(self.1))
            }))
            .finish()
    }
}

impl fmt::Debug for LogChangedRegions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(
                self.0
                    .0
                    .iter()
                    .map(|region| LogAnchorRange(&region.range, self.1)),
            )
            .finish()
    }
}

impl fmt::Debug for LogPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.0.row, self.0.column).fmt(f)
    }
}
