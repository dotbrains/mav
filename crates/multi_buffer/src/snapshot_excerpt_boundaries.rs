use super::*;

impl MultiBufferSnapshot {
    pub fn excerpt_before(&self, anchor: Anchor) -> Option<ExcerptRange<text::Anchor>> {
        let target = anchor.try_seek_target(&self)?;
        let mut excerpts = self.excerpts.cursor::<ExcerptSummary>(());
        excerpts.seek(&target, Bias::Left);
        excerpts.prev();
        Some(excerpts.item()?.range.clone())
    }

    pub fn excerpt_boundaries_in_range<R, T>(
        &self,
        range: R,
    ) -> impl Iterator<Item = ExcerptBoundary> + '_
    where
        R: RangeBounds<T>,
        T: ToOffset,
    {
        let start_offset;
        let start = match range.start_bound() {
            Bound::Included(start) => {
                start_offset = start.to_offset(self);
                Bound::Included(start_offset)
            }
            Bound::Excluded(_) => {
                panic!("not supported")
            }
            Bound::Unbounded => {
                start_offset = MultiBufferOffset::ZERO;
                Bound::Unbounded
            }
        };
        let end = match range.end_bound() {
            Bound::Included(end) => Bound::Included(end.to_offset(self)),
            Bound::Excluded(end) => Bound::Excluded(end.to_offset(self)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let bounds = (start, end);
        let mut cursor = self.cursor::<DimensionPair<MultiBufferOffset, Point>, BufferOffset>();
        cursor.seek(&DimensionPair {
            key: start_offset,
            value: None,
        });

        if cursor
            .fetch_excerpt_with_range()
            .is_some_and(|(_, range)| bounds.contains(&range.start.key))
        {
            cursor.prev_excerpt();
        } else {
            cursor.seek_to_start_of_current_excerpt();
        }
        let mut prev_excerpt = cursor
            .fetch_excerpt_with_range()
            .map(|(excerpt, _)| excerpt);

        cursor.next_excerpt_forwards();

        iter::from_fn(move || {
            loop {
                if self.singleton {
                    return None;
                }

                let (next_excerpt, next_range) = cursor.fetch_excerpt_with_range()?;
                cursor.next_excerpt_forwards();
                if !bounds.contains(&next_range.start.key) {
                    prev_excerpt = Some(next_excerpt);
                    continue;
                }

                let next_region_start = next_range.start.value.unwrap();
                let next_region_end = if let Some((_, range)) = cursor.fetch_excerpt_with_range() {
                    range.start.value.unwrap()
                } else {
                    self.max_point()
                };

                let prev = prev_excerpt.as_ref().map(|excerpt| ExcerptBoundaryInfo {
                    start_anchor: Anchor::in_buffer(
                        excerpt.path_key_index,
                        excerpt.range.context.start,
                    ),
                    range: excerpt.range.clone(),
                    end_row: MultiBufferRow(next_region_start.row),
                });

                let next = ExcerptBoundaryInfo {
                    start_anchor: Anchor::in_buffer(
                        next_excerpt.path_key_index,
                        next_excerpt.range.context.start,
                    ),
                    range: next_excerpt.range.clone(),
                    end_row: if next_excerpt.has_trailing_newline {
                        MultiBufferRow(next_region_end.row - 1)
                    } else {
                        MultiBufferRow(next_region_end.row)
                    },
                };

                let row = MultiBufferRow(next_region_start.row);

                prev_excerpt = Some(next_excerpt);

                return Some(ExcerptBoundary { row, prev, next });
            }
        })
    }

    pub fn edit_count(&self) -> usize {
        self.edit_count
    }

    pub fn non_text_state_update_count(&self) -> usize {
        self.non_text_state_update_count
    }

    /// Allows converting several ranges within the same excerpt between buffer offsets and multibuffer offsets.
    ///
    /// If the input range is contained in a single excerpt, invokes the callback with the full range of that excerpt
    /// and the input range both converted to buffer coordinates. The buffer ranges returned by the callback are lifted back
    /// to multibuffer offsets and returned.
    ///
    /// Returns `None` if the input range spans multiple excerpts.
    pub fn map_excerpt_ranges<'a, T>(
        &'a self,
        position: Range<MultiBufferOffset>,
        f: impl FnOnce(
            &'a BufferSnapshot,
            ExcerptRange<BufferOffset>,
            Range<BufferOffset>,
        ) -> Vec<(Range<BufferOffset>, T)>,
    ) -> Option<Vec<(Range<MultiBufferOffset>, T)>> {
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&position.start);

        let region = cursor.region()?;
        if !region.is_main_buffer {
            return None;
        }
        let excerpt = cursor.excerpt()?;
        let excerpt_start = *cursor.excerpts.start();
        let input_buffer_start = cursor.buffer_position_at(&position.start)?;

        cursor.seek_forward(&position.end);
        if cursor.excerpt()? != excerpt {
            return None;
        }
        let region = cursor.region()?;
        if !region.is_main_buffer {
            return None;
        }
        let input_buffer_end = cursor.buffer_position_at(&position.end)?;
        let input_buffer_range = input_buffer_start..input_buffer_end;
        let buffer = excerpt.buffer_snapshot(self);
        let excerpt_context_range = excerpt.range.context.to_offset(buffer);
        let excerpt_context_range =
            BufferOffset(excerpt_context_range.start)..BufferOffset(excerpt_context_range.end);
        let excerpt_primary_range = excerpt.range.primary.to_offset(buffer);
        let excerpt_primary_range =
            BufferOffset(excerpt_primary_range.start)..BufferOffset(excerpt_primary_range.end);
        let results = f(
            buffer,
            ExcerptRange {
                context: excerpt_context_range.clone(),
                primary: excerpt_primary_range,
            },
            input_buffer_range,
        );
        let mut diff_transforms = cursor.diff_transforms;
        Some(
            results
                .into_iter()
                .map(|(buffer_range, metadata)| {
                    let clamped_start = buffer_range
                        .start
                        .max(excerpt_context_range.start)
                        .min(excerpt_context_range.end);
                    let clamped_end = buffer_range
                        .end
                        .max(clamped_start)
                        .min(excerpt_context_range.end);
                    let excerpt_offset_start =
                        excerpt_start + (clamped_start.0 - excerpt_context_range.start.0);
                    let excerpt_offset_end =
                        excerpt_start + (clamped_end.0 - excerpt_context_range.start.0);

                    diff_transforms.seek(&excerpt_offset_start, Bias::Right);
                    let mut output_start = diff_transforms.start().output_dimension;
                    output_start +=
                        excerpt_offset_start - diff_transforms.start().excerpt_dimension;

                    diff_transforms.seek_forward(&excerpt_offset_end, Bias::Right);
                    let mut output_end = diff_transforms.start().output_dimension;
                    output_end += excerpt_offset_end - diff_transforms.start().excerpt_dimension;

                    (output_start.0..output_end.0, metadata)
                })
                .collect(),
        )
    }
}
