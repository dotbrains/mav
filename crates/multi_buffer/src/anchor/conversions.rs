use super::*;

impl ToOffset for Anchor {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        self.summary(snapshot)
    }
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        self.summary(snapshot)
    }
}

impl ToPoint for Anchor {
    fn to_point<'a>(&self, snapshot: &MultiBufferSnapshot) -> Point {
        self.summary(snapshot)
    }
    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> rope::PointUtf16 {
        self.summary(snapshot)
    }
}

pub trait AnchorRangeExt {
    fn cmp(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> Ordering;
    fn includes(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> bool;
    fn overlaps(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> bool;
    fn to_offset(&self, content: &MultiBufferSnapshot) -> Range<MultiBufferOffset>;
    fn to_point(&self, content: &MultiBufferSnapshot) -> Range<Point>;
}

impl AnchorRangeExt for Range<Anchor> {
    fn cmp(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> Ordering {
        match self.start.cmp(&other.start, buffer) {
            Ordering::Equal => other.end.cmp(&self.end, buffer),
            ord => ord,
        }
    }

    fn includes(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> bool {
        self.start.cmp(&other.start, buffer).is_le() && other.end.cmp(&self.end, buffer).is_le()
    }

    fn overlaps(&self, other: &Range<Anchor>, buffer: &MultiBufferSnapshot) -> bool {
        self.end.cmp(&other.start, buffer).is_ge() && self.start.cmp(&other.end, buffer).is_le()
    }

    fn to_offset(&self, content: &MultiBufferSnapshot) -> Range<MultiBufferOffset> {
        self.start.to_offset(content)..self.end.to_offset(content)
    }

    fn to_point(&self, content: &MultiBufferSnapshot) -> Range<Point> {
        self.start.to_point(content)..self.end.to_point(content)
    }
}
