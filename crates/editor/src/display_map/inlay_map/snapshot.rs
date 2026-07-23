use super::*;

impl InlaySnapshot {
    #[ztracing::instrument(skip_all)]
    pub fn to_point(&self, offset: InlayOffset) -> InlayPoint {
        let (start, _, item) = self.transforms.find::<Dimensions<
            InlayOffset,
            InlayPoint,
            MultiBufferOffset,
        >, _>((), &offset, Bias::Right);
        let overshoot = offset.0 - start.0.0;
        match item {
            Some(Transform::Isomorphic(_)) => {
                let buffer_offset_start = start.2;
                let buffer_offset_end = buffer_offset_start + overshoot;
                let buffer_start = self.buffer.offset_to_point(buffer_offset_start);
                let buffer_end = self.buffer.offset_to_point(buffer_offset_end);
                InlayPoint(start.1.0 + (buffer_end - buffer_start))
            }
            Some(Transform::Inlay(inlay)) => {
                let overshoot = inlay.text().offset_to_point(overshoot);
                InlayPoint(start.1.0 + overshoot)
            }
            None => self.max_point(),
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn len(&self) -> InlayOffset {
        InlayOffset(self.transforms.summary().output.len)
    }

    #[ztracing::instrument(skip_all)]
    pub fn max_point(&self) -> InlayPoint {
        InlayPoint(self.transforms.summary().output.lines)
    }

    #[ztracing::instrument(skip_all, fields(point))]
    pub fn to_offset(&self, point: InlayPoint) -> InlayOffset {
        let (start, _, item) = self
            .transforms
            .find::<Dimensions<InlayPoint, InlayOffset, Point>, _>((), &point, Bias::Right);
        let overshoot = point.0 - start.0.0;
        match item {
            Some(Transform::Isomorphic(_)) => {
                let buffer_point_start = start.2;
                let buffer_point_end = buffer_point_start + overshoot;
                let buffer_offset_start = self.buffer.point_to_offset(buffer_point_start);
                let buffer_offset_end = self.buffer.point_to_offset(buffer_point_end);
                InlayOffset(start.1.0 + (buffer_offset_end - buffer_offset_start))
            }
            Some(Transform::Inlay(inlay)) => {
                let overshoot = inlay.text().point_to_offset(overshoot);
                InlayOffset(start.1.0 + overshoot)
            }
            None => self.len(),
        }
    }
    #[ztracing::instrument(skip_all)]
    pub fn to_buffer_point(&self, point: InlayPoint) -> Point {
        let (start, _, item) =
            self.transforms
                .find::<Dimensions<InlayPoint, Point>, _>((), &point, Bias::Right);
        match item {
            Some(Transform::Isomorphic(_)) => {
                let overshoot = point.0 - start.0.0;
                start.1 + overshoot
            }
            Some(Transform::Inlay(_)) => start.1,
            None => self.buffer.max_point(),
        }
    }
    #[ztracing::instrument(skip_all)]
    pub fn to_buffer_offset(&self, offset: InlayOffset) -> MultiBufferOffset {
        let (start, _, item) = self
            .transforms
            .find::<Dimensions<InlayOffset, MultiBufferOffset>, _>((), &offset, Bias::Right);
        match item {
            Some(Transform::Isomorphic(_)) => {
                let overshoot = offset - start.0;
                start.1 + overshoot
            }
            Some(Transform::Inlay(_)) => start.1,
            None => self.buffer.len(),
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_inlay_offset(&self, offset: MultiBufferOffset) -> InlayOffset {
        let mut cursor = self
            .transforms
            .cursor::<Dimensions<MultiBufferOffset, InlayOffset>>(());
        cursor.seek(&offset, Bias::Left);
        loop {
            match cursor.item() {
                Some(Transform::Isomorphic(_)) => {
                    if offset == cursor.end().0 {
                        while let Some(Transform::Inlay(inlay)) = cursor.next_item() {
                            if inlay.position.bias() == Bias::Right {
                                break;
                            } else {
                                cursor.next();
                            }
                        }
                        return cursor.end().1;
                    } else {
                        let overshoot = offset - cursor.start().0;
                        return InlayOffset(cursor.start().1.0 + overshoot);
                    }
                }
                Some(Transform::Inlay(inlay)) => {
                    if inlay.position.bias() == Bias::Left {
                        cursor.next();
                    } else {
                        return cursor.start().1;
                    }
                }
                None => {
                    return self.len();
                }
            }
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_inlay_point(&self, point: Point) -> InlayPoint {
        self.inlay_point_cursor().map(point, Bias::Left)
    }

    /// Converts a buffer offset range into one or more `InlayOffset` ranges that
    /// cover only the actual buffer text, skipping any inlay hint text that falls
    /// within the range. When there are no inlays the returned vec contains a
    /// single element identical to the input mapped into inlay-offset space.
    pub fn buffer_offset_to_inlay_ranges(
        &self,
        range: Range<MultiBufferOffset>,
    ) -> impl Iterator<Item = Range<InlayOffset>> {
        let mut cursor = self
            .transforms
            .cursor::<Dimensions<MultiBufferOffset, InlayOffset>>(());
        cursor.seek(&range.start, Bias::Right);

        std::iter::from_fn(move || {
            loop {
                match cursor.item()? {
                    Transform::Isomorphic(_) => {
                        let seg_buffer_start = cursor.start().0;
                        let seg_buffer_end = cursor.end().0;
                        let seg_inlay_start = cursor.start().1;

                        let overlap_start = cmp::max(range.start, seg_buffer_start);
                        let overlap_end = cmp::min(range.end, seg_buffer_end);

                        let past_end = seg_buffer_end >= range.end;
                        cursor.next();

                        if overlap_start < overlap_end {
                            let inlay_start =
                                InlayOffset(seg_inlay_start.0 + (overlap_start - seg_buffer_start));
                            let inlay_end =
                                InlayOffset(seg_inlay_start.0 + (overlap_end - seg_buffer_start));
                            return Some(inlay_start..inlay_end);
                        }

                        if past_end {
                            return None;
                        }
                    }
                    Transform::Inlay(_) => cursor.next(),
                }
            }
        })
    }

    pub fn buffer_offset_to_inlay_point_cursor(&self) -> BufferOffsetToInlayPointCursor<'_> {
        BufferOffsetToInlayPointCursor {
            snapshot: self,
            cursor: self
                .transforms
                .cursor::<Dimensions<MultiBufferOffset, InlayPoint>>(()),
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn inlay_point_cursor(&self) -> InlayPointCursor<'_> {
        let cursor = self.transforms.cursor::<Dimensions<Point, InlayPoint>>(());
        InlayPointCursor {
            cursor,
            transforms: &self.transforms,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn clip_point(&self, mut point: InlayPoint, mut bias: Bias) -> InlayPoint {
        let mut cursor = self.transforms.cursor::<Dimensions<InlayPoint, Point>>(());
        cursor.seek(&point, Bias::Left);
        loop {
            match cursor.item() {
                Some(Transform::Isomorphic(transform)) => {
                    if cursor.start().0 == point {
                        if let Some(Transform::Inlay(inlay)) = cursor.prev_item() {
                            if inlay.position.bias() == Bias::Left {
                                return point;
                            } else if bias == Bias::Left {
                                cursor.prev();
                            } else if transform.first_line_chars == 0 {
                                point.0 += Point::new(1, 0);
                            } else {
                                point.0 += Point::new(0, 1);
                            }
                        } else {
                            return point;
                        }
                    } else if cursor.end().0 == point {
                        if let Some(Transform::Inlay(inlay)) = cursor.next_item() {
                            if inlay.position.bias() == Bias::Right {
                                return point;
                            } else if bias == Bias::Right {
                                cursor.next();
                            } else if point.0.column == 0 {
                                point.0.row -= 1;
                                point.0.column = self.line_len(point.0.row);
                            } else {
                                point.0.column -= 1;
                            }
                        } else {
                            return point;
                        }
                    } else {
                        let overshoot = point.0 - cursor.start().0.0;
                        let buffer_point = cursor.start().1 + overshoot;
                        let clipped_buffer_point = self.buffer.clip_point(buffer_point, bias);
                        let clipped_overshoot = clipped_buffer_point - cursor.start().1;
                        let clipped_point = InlayPoint(cursor.start().0.0 + clipped_overshoot);
                        if clipped_point == point {
                            return clipped_point;
                        } else {
                            point = clipped_point;
                        }
                    }
                }
                Some(Transform::Inlay(inlay)) => {
                    if point == cursor.start().0 && inlay.position.bias() == Bias::Right {
                        match cursor.prev_item() {
                            Some(Transform::Inlay(inlay)) => {
                                if inlay.position.bias() == Bias::Left {
                                    return point;
                                }
                            }
                            _ => return point,
                        }
                    } else if point == cursor.end().0 && inlay.position.bias() == Bias::Left {
                        match cursor.next_item() {
                            Some(Transform::Inlay(inlay)) => {
                                if inlay.position.bias() == Bias::Right {
                                    return point;
                                }
                            }
                            _ => return point,
                        }
                    }

                    if bias == Bias::Left {
                        point = cursor.start().0;
                        cursor.prev();
                    } else {
                        cursor.next();
                        point = cursor.start().0;
                    }
                }
                None => {
                    bias = bias.invert();
                    if bias == Bias::Left {
                        point = cursor.start().0;
                        cursor.prev();
                    } else {
                        cursor.next();
                        point = cursor.start().0;
                    }
                }
            }
        }
    }

    pub fn inlay_bias_at_point(&self, point: InlayPoint) -> Option<Bias> {
        let mut cursor = self.transforms.cursor::<Dimensions<InlayPoint, Point>>(());
        cursor.seek(&point, Bias::Left);
        match cursor.item() {
            Some(Transform::Inlay(inlay)) => Some(inlay.position.bias()),
            _ => None,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn text_summary(&self) -> MBTextSummary {
        self.transforms.summary().output
    }

    #[ztracing::instrument(skip_all)]
    pub fn text_summary_for_range(&self, range: Range<InlayOffset>) -> MBTextSummary {
        let mut summary = MBTextSummary::default();

        let mut cursor = self
            .transforms
            .cursor::<Dimensions<InlayOffset, MultiBufferOffset>>(());
        cursor.seek(&range.start, Bias::Right);

        let overshoot = range.start.0 - cursor.start().0.0;
        match cursor.item() {
            Some(Transform::Isomorphic(_)) => {
                let buffer_start = cursor.start().1;
                let suffix_start = buffer_start + overshoot;
                let suffix_end =
                    buffer_start + (cmp::min(cursor.end().0, range.end).0 - cursor.start().0.0);
                summary = self.buffer.text_summary_for_range(suffix_start..suffix_end);
                cursor.next();
            }
            Some(Transform::Inlay(inlay)) => {
                let suffix_start = overshoot;
                let suffix_end = cmp::min(cursor.end().0, range.end).0 - cursor.start().0.0;
                summary = MBTextSummary::from(
                    inlay
                        .text()
                        .cursor(suffix_start)
                        .summary::<TextSummary>(suffix_end),
                );
                cursor.next();
            }
            None => {}
        }

        if range.end > cursor.start().0 {
            summary += cursor
                .summary::<_, TransformSummary>(&range.end, Bias::Right)
                .output;

            let overshoot = range.end.0 - cursor.start().0.0;
            match cursor.item() {
                Some(Transform::Isomorphic(_)) => {
                    let prefix_start = cursor.start().1;
                    let prefix_end = prefix_start + overshoot;
                    summary += self
                        .buffer
                        .text_summary_for_range::<MBTextSummary, _>(prefix_start..prefix_end);
                }
                Some(Transform::Inlay(inlay)) => {
                    let prefix_end = overshoot;
                    summary += inlay.text().cursor(0).summary::<TextSummary>(prefix_end);
                }
                None => {}
            }
        }

        summary
    }

    #[ztracing::instrument(skip_all)]
    pub fn row_infos(&self, row: u32) -> InlayBufferRows<'_> {
        let mut cursor = self.transforms.cursor::<Dimensions<InlayPoint, Point>>(());
        let inlay_point = InlayPoint::new(row, 0);
        cursor.seek(&inlay_point, Bias::Left);

        let max_buffer_row = self.buffer.max_row();
        let mut buffer_point = cursor.start().1;
        let buffer_row = if row == 0 {
            MultiBufferRow(0)
        } else {
            match cursor.item() {
                Some(Transform::Isomorphic(_)) => {
                    buffer_point += inlay_point.0 - cursor.start().0.0;
                    MultiBufferRow(buffer_point.row)
                }
                _ => cmp::min(MultiBufferRow(buffer_point.row + 1), max_buffer_row),
            }
        };

        InlayBufferRows {
            transforms: cursor,
            inlay_row: inlay_point.row(),
            buffer_rows: self.buffer.row_infos(buffer_row),
            max_buffer_row,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn line_len(&self, row: u32) -> u32 {
        let line_start = self.to_offset(InlayPoint::new(row, 0)).0;
        let line_end = if row >= self.max_point().row() {
            self.len().0
        } else {
            self.to_offset(InlayPoint::new(row + 1, 0)).0 - 1
        };
        (line_end - line_start) as u32
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn chunks<'a>(
        &'a self,
        range: Range<InlayOffset>,
        language_aware: LanguageAwareStyling,
        highlights: Highlights<'a>,
    ) -> InlayChunks<'a> {
        let mut cursor = self
            .transforms
            .cursor::<Dimensions<InlayOffset, MultiBufferOffset>>(());
        cursor.seek(&range.start, Bias::Right);

        let buffer_range = self.to_buffer_offset(range.start)..self.to_buffer_offset(range.end);
        let buffer_chunks = CustomHighlightsChunks::new(
            buffer_range,
            language_aware,
            highlights.text_highlights,
            highlights.semantic_token_highlights,
            &self.buffer,
        );

        InlayChunks {
            transforms: cursor,
            buffer_chunks,
            inlay_chunks: None,
            inlay_chunk: None,
            buffer_chunk: None,
            output_offset: range.start,
            max_output_offset: range.end,
            highlight_styles: highlights.styles,
            highlights,
            snapshot: self,
        }
    }

    #[cfg(test)]
    #[ztracing::instrument(skip_all)]
    pub fn text(&self) -> String {
        self.chunks(
            Default::default()..self.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            Highlights::default(),
        )
        .map(|chunk| chunk.chunk.text)
        .collect()
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn check_invariants(&self) {
        #[cfg(any(debug_assertions, feature = "test-support"))]
        {
            assert_eq!(self.transforms.summary().input, self.buffer.text_summary());
            let mut transforms = self.transforms.iter().peekable();
            while let Some(transform) = transforms.next() {
                let transform_is_isomorphic = matches!(transform, Transform::Isomorphic(_));
                if let Some(next_transform) = transforms.peek() {
                    let next_transform_is_isomorphic =
                        matches!(next_transform, Transform::Isomorphic(_));
                    assert!(
                        !transform_is_isomorphic || !next_transform_is_isomorphic,
                        "two adjacent isomorphic transforms"
                    );
                }
            }
        }
    }
}
