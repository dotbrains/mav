use super::*;

impl MultiBufferSnapshot {
    pub fn summary_for_anchor<MBD>(&self, anchor: &Anchor) -> MBD
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + Sub<MBD::TextDimension, Output = MBD>
            + AddAssign<MBD::TextDimension>
            + Add<MBD::TextDimension, Output = MBD>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
    {
        let target = anchor.seek_target(self);
        let anchor = match anchor {
            Anchor::Min => {
                return MBD::default();
            }
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
            Anchor::Max => {
                return MBD::from_summary(&self.text_summary());
            }
        };

        let (start, _, item) = self
            .excerpts
            .find::<ExcerptSummary, _>((), &target, Bias::Left);
        let start = MBD::from_summary(&start.text);

        let excerpt_start_position = ExcerptDimension(start);
        if self.diff_transforms.is_empty() {
            if let Some(excerpt) = item {
                if !excerpt.contains(anchor, self) {
                    return excerpt_start_position.0;
                }
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let buffer_summary = anchor
                    .text_anchor()
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                let mut position = excerpt_start_position;
                if summary > excerpt_buffer_start {
                    position += summary - excerpt_buffer_start;
                }

                position.0
            } else {
                excerpt_start_position.0
            }
        } else {
            let mut diff_transforms_cursor = self
                .diff_transforms
                .cursor::<Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>>(());

            if let Some(excerpt) = item {
                if !excerpt.contains(anchor, self) {
                    diff_transforms_cursor.seek(&excerpt_start_position, Bias::Left);
                    return self.summary_for_excerpt_position_without_hunks(
                        Bias::Left,
                        excerpt_start_position,
                        &mut diff_transforms_cursor,
                    );
                }
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let buffer_summary = anchor
                    .text_anchor()
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                let mut position = excerpt_start_position;
                if summary > excerpt_buffer_start {
                    position += summary - excerpt_buffer_start;
                }

                diff_transforms_cursor.seek(&position, Bias::Left);
                self.summary_for_anchor_with_excerpt_position(
                    *anchor,
                    position,
                    &mut diff_transforms_cursor,
                    &buffer_snapshot,
                )
            } else {
                diff_transforms_cursor.seek(&excerpt_start_position, Bias::Left);
                self.summary_for_excerpt_position_without_hunks(
                    Bias::Right,
                    excerpt_start_position,
                    &mut diff_transforms_cursor,
                )
            }
        }
    }

    /// Maps an anchor's excerpt-space position to its output-space position by
    /// walking the diff transforms. The cursor is shared across consecutive
    /// calls, so it may already be partway through the transform list.
    fn summary_for_anchor_with_excerpt_position<MBD>(
        &self,
        anchor: ExcerptAnchor,
        excerpt_position: ExcerptDimension<MBD>,
        diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>,
        >,
        excerpt_buffer: &text::BufferSnapshot,
    ) -> MBD
    where
        MBD: MultiBufferDimension + Ord + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        loop {
            let transform_end_position = diff_transforms.end().0;
            let item = diff_transforms.item();
            let at_transform_end = transform_end_position == excerpt_position && item.is_some();

            // A right-biased anchor at a transform boundary belongs to the
            // *next* transform, so advance past the current one.
            if anchor.text_anchor.bias == Bias::Right && at_transform_end {
                diff_transforms.next();
                continue;
            }

            let mut position = diff_transforms.start().1;
            match item {
                Some(DiffTransform::DeletedHunk {
                    buffer_id,
                    base_text_byte_range,
                    hunk_info,
                    ..
                }) => {
                    if let Some(diff_base_anchor) = anchor.diff_base_anchor
                        && let Some(base_text) =
                            self.diff_state(*buffer_id).map(|diff| diff.base_text())
                        && diff_base_anchor.is_valid(&base_text)
                    {
                        // The anchor carries a diff-base position — resolve it
                        // to a location inside the deleted hunk.
                        let base_text_offset = diff_base_anchor.to_offset(base_text);
                        if base_text_offset >= base_text_byte_range.start
                            && base_text_offset <= base_text_byte_range.end
                        {
                            let position_in_hunk = base_text
                                .text_summary_for_range::<MBD::TextDimension, _>(
                                    base_text_byte_range.start..base_text_offset,
                                );
                            position.0.add_text_dim(&position_in_hunk);
                        } else if at_transform_end {
                            // diff_base offset falls outside this hunk's range;
                            // advance to see if the next transform is a better fit.
                            diff_transforms.next();
                            continue;
                        }
                    } else if at_transform_end
                        && anchor
                            .text_anchor()
                            .cmp(&hunk_info.hunk_start_anchor, excerpt_buffer)
                            .is_gt()
                    {
                        // The anchor has no (valid) diff-base position, so it
                        // belongs in the buffer content, not in the deleted
                        // hunk. However, after an edit deletes the text between
                        // the hunk boundary and this anchor, both resolve to
                        // the same excerpt_position—landing us here on the
                        // DeletedHunk left behind by the shared cursor. Use the
                        // CRDT ordering to detect that the anchor is strictly
                        // *past* the hunk boundary and skip to the following
                        // BufferContent.
                        diff_transforms.next();
                        continue;
                    }
                }
                _ => {
                    // On a BufferContent (or no transform). If the anchor
                    // carries a diff_base_anchor it needs a DeletedHunk, so
                    // advance to find one.
                    if at_transform_end && anchor.diff_base_anchor.is_some() {
                        diff_transforms.next();
                        continue;
                    }
                    let overshoot = excerpt_position - diff_transforms.start().0;
                    position += overshoot;
                }
            }

            return position.0;
        }
    }

    /// Like `resolve_summary_for_anchor` but optimized for min/max anchors.
    fn summary_for_excerpt_position_without_hunks<MBD>(
        &self,
        bias: Bias,
        excerpt_position: ExcerptDimension<MBD>,
        diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>,
        >,
    ) -> MBD
    where
        MBD: MultiBufferDimension + Ord + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        loop {
            let transform_end_position = diff_transforms.end().0;
            let item = diff_transforms.item();
            let at_transform_end = transform_end_position == excerpt_position && item.is_some();

            // A right-biased anchor at a transform boundary belongs to the
            // *next* transform, so advance past the current one.
            if bias == Bias::Right && at_transform_end {
                diff_transforms.next();
                continue;
            }

            let mut position = diff_transforms.start().1;
            if let Some(DiffTransform::BufferContent { .. }) | None = item {
                let overshoot = excerpt_position - diff_transforms.start().0;
                position += overshoot;
            }

            return position.0;
        }
    }

    pub(super) fn excerpt_offset_for_anchor(&self, anchor: &Anchor) -> ExcerptOffset {
        let anchor = match anchor {
            Anchor::Min => return ExcerptOffset::default(),
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
            Anchor::Max => return self.excerpts.summary().len(),
        };
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        let target = anchor.seek_target(self);

        cursor.seek(&target, Bias::Left);

        let mut position = cursor.start().len();
        if let Some(excerpt) = cursor.item()
            && excerpt.contains(anchor, self)
        {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let excerpt_buffer_start =
                buffer_snapshot.offset_for_anchor(&excerpt.range.context.start);
            let excerpt_buffer_end = buffer_snapshot.offset_for_anchor(&excerpt.range.context.end);
            let buffer_position = cmp::min(
                excerpt_buffer_end,
                buffer_snapshot.offset_for_anchor(&anchor.text_anchor()),
            );
            if buffer_position > excerpt_buffer_start {
                position += buffer_position - excerpt_buffer_start;
            }
        }
        position
    }

    pub fn summaries_for_anchors<'a, MBD, I>(&'a self, anchors: I) -> Vec<MBD>
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
        I: 'a + IntoIterator<Item = &'a Anchor>,
    {
        let mut summaries = Vec::new();
        self.summaries_for_anchors_cb(anchors, |summary| summaries.push(summary));
        summaries
    }

    pub fn summaries_for_anchors_cb<'a, MBD, I>(&'a self, anchors: I, mut cb: impl FnMut(MBD))
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
        I: 'a + IntoIterator<Item = &'a Anchor>,
    {
        let mut anchors = anchors.into_iter().peekable();
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        let mut diff_transforms_cursor = self
            .diff_transforms
            .cursor::<Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>>(());
        diff_transforms_cursor.next();

        while let Some(anchor) = anchors.peek() {
            let target = anchor.seek_target(self);
            let excerpt_anchor = match anchor {
                Anchor::Min => {
                    cb(MBD::default());
                    anchors.next();
                    continue;
                }
                Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
                Anchor::Max => {
                    cb(MBD::from_summary(&self.text_summary()));
                    anchors.next();
                    continue;
                }
            };

            cursor.seek_forward(&target, Bias::Left);

            let excerpt_start_position = ExcerptDimension(MBD::from_summary(&cursor.start().text));
            if let Some(excerpt) = cursor.item() {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                if !excerpt.contains(&excerpt_anchor, self) {
                    diff_transforms_cursor.seek_forward(&excerpt_start_position, Bias::Left);
                    let position = self.summary_for_excerpt_position_without_hunks(
                        Bias::Left,
                        excerpt_start_position,
                        &mut diff_transforms_cursor,
                    );
                    cb(position);
                    anchors.next();
                    continue;
                }
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(buffer_snapshot);
                for (buffer_summary, excerpt_anchor) in buffer_snapshot
                    .summaries_for_anchors_with_payload::<MBD::TextDimension, _, _>(
                        std::iter::from_fn(|| {
                            let excerpt_anchor = anchors.peek()?.excerpt_anchor()?;
                            if !excerpt.contains(&excerpt_anchor, self) {
                                return None;
                            }
                            anchors.next();
                            Some((excerpt_anchor.text_anchor(), excerpt_anchor))
                        }),
                    )
                {
                    let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                    let mut position = excerpt_start_position;
                    if summary > excerpt_buffer_start {
                        position += summary - excerpt_buffer_start;
                    }

                    if diff_transforms_cursor.start().0 < position {
                        diff_transforms_cursor.seek_forward(&position, Bias::Left);
                    }

                    cb(self.summary_for_anchor_with_excerpt_position(
                        excerpt_anchor,
                        position,
                        &mut diff_transforms_cursor,
                        &buffer_snapshot,
                    ));
                }
            } else {
                diff_transforms_cursor.seek_forward(&excerpt_start_position, Bias::Left);
                let position = self.summary_for_excerpt_position_without_hunks(
                    Bias::Right,
                    excerpt_start_position,
                    &mut diff_transforms_cursor,
                );
                cb(position);
                anchors.next();
            }
        }
    }

    pub fn dimensions_from_points<'a, MBD>(
        &'a self,
        points: impl 'a + IntoIterator<Item = Point>,
    ) -> impl 'a + Iterator<Item = MBD>
    where
        MBD: MultiBufferDimension + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        let mut cursor = self.cursor::<DimensionPair<Point, MBD>, Point>();
        cursor.seek(&DimensionPair {
            key: Point::default(),
            value: None,
        });
        let mut points = points.into_iter();
        iter::from_fn(move || {
            let point = points.next()?;

            cursor.seek_forward(&DimensionPair {
                key: point,
                value: None,
            });

            if let Some(region) = cursor.region() {
                let overshoot = point - region.range.start.key;
                let buffer_point = region.buffer_range.start + overshoot;
                let mut position = region.range.start.value.unwrap();
                position.add_text_dim(
                    &region
                        .buffer
                        .text_summary_for_range(region.buffer_range.start..buffer_point),
                );
                if point == region.range.end.key && region.has_trailing_newline {
                    position.add_mb_text_summary(&MBTextSummary::from(TextSummary::newline()));
                }
                Some(position)
            } else {
                Some(MBD::from_summary(&self.text_summary()))
            }
        })
    }

    pub fn excerpts_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> impl Iterator<Item = ExcerptRange<text::Anchor>> {
        if let Some(buffer_state) = self.buffers.get(&buffer_id) {
            let path_key = buffer_state.path_key.clone();
            let mut cursor = self.excerpts.cursor::<PathKey>(());
            cursor.seek_forward(&path_key, Bias::Left);
            Some(iter::from_fn(move || {
                let excerpt = cursor.item()?;
                if excerpt.path_key != path_key {
                    return None;
                }
                cursor.next();
                Some(excerpt.range.clone())
            }))
        } else {
            None
        }
        .into_iter()
        .flatten()
    }
}
