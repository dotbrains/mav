use super::*;

impl std::ops::Deref for DisplaySnapshot {
    type Target = BlockSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.block_snapshot
    }
}

/// A zero-indexed point in a text buffer consisting of a row and column adjusted for inserted blocks.
#[derive(Copy, Clone, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct DisplayPoint(BlockPoint);

impl Debug for DisplayPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "DisplayPoint({}, {})",
            self.row().0,
            self.column()
        ))
    }
}

impl Add for DisplayPoint {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        DisplayPoint(BlockPoint(self.0.0 + other.0.0))
    }
}

impl Sub for DisplayPoint {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        DisplayPoint(BlockPoint(self.0.0 - other.0.0))
    }
}

#[derive(Debug, Copy, Clone, Default, Eq, Ord, PartialOrd, PartialEq, Deserialize, Hash)]
#[serde(transparent)]
pub struct DisplayRow(pub u32);

impl DisplayRow {
    pub(crate) fn as_display_point(&self) -> DisplayPoint {
        DisplayPoint::new(*self, 0)
    }
}

impl Add<DisplayRow> for DisplayRow {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        DisplayRow(self.0 + other.0)
    }
}

impl Add<u32> for DisplayRow {
    type Output = Self;

    fn add(self, other: u32) -> Self::Output {
        DisplayRow(self.0 + other)
    }
}

impl Sub<DisplayRow> for DisplayRow {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        DisplayRow(self.0 - other.0)
    }
}

impl Sub<u32> for DisplayRow {
    type Output = Self;

    fn sub(self, other: u32) -> Self::Output {
        DisplayRow(self.0 - other)
    }
}

impl DisplayPoint {
    pub fn new(row: DisplayRow, column: u32) -> Self {
        Self(BlockPoint(Point::new(row.0, column)))
    }

    pub fn zero() -> Self {
        Self::new(DisplayRow(0), 0)
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn row(self) -> DisplayRow {
        DisplayRow(self.0.row)
    }

    pub fn column(self) -> u32 {
        self.0.column
    }

    pub fn row_mut(&mut self) -> &mut u32 {
        &mut self.0.row
    }

    pub fn column_mut(&mut self) -> &mut u32 {
        &mut self.0.column
    }

    pub fn to_point(self, map: &DisplaySnapshot) -> Point {
        map.display_point_to_point(self, Bias::Left)
    }

    pub fn to_offset(self, map: &DisplaySnapshot, bias: Bias) -> MultiBufferOffset {
        let wrap_point = map.block_snapshot.to_wrap_point(self.0, bias);
        let tab_point = map.wrap_snapshot().to_tab_point(wrap_point);
        let fold_point = map
            .tab_snapshot()
            .tab_point_to_fold_point(tab_point, bias)
            .0;
        let inlay_point = fold_point.to_inlay_point(map.fold_snapshot());
        map.inlay_snapshot()
            .to_buffer_offset(map.inlay_snapshot().to_offset(inlay_point))
    }
}

impl ToDisplayPoint for MultiBufferOffset {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        map.point_to_display_point(self.to_point(map.buffer_snapshot()), Bias::Left)
    }
}

impl ToDisplayPoint for MultiBufferOffsetUtf16 {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        self.to_offset(map.buffer_snapshot()).to_display_point(map)
    }
}

impl ToDisplayPoint for Point {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        map.point_to_display_point(*self, Bias::Left)
    }
}

impl ToDisplayPoint for Anchor {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        self.to_point(map.buffer_snapshot()).to_display_point(map)
    }
}

/// Maps buffer offset ranges to `DisplayPoint` ranges covering only buffer text
/// (excluding inlay text), reusing cursor state across calls.
///
/// Created via [`DisplaySnapshot::display_point_converter`]. Each layer
/// (inlay -> fold -> tab -> wrap -> block) is backed by a forward-only cursor,
/// so it is most efficient when ranges are supplied with non-decreasing
/// offsets. If a range starts before the previous one ended, the cursors are
/// transparently reset so the result stays correct (at the cost of an extra
/// seek), which keeps the converter robust to overlapping inputs such as the
/// base and buffer word diffs of an inline modified hunk.
pub struct DisplayPointConverter<'a> {
    inlay_cursor: BufferOffsetToInlayPointCursor<'a>,
    fold_point_cursor: FoldPointCursor<'a>,
    tab_point_cursor: TabPointCursor<'a>,
    wrap_point_cursor: WrapPointCursor<'a>,
    block_point_cursor: BlockPointCursor<'a>,
    prev_end: Option<MultiBufferOffset>,
}

impl DisplayPointConverter<'_> {
    pub fn map(&mut self, range: Range<MultiBufferOffset>) -> SmallVec<[Range<DisplayPoint>; 1]> {
        if self.prev_end.is_some_and(|prev_end| range.start < prev_end) {
            // The input went backward relative to where the cursors are
            // positioned; reset them so they can seek freely.
            self.inlay_cursor.reset();
            self.fold_point_cursor.reset();
            self.tab_point_cursor.reset();
            self.wrap_point_cursor.reset();
            self.block_point_cursor.reset();
        }
        self.prev_end = Some(range.end);

        let inlay_ranges = self.inlay_cursor.map(range);
        inlay_ranges
            .into_iter()
            .map(|inlay_range| {
                let start = self.inlay_point_to_display_point(inlay_range.start);
                let end = self.inlay_point_to_display_point(inlay_range.end);
                start..end
            })
            .collect()
    }

    fn inlay_point_to_display_point(&mut self, inlay_point: InlayPoint) -> DisplayPoint {
        let fold_point = self.fold_point_cursor.map(inlay_point, Bias::Left);
        let tab_point = self.tab_point_cursor.map(fold_point);
        let wrap_point = self.wrap_point_cursor.map(tab_point);
        DisplayPoint(self.block_point_cursor.map(wrap_point))
    }
}
