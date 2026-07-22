use super::*;

impl Rope {
    pub fn offset_to_offset_utf16(&self, offset: usize) -> OffsetUtf16 {
        if offset >= self.summary().len {
            return self.summary().len_utf16;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<usize, OffsetUtf16>, _>((), &offset, Bias::Left);
        let overshoot = offset - start.0;
        start.1
            + item.map_or(Default::default(), |chunk| {
                chunk.as_slice().offset_to_offset_utf16(overshoot)
            })
    }

    pub fn offset_utf16_to_offset(&self, offset: OffsetUtf16) -> usize {
        if offset >= self.summary().len_utf16 {
            return self.summary().len;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<OffsetUtf16, usize>, _>((), &offset, Bias::Left);
        let overshoot = offset - start.0;
        start.1
            + item.map_or(Default::default(), |chunk| {
                chunk.as_slice().offset_utf16_to_offset(overshoot)
            })
    }

    pub fn offset_to_point(&self, offset: usize) -> Point {
        if offset >= self.summary().len {
            return self.summary().lines;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<usize, Point>, _>((), &offset, Bias::Left);
        let overshoot = offset - start.0;
        start.1
            + item.map_or(Point::zero(), |chunk| {
                chunk.as_slice().offset_to_point(overshoot)
            })
    }

    pub fn offset_to_point_utf16(&self, offset: usize) -> PointUtf16 {
        if offset >= self.summary().len {
            return self.summary().lines_utf16();
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<usize, PointUtf16>, _>((), &offset, Bias::Left);
        let overshoot = offset - start.0;
        start.1
            + item.map_or(PointUtf16::zero(), |chunk| {
                chunk.as_slice().offset_to_point_utf16(overshoot)
            })
    }

    pub fn point_to_point_utf16(&self, point: Point) -> PointUtf16 {
        if point >= self.summary().lines {
            return self.summary().lines_utf16();
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<Point, PointUtf16>, _>((), &point, Bias::Left);
        let overshoot = point - start.0;
        start.1
            + item.map_or(PointUtf16::zero(), |chunk| {
                chunk.as_slice().point_to_point_utf16(overshoot)
            })
    }

    pub fn point_utf16_to_point(&self, point: PointUtf16) -> Point {
        if point >= self.summary().lines_utf16() {
            return self.summary().lines;
        }
        let mut cursor = self.chunks.cursor::<Dimensions<PointUtf16, Point>>(());
        cursor.seek(&point, Bias::Left);
        let overshoot = point - cursor.start().0;
        cursor.start().1
            + cursor.item().map_or(Point::zero(), |chunk| {
                chunk
                    .as_slice()
                    .offset_to_point(chunk.as_slice().point_utf16_to_offset(overshoot, false))
            })
    }

    #[instrument(skip_all)]
    pub fn point_to_offset(&self, point: Point) -> usize {
        if point >= self.summary().lines {
            return self.summary().len;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<Point, usize>, _>((), &point, Bias::Left);
        let overshoot = point - start.0;
        start.1 + item.map_or(0, |chunk| chunk.as_slice().point_to_offset(overshoot))
    }

    pub fn point_to_offset_utf16(&self, point: Point) -> OffsetUtf16 {
        if point >= self.summary().lines {
            return self.summary().len_utf16;
        }
        let mut cursor = self.chunks.cursor::<Dimensions<Point, OffsetUtf16>>(());
        cursor.seek(&point, Bias::Left);
        let overshoot = point - cursor.start().0;
        cursor.start().1
            + cursor.item().map_or(OffsetUtf16(0), |chunk| {
                chunk.as_slice().point_to_offset_utf16(overshoot)
            })
    }

    pub fn point_utf16_to_offset(&self, point: PointUtf16) -> usize {
        self.point_utf16_to_offset_impl(point, false)
    }

    pub fn point_utf16_to_offset_utf16(&self, point: PointUtf16) -> OffsetUtf16 {
        self.point_utf16_to_offset_utf16_impl(point, false)
    }

    pub fn unclipped_point_utf16_to_offset(&self, point: Unclipped<PointUtf16>) -> usize {
        self.point_utf16_to_offset_impl(point.0, true)
    }

    fn point_utf16_to_offset_impl(&self, point: PointUtf16, clip: bool) -> usize {
        if point >= self.summary().lines_utf16() {
            return self.summary().len;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<PointUtf16, usize>, _>((), &point, Bias::Left);
        let overshoot = point - start.0;
        start.1
            + item.map_or(0, |chunk| {
                chunk.as_slice().point_utf16_to_offset(overshoot, clip)
            })
    }

    fn point_utf16_to_offset_utf16_impl(&self, point: PointUtf16, clip: bool) -> OffsetUtf16 {
        if point >= self.summary().lines_utf16() {
            return self.summary().len_utf16;
        }
        let mut cursor = self
            .chunks
            .cursor::<Dimensions<PointUtf16, OffsetUtf16>>(());
        cursor.seek(&point, Bias::Left);
        let overshoot = point - cursor.start().0;
        cursor.start().1
            + cursor.item().map_or(OffsetUtf16(0), |chunk| {
                chunk
                    .as_slice()
                    .offset_to_offset_utf16(chunk.as_slice().point_utf16_to_offset(overshoot, clip))
            })
    }

    pub fn unclipped_point_utf16_to_point(&self, point: Unclipped<PointUtf16>) -> Point {
        if point.0 >= self.summary().lines_utf16() {
            return self.summary().lines;
        }
        let (start, _, item) =
            self.chunks
                .find::<Dimensions<PointUtf16, Point>, _>((), &point.0, Bias::Left);
        let overshoot = Unclipped(point.0 - start.0);
        start.1
            + item.map_or(Point::zero(), |chunk| {
                chunk.as_slice().unclipped_point_utf16_to_point(overshoot)
            })
    }

    pub fn clip_offset(&self, offset: usize, bias: Bias) -> usize {
        match bias {
            Bias::Left => self.floor_char_boundary(offset),
            Bias::Right => self.ceil_char_boundary(offset),
        }
    }

    pub fn clip_offset_utf16(&self, offset: OffsetUtf16, bias: Bias) -> OffsetUtf16 {
        let (start, _, item) = self.chunks.find::<OffsetUtf16, _>((), &offset, Bias::Right);
        if let Some(chunk) = item {
            let overshoot = offset - start;
            start + chunk.as_slice().clip_offset_utf16(overshoot, bias)
        } else {
            self.summary().len_utf16
        }
    }

    pub fn clip_point(&self, point: Point, bias: Bias) -> Point {
        let (start, _, item) = self.chunks.find::<Point, _>((), &point, Bias::Right);
        if let Some(chunk) = item {
            let overshoot = point - start;
            start + chunk.as_slice().clip_point(overshoot, bias)
        } else {
            self.summary().lines
        }
    }

    pub fn clip_point_utf16(&self, point: Unclipped<PointUtf16>, bias: Bias) -> PointUtf16 {
        let (start, _, item) = self.chunks.find::<PointUtf16, _>((), &point.0, Bias::Right);
        if let Some(chunk) = item {
            let overshoot = Unclipped(point.0 - start);
            start + chunk.as_slice().clip_point_utf16(overshoot, bias)
        } else {
            self.summary().lines_utf16()
        }
    }

    pub fn starts_with(&self, pattern: &str) -> bool {
        if pattern.len() > self.len() {
            return false;
        }
        let mut remaining = pattern;
        for chunk in self.chunks_in_range(0..self.len()) {
            let Some(chunk) = chunk.get(..remaining.len().min(chunk.len())) else {
                return false;
            };
            if remaining.starts_with(chunk) {
                remaining = &remaining[chunk.len()..];
                if remaining.is_empty() {
                    return true;
                }
            } else {
                return false;
            }
        }
        remaining.is_empty()
    }

    pub fn ends_with(&self, pattern: &str) -> bool {
        if pattern.len() > self.len() {
            return false;
        }
        let mut remaining = pattern;
        for chunk in self.reversed_chunks_in_range(0..self.len()) {
            let Some(chunk) = chunk.get(chunk.len() - remaining.len().min(chunk.len())..) else {
                return false;
            };
            if remaining.ends_with(chunk) {
                remaining = &remaining[..remaining.len() - chunk.len()];
                if remaining.is_empty() {
                    return true;
                }
            } else {
                return false;
            }
        }
        remaining.is_empty()
    }

    pub fn line_len(&self, row: u32) -> u32 {
        self.clip_point(Point::new(row, u32::MAX), Bias::Left)
            .column
    }
}
