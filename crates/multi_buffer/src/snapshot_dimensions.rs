use super::*;

impl MultiBufferSnapshot {
    pub fn bytes_in_range<T: ToOffset>(&self, range: Range<T>) -> MultiBufferBytes<'_> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut excerpts = self.cursor::<MultiBufferOffset, BufferOffset>();
        excerpts.seek(&range.start);

        let mut chunk;
        let mut has_trailing_newline;
        let excerpt_bytes;
        if let Some(region) = excerpts.region() {
            let mut bytes = region.buffer.bytes_in_range(
                region.buffer_range.start + (range.start - region.range.start)
                    ..(region.buffer_range.start + (range.end - region.range.start))
                        .min(region.buffer_range.end),
            );
            chunk = bytes.next().unwrap_or(&[][..]);
            excerpt_bytes = Some(bytes);
            has_trailing_newline = region.has_trailing_newline && range.end >= region.range.end;
            if chunk.is_empty() && has_trailing_newline {
                chunk = b"\n";
                has_trailing_newline = false;
            }
        } else {
            chunk = &[][..];
            excerpt_bytes = None;
            has_trailing_newline = false;
        };

        MultiBufferBytes {
            range,
            cursor: excerpts,
            excerpt_bytes,
            has_trailing_newline,
            chunk,
        }
    }

    pub fn reversed_bytes_in_range<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> ReversedMultiBufferBytes<'_> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut chunks = self.reversed_chunks_in_range(range.clone());
        let chunk = chunks.next().map_or(&[][..], |c| c.as_bytes());
        ReversedMultiBufferBytes {
            range,
            chunks,
            chunk,
        }
    }

    pub fn row_infos(&self, start_row: MultiBufferRow) -> MultiBufferRows<'_> {
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&Point::new(start_row.0, 0));
        let mut result = MultiBufferRows {
            point: Point::new(0, 0),
            is_empty: self.excerpts.is_empty(),
            is_singleton: self.is_singleton(),
            cursor,
        };
        result.seek(start_row);
        result
    }

    pub fn chunks<T: ToOffset>(
        &self,
        range: Range<T>,
        language_aware: LanguageAwareStyling,
    ) -> MultiBufferChunks<'_> {
        let mut chunks = MultiBufferChunks {
            excerpt_offset_range: ExcerptDimension(MultiBufferOffset::ZERO)
                ..ExcerptDimension(MultiBufferOffset::ZERO),
            range: MultiBufferOffset::ZERO..MultiBufferOffset::ZERO,
            excerpts: self.excerpts.cursor(()),
            diff_transforms: self.diff_transforms.cursor(()),
            diff_base_chunks: None,
            excerpt_chunks: None,
            buffer_chunk: None,
            language_aware,
            snapshot: self,
        };
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        chunks.seek(range);
        chunks
    }

    pub fn clip_offset(&self, offset: MultiBufferOffset, bias: Bias) -> MultiBufferOffset {
        self.clip_dimension(offset, bias, text::BufferSnapshot::clip_offset)
    }

    pub fn clip_point(&self, point: Point, bias: Bias) -> Point {
        self.clip_dimension(point, bias, text::BufferSnapshot::clip_point)
    }

    pub fn clip_offset_utf16(
        &self,
        offset: MultiBufferOffsetUtf16,
        bias: Bias,
    ) -> MultiBufferOffsetUtf16 {
        self.clip_dimension(offset, bias, text::BufferSnapshot::clip_offset_utf16)
    }

    pub fn clip_point_utf16(&self, point: Unclipped<PointUtf16>, bias: Bias) -> PointUtf16 {
        self.clip_dimension(point.0, bias, |buffer, point, bias| {
            buffer.clip_point_utf16(Unclipped(point), bias)
        })
    }

    pub fn offset_to_point(&self, offset: MultiBufferOffset) -> Point {
        self.convert_dimension(offset, text::BufferSnapshot::offset_to_point)
    }

    pub fn offset_to_point_utf16(&self, offset: MultiBufferOffset) -> PointUtf16 {
        self.convert_dimension(offset, text::BufferSnapshot::offset_to_point_utf16)
    }

    pub fn point_to_point_utf16(&self, point: Point) -> PointUtf16 {
        self.convert_dimension(point, text::BufferSnapshot::point_to_point_utf16)
    }

    pub fn point_utf16_to_point(&self, point: PointUtf16) -> Point {
        self.convert_dimension(point, text::BufferSnapshot::point_utf16_to_point)
    }

    #[instrument(skip_all)]
    pub fn point_to_offset(&self, point: Point) -> MultiBufferOffset {
        self.convert_dimension(point, text::BufferSnapshot::point_to_offset)
    }

    pub fn point_to_offset_utf16(&self, point: Point) -> MultiBufferOffsetUtf16 {
        self.convert_dimension(point, text::BufferSnapshot::point_to_offset_utf16)
    }

    pub fn offset_utf16_to_offset(&self, offset: MultiBufferOffsetUtf16) -> MultiBufferOffset {
        self.convert_dimension(offset, text::BufferSnapshot::offset_utf16_to_offset)
    }

    pub fn offset_to_offset_utf16(&self, offset: MultiBufferOffset) -> MultiBufferOffsetUtf16 {
        self.convert_dimension(offset, text::BufferSnapshot::offset_to_offset_utf16)
    }

    pub fn point_utf16_to_offset(&self, point: PointUtf16) -> MultiBufferOffset {
        self.convert_dimension(point, text::BufferSnapshot::point_utf16_to_offset)
    }

    pub fn point_utf16_to_offset_utf16(&self, point: PointUtf16) -> MultiBufferOffsetUtf16 {
        self.convert_dimension(point, text::BufferSnapshot::point_utf16_to_offset_utf16)
    }

    fn clip_dimension<MBD, BD>(
        &self,
        position: MBD,
        bias: Bias,
        clip_buffer_position: fn(&text::BufferSnapshot, BD, Bias) -> BD,
    ) -> MBD
    where
        MBD: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBD as Sub>::Output>,
        BD: TextDimension + Sub<Output = <MBD as Sub>::Output> + AddAssign<<MBD as Sub>::Output>,
    {
        let mut cursor = self.cursor::<MBD, BD>();
        cursor.seek(&position);
        if let Some(region) = cursor.region() {
            if position >= region.range.end {
                return region.range.end;
            }
            let overshoot = position - region.range.start;
            let mut buffer_position = region.buffer_range.start;
            buffer_position += overshoot;
            let clipped_buffer_position =
                clip_buffer_position(region.buffer, buffer_position, bias);
            let mut position = region.range.start;
            position += clipped_buffer_position - region.buffer_range.start;
            position
        } else {
            self.max_position()
        }
    }

    #[instrument(skip_all)]
    fn convert_dimension<MBR1, MBR2, BR1, BR2>(
        &self,
        key: MBR1,
        convert_buffer_dimension: fn(&text::BufferSnapshot, BR1) -> BR2,
    ) -> MBR2
    where
        MBR1: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBR1 as Sub>::Output>,
        BR1: TextDimension + Sub<Output = <MBR1 as Sub>::Output> + AddAssign<<MBR1 as Sub>::Output>,
        MBR2: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBR2 as Sub>::Output>,
        BR2: TextDimension + Sub<Output = <MBR2 as Sub>::Output> + AddAssign<<MBR2 as Sub>::Output>,
    {
        let mut cursor = self.cursor::<DimensionPair<MBR1, MBR2>, DimensionPair<BR1, BR2>>();
        cursor.seek(&DimensionPair { key, value: None });
        if let Some(region) = cursor.region() {
            if key >= region.range.end.key {
                return region.range.end.value.unwrap();
            }
            let start_key = region.range.start.key;
            let start_value = region.range.start.value.unwrap();
            let buffer_start_key = region.buffer_range.start.key;
            let buffer_start_value = region.buffer_range.start.value.unwrap();
            let mut buffer_key = buffer_start_key;
            buffer_key += key - start_key;
            let buffer_value = convert_buffer_dimension(region.buffer, buffer_key);
            let mut result = start_value;
            result += buffer_value - buffer_start_value;
            result
        } else {
            self.max_position()
        }
    }

    pub fn point_to_buffer_offset<T: ToOffset>(
        &self,
        point: T,
    ) -> Option<(&BufferSnapshot, BufferOffset)> {
        let offset = point.to_offset(self);
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&offset);
        let region = cursor.region()?;
        let overshoot = offset - region.range.start;
        let buffer_offset = region.buffer_range.start + overshoot;
        if buffer_offset == BufferOffset(region.buffer.len() + 1)
            && region.has_trailing_newline
            && !region.is_main_buffer
        {
            let main_buffer_position = cursor.main_buffer_position()?;
            let buffer_snapshot = cursor.excerpt()?.buffer_snapshot(self);
            return Some((buffer_snapshot, main_buffer_position));
        } else if buffer_offset > BufferOffset(region.buffer.len()) {
            return None;
        }
        Some((region.buffer, buffer_offset))
    }

    pub fn point_to_buffer_point(&self, point: Point) -> Option<(&BufferSnapshot, Point)> {
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&point);
        let region = cursor.region()?;
        let overshoot = point - region.range.start;
        let buffer_point = region.buffer_range.start + overshoot;
        let excerpt = cursor.excerpt()?;
        if buffer_point == region.buffer.max_point() + Point::new(1, 0)
            && region.has_trailing_newline
            && !region.is_main_buffer
        {
            return Some((
                &excerpt.buffer_snapshot(self),
                cursor.main_buffer_position()?,
            ));
        } else if buffer_point > region.buffer.max_point() {
            return None;
        }
        Some((region.buffer, buffer_point))
    }
}
