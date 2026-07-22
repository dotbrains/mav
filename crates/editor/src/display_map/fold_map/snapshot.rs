use super::*;

#[derive(Clone)]
pub struct FoldSnapshot {
    pub inlay_snapshot: InlaySnapshot,
    transforms: SumTree<Transform>,
    folds: SumTree<Fold>,
    fold_metadata_by_id: TreeMap<FoldId, FoldMetadata>,
    pub version: usize,
}

impl Deref for FoldSnapshot {
    type Target = InlaySnapshot;

    fn deref(&self) -> &Self::Target {
        &self.inlay_snapshot
    }
}

impl FoldSnapshot {
    pub fn buffer(&self) -> &MultiBufferSnapshot {
        &self.inlay_snapshot.buffer
    }

    #[ztracing::instrument(skip_all)]
    fn fold_width(&self, fold_id: &FoldId) -> Option<Pixels> {
        self.fold_metadata_by_id.get(fold_id)?.width
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.chunks(
            FoldOffset(MultiBufferOffset(0))..self.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            Highlights::default(),
        )
        .map(|c| c.text)
        .collect()
    }

    #[cfg(test)]
    pub fn fold_count(&self) -> usize {
        self.folds.items(&self.inlay_snapshot.buffer).len()
    }

    #[inline(always)]
    pub fn has_folds(&self) -> bool {
        !self.folds.is_empty()
    }

    #[ztracing::instrument(skip_all)]
    pub fn text_summary_for_range(&self, range: Range<FoldPoint>) -> MBTextSummary {
        let mut summary = MBTextSummary::default();

        let mut cursor = self
            .transforms
            .cursor::<Dimensions<FoldPoint, InlayPoint>>(());
        cursor.seek(&range.start, Bias::Right);
        if let Some(transform) = cursor.item() {
            let start_in_transform = range.start.0 - cursor.start().0.0;
            let end_in_transform = cmp::min(range.end, cursor.end().0).0 - cursor.start().0.0;
            if let Some(placeholder) = transform.placeholder.as_ref() {
                summary = MBTextSummary::from(
                    &placeholder.text.as_ref()
                        [start_in_transform.column as usize..end_in_transform.column as usize],
                );
            } else {
                let inlay_start = self
                    .inlay_snapshot
                    .to_offset(InlayPoint(cursor.start().1.0 + start_in_transform));
                let inlay_end = self
                    .inlay_snapshot
                    .to_offset(InlayPoint(cursor.start().1.0 + end_in_transform));
                summary = self
                    .inlay_snapshot
                    .text_summary_for_range(inlay_start..inlay_end);
            }
        }

        if range.end > cursor.end().0 {
            cursor.next();
            summary += cursor
                .summary::<_, TransformSummary>(&range.end, Bias::Right)
                .output;
            if let Some(transform) = cursor.item() {
                let end_in_transform = range.end.0 - cursor.start().0.0;
                if let Some(placeholder) = transform.placeholder.as_ref() {
                    summary += MBTextSummary::from(
                        &placeholder.text.as_ref()[..end_in_transform.column as usize],
                    );
                } else {
                    let inlay_start = self.inlay_snapshot.to_offset(cursor.start().1);
                    let inlay_end = self
                        .inlay_snapshot
                        .to_offset(InlayPoint(cursor.start().1.0 + end_in_transform));
                    summary += self
                        .inlay_snapshot
                        .text_summary_for_range(inlay_start..inlay_end);
                }
            }
        }

        summary
    }

    #[ztracing::instrument(skip_all)]
    pub fn to_fold_point(&self, point: InlayPoint, bias: Bias) -> FoldPoint {
        let (start, end, item) = self
            .transforms
            .find::<Dimensions<InlayPoint, FoldPoint>, _>((), &point, Bias::Right);
        if item.is_some_and(|t| t.is_fold()) {
            if bias == Bias::Left || point == start.0 {
                start.1
            } else {
                end.1
            }
        } else {
            let overshoot = point.0 - start.0.0;
            FoldPoint(cmp::min(start.1.0 + overshoot, end.1.0))
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn fold_point_cursor(&self) -> FoldPointCursor<'_> {
        let cursor = self
            .transforms
            .cursor::<Dimensions<InlayPoint, FoldPoint>>(());
        FoldPointCursor { cursor }
    }

    #[ztracing::instrument(skip_all)]
    pub fn len(&self) -> FoldOffset {
        FoldOffset(self.transforms.summary().output.len)
    }

    #[ztracing::instrument(skip_all)]
    pub fn line_len(&self, row: u32) -> u32 {
        let line_start = FoldPoint::new(row, 0).to_offset(self).0;
        let line_end = if row >= self.max_point().row() {
            self.len().0
        } else {
            FoldPoint::new(row + 1, 0).to_offset(self).0 - 1
        };
        (line_end - line_start) as u32
    }

    #[ztracing::instrument(skip_all)]
    pub fn row_infos(&self, start_row: u32) -> FoldRows<'_> {
        if start_row > self.transforms.summary().output.lines.row {
            panic!("invalid display row {}", start_row);
        }

        let fold_point = FoldPoint::new(start_row, 0);
        let mut cursor = self
            .transforms
            .cursor::<Dimensions<FoldPoint, InlayPoint>>(());
        cursor.seek(&fold_point, Bias::Left);

        let overshoot = fold_point.0 - cursor.start().0.0;
        let inlay_point = InlayPoint(cursor.start().1.0 + overshoot);
        let input_rows = self.inlay_snapshot.row_infos(inlay_point.row());

        FoldRows {
            fold_point,
            input_rows,
            cursor,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn max_point(&self) -> FoldPoint {
        FoldPoint(self.transforms.summary().output.lines)
    }

    #[cfg(test)]
    pub fn longest_row(&self) -> u32 {
        self.transforms.summary().output.longest_row
    }

    #[ztracing::instrument(skip_all)]
    pub fn folds_in_range<T>(&self, range: Range<T>) -> impl Iterator<Item = &Fold>
    where
        T: ToOffset,
    {
        let buffer = &self.inlay_snapshot.buffer;
        let range = range.start.to_offset(buffer)..range.end.to_offset(buffer);
        let mut folds = intersecting_folds(&self.inlay_snapshot, &self.folds, range, false);
        iter::from_fn(move || {
            let item = folds.item();
            folds.next();
            item
        })
    }

    #[ztracing::instrument(skip_all)]
    pub fn intersects_fold<T>(&self, offset: T) -> bool
    where
        T: ToOffset,
    {
        let buffer_offset = offset.to_offset(&self.inlay_snapshot.buffer);
        let inlay_offset = self.inlay_snapshot.to_inlay_offset(buffer_offset);
        let (_, _, item) = self
            .transforms
            .find::<InlayOffset, _>((), &inlay_offset, Bias::Right);
        item.is_some_and(|t| t.placeholder.is_some())
    }

    #[ztracing::instrument(skip_all)]
    pub fn is_line_folded(&self, buffer_row: MultiBufferRow) -> bool {
        let mut inlay_point = self
            .inlay_snapshot
            .to_inlay_point(Point::new(buffer_row.0, 0));
        let mut cursor = self.transforms.cursor::<InlayPoint>(());
        cursor.seek(&inlay_point, Bias::Right);
        loop {
            match cursor.item() {
                Some(transform) => {
                    let buffer_point = self.inlay_snapshot.to_buffer_point(inlay_point);
                    if buffer_point.row != buffer_row.0 {
                        return false;
                    } else if transform.placeholder.is_some() {
                        return true;
                    }
                }
                None => return false,
            }

            if cursor.end().row() == inlay_point.row() {
                cursor.next();
            } else {
                inlay_point.0 += Point::new(1, 0);
                cursor.seek(&inlay_point, Bias::Right);
            }
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn chunks<'a>(
        &'a self,
        range: Range<FoldOffset>,
        language_aware: LanguageAwareStyling,
        highlights: Highlights<'a>,
    ) -> FoldChunks<'a> {
        let mut transform_cursor = self
            .transforms
            .cursor::<Dimensions<FoldOffset, InlayOffset>>(());
        transform_cursor.seek(&range.start, Bias::Right);

        let inlay_start = {
            let overshoot = range.start - transform_cursor.start().0;
            transform_cursor.start().1 + overshoot
        };

        let transform_end = transform_cursor.end();

        let inlay_end = if transform_cursor
            .item()
            .is_none_or(|transform| transform.is_fold())
        {
            inlay_start
        } else if range.end < transform_end.0 {
            let overshoot = range.end - transform_cursor.start().0;
            transform_cursor.start().1 + overshoot
        } else {
            transform_end.1
        };

        FoldChunks {
            transform_cursor,
            inlay_chunks: self.inlay_snapshot.chunks(
                inlay_start..inlay_end,
                language_aware,
                highlights,
            ),
            inlay_chunk: None,
            inlay_offset: inlay_start,
            output_offset: range.start,
            max_output_offset: range.end,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn chars_at(&self, start: FoldPoint) -> impl '_ + Iterator<Item = char> {
        self.chunks(
            start.to_offset(self)..self.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            Highlights::default(),
        )
        .flat_map(|chunk| chunk.text.chars())
    }

    #[ztracing::instrument(skip_all)]
    pub fn chunks_at(&self, start: FoldPoint) -> FoldChunks<'_> {
        self.chunks(
            start.to_offset(self)..self.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            Highlights::default(),
        )
    }

    #[cfg(test)]
    #[ztracing::instrument(skip_all)]
    pub fn clip_offset(&self, offset: FoldOffset, bias: Bias) -> FoldOffset {
        if offset > self.len() {
            self.len()
        } else {
            self.clip_point(offset.to_point(self), bias).to_offset(self)
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn clip_point(&self, point: FoldPoint, bias: Bias) -> FoldPoint {
        let (start, end, item) = self
            .transforms
            .find::<Dimensions<FoldPoint, InlayPoint>, _>((), &point, Bias::Right);
        if let Some(transform) = item {
            let transform_start = start.0.0;
            if transform.placeholder.is_some() {
                if point.0 == transform_start || matches!(bias, Bias::Left) {
                    FoldPoint(transform_start)
                } else {
                    FoldPoint(end.0.0)
                }
            } else {
                let overshoot = InlayPoint(point.0 - transform_start);
                let inlay_point = start.1 + overshoot;
                let clipped_inlay_point = self.inlay_snapshot.clip_point(inlay_point, bias);
                FoldPoint(start.0.0 + (clipped_inlay_point - start.1).0)
            }
        } else {
            FoldPoint(self.transforms.summary().output.lines)
        }
    }
}
