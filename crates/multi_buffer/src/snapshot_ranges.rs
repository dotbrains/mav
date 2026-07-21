use super::*;

impl MultiBufferSnapshot {
    pub(super) fn excerpts_for_range<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = &Excerpt> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&range.start);
        std::iter::from_fn(move || {
            let region = cursor.region()?;
            if region.range.start > range.end
                || region.range.start == range.end && region.range.start > range.start
            {
                return None;
            }
            let excerpt = region.excerpt;
            cursor.next_excerpt_forwards();
            Some(excerpt)
        })
    }

    pub fn buffer_ids_for_range<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = BufferId> + '_ {
        self.excerpts_for_range(range)
            .map(|excerpt| excerpt.buffer_snapshot(self).remote_id())
    }

    /// Resolves the given [`text::Anchor`]s to [`crate::Anchor`]s if the anchor is within a visible excerpt.
    ///
    /// The passed in anchors must be ordered.
    pub fn text_anchors_to_visible_anchors(
        &self,
        anchors: impl IntoIterator<Item = text::Anchor>,
    ) -> Vec<Option<Anchor>> {
        let anchors = anchors.into_iter();
        let mut result = Vec::with_capacity(anchors.size_hint().0);
        let mut anchors = anchors.peekable();
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        'anchors: while let Some(anchor) = anchors.peek() {
            let buffer_id = anchor.buffer_id;
            let mut same_buffer_anchors = anchors.peeking_take_while(|a| a.buffer_id == buffer_id);

            if let Some(buffer) = self.buffers.get(&buffer_id) {
                let path = &buffer.path_key;
                let Some(mut next) = same_buffer_anchors.next() else {
                    continue 'anchors;
                };
                cursor.seek_forward(path, Bias::Left);
                'excerpts: while let Some(excerpt) = cursor.item() {
                    if excerpt.path_key != *path {
                        break;
                    }
                    let buffer_snapshot = excerpt.buffer_snapshot(self);

                    loop {
                        // anchor is before the first excerpt
                        if excerpt
                            .range
                            .context
                            .start
                            .cmp(&next, &buffer_snapshot)
                            .is_gt()
                        {
                            // so we skip it and try the next anchor
                            result.push(None);
                            match same_buffer_anchors.next() {
                                Some(anchor) => next = anchor,
                                None => continue 'anchors,
                            }
                        // anchor is within the excerpt
                        } else if excerpt
                            .range
                            .context
                            .end
                            .cmp(&next, &buffer_snapshot)
                            .is_ge()
                        {
                            // record it and all following anchors that are within
                            result.push(Some(Anchor::in_buffer(excerpt.path_key_index, next)));
                            result.extend(
                                same_buffer_anchors
                                    .peeking_take_while(|a| {
                                        excerpt.range.context.end.cmp(a, &buffer_snapshot).is_ge()
                                    })
                                    .map(|a| Some(Anchor::in_buffer(excerpt.path_key_index, a))),
                            );
                            match same_buffer_anchors.next() {
                                Some(anchor) => next = anchor,
                                None => continue 'anchors,
                            }
                        // anchor is after the excerpt, try the next one
                        } else {
                            cursor.next();
                            continue 'excerpts;
                        }
                    }
                }
                // account for `next`
                result.push(None);
            }
            result.extend(same_buffer_anchors.map(|_| None));
        }

        result
    }

    /// Callers should not provide a range where `end < start`
    pub fn range_to_buffer_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Vec<(
        &BufferSnapshot,
        Range<BufferOffset>,
        ExcerptRange<text::Anchor>,
    )> {
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        let range_non_empty = end > start;
        cursor.seek(&start);

        let mut result: Vec<(
            &BufferSnapshot,
            Range<BufferOffset>,
            ExcerptRange<text::Anchor>,
        )> = Vec::new();
        while let Some(region) = cursor.region() {
            if region.range.start > end || (region.range.start == end && range_non_empty) {
                break;
            }
            if region.is_main_buffer {
                let start_overshoot = start.saturating_sub(region.range.start);
                let end_offset = end;
                let end_overshoot = end_offset.saturating_sub(region.range.start);
                let start = region
                    .buffer_range
                    .end
                    .min(region.buffer_range.start + start_overshoot);
                let end = region
                    .buffer_range
                    .end
                    .min(region.buffer_range.start + end_overshoot);
                let excerpt_range = region.excerpt.range.clone();
                if let Some(prev) =
                    result
                        .last_mut()
                        .filter(|(prev_buffer, prev_range, prev_excerpt)| {
                            prev_buffer.remote_id() == region.buffer.remote_id()
                                && prev_range.end == start
                                && prev_excerpt.context.start == excerpt_range.context.start
                        })
                {
                    prev.1.end = end;
                } else {
                    result.push((region.buffer, start..end, excerpt_range));
                }
            }
            cursor.next();
        }

        if let Some(excerpt) = cursor.excerpt()
            && excerpt.text_summary.len == 0
            && end == self.len()
        {
            let buffer_snapshot = excerpt.buffer_snapshot(self);

            let buffer_offset =
                BufferOffset(excerpt.range.context.start.to_offset(buffer_snapshot));
            let excerpt_range = excerpt.range.clone();
            if result
                .last_mut()
                .is_none_or(|(prev_buffer, prev_range, prev_excerpt)| {
                    prev_buffer.remote_id() != buffer_snapshot.remote_id()
                        || prev_range.end != buffer_offset
                        || prev_excerpt.context.start != excerpt_range.context.start
                })
            {
                result.push((buffer_snapshot, buffer_offset..buffer_offset, excerpt_range));
            }
        }

        result
    }

    pub fn range_to_buffer_ranges_with_deleted_hunks<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = (&BufferSnapshot, Range<BufferOffset>, Option<Anchor>)> + '_ {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);

        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&start);

        std::iter::from_fn(move || {
            let region = cursor.region()?;
            if region.range.start > end {
                return None;
            }
            let start_overshoot = start.saturating_sub(region.range.start);
            let end_overshoot = end.saturating_sub(region.range.start);
            let start = region
                .buffer_range
                .end
                .min(region.buffer_range.start + start_overshoot);
            let end = region
                .buffer_range
                .end
                .min(region.buffer_range.start + end_overshoot);

            let deleted_hunk_anchor = if region.is_main_buffer {
                None
            } else {
                Some(self.anchor_before(region.range.start))
            };
            let result = (region.buffer, start..end, deleted_hunk_anchor);
            cursor.next();
            Some(result)
        })
    }

    /// Retrieves buffer metadata for the given range, and converts it into multi-buffer
    /// coordinates.
    ///
    /// The given callback will be called for every excerpt intersecting the given range. It will
    /// be passed the excerpt's buffer and the buffer range that the input range intersects.
    /// The callback should return an iterator of metadata items from that buffer, each paired
    /// with a buffer range.
    ///
    /// The returned iterator yields each of these metadata items, paired with its range in
    /// multi-buffer coordinates.
    pub(super) fn lift_buffer_metadata<'a, MBD, M, I>(
        &'a self,
        query_range: Range<MBD>,
        get_buffer_metadata: impl 'a + Fn(&'a BufferSnapshot, Range<MBD::TextDimension>) -> Option<I>,
    ) -> impl Iterator<Item = (Range<MBD>, M, &'a Excerpt)> + 'a
    where
        I: Iterator<Item = (Range<MBD::TextDimension>, M)> + 'a,
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + ops::Add<MBD::TextDimension, Output = MBD>
            + ops::AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension>
            + ops::Add<Output = MBD::TextDimension>
            + AddAssign<MBD::TextDimension>
            + Ord,
    {
        let mut current_excerpt_metadata: Option<(ExcerptRange<text::Anchor>, I)> = None;
        let mut cursor = self.cursor::<MBD, MBD::TextDimension>();

        // Find the excerpt and buffer offset where the given range ends.
        cursor.seek(&query_range.end);
        let mut range_end = None;
        while let Some(region) = cursor.region() {
            if region.is_main_buffer {
                let mut buffer_end = region.buffer_range.start;
                let overshoot = if query_range.end > region.range.start {
                    query_range.end - region.range.start
                } else {
                    <MBD::TextDimension>::default()
                };
                buffer_end = buffer_end + overshoot;
                range_end = Some((region.excerpt.range.clone(), buffer_end));
                break;
            }
            cursor.next();
        }

        cursor.seek(&query_range.start);

        if let Some(region) = cursor.region().filter(|region| !region.is_main_buffer)
            && region.range.start > MBD::default()
        {
            cursor.prev()
        } else if let Some(region) = cursor.region()
            && region.is_main_buffer
            && region.diff_hunk_status.is_some()
        {
            cursor.prev();
            if cursor.region().is_none_or(|region| region.is_main_buffer) {
                cursor.next();
            }
        }

        iter::from_fn(move || {
            loop {
                let excerpt = cursor.excerpt()?;
                let buffer_snapshot = excerpt.buffer_snapshot(self);

                // If we have already retrieved metadata for this excerpt, continue to use it.
                let metadata_iter = if let Some((_, metadata)) = current_excerpt_metadata
                    .as_mut()
                    .filter(|(excerpt_info, _)| excerpt_info == &excerpt.range)
                {
                    Some(metadata)
                }
                // Otherwise, compute the intersection of the input range with the excerpt's range,
                // and retrieve the metadata for the resulting range.
                else {
                    let region = cursor.region()?;
                    let mut buffer_start;
                    if region.is_main_buffer {
                        buffer_start = region.buffer_range.start;
                        if query_range.start > region.range.start {
                            let overshoot = query_range.start - region.range.start;
                            buffer_start = buffer_start + overshoot;
                        }
                        buffer_start = buffer_start.min(region.buffer_range.end);
                    } else {
                        buffer_start = cursor.main_buffer_position()?;
                    };
                    let mut buffer_end = excerpt
                        .range
                        .context
                        .end
                        .summary::<MBD::TextDimension>(&buffer_snapshot);
                    if let Some((end_excerpt, end_buffer_offset)) = &range_end
                        && &excerpt.range == end_excerpt
                    {
                        buffer_end = buffer_end.min(*end_buffer_offset);
                    }

                    get_buffer_metadata(&buffer_snapshot, buffer_start..buffer_end).map(
                        |iterator| {
                            &mut current_excerpt_metadata
                                .insert((excerpt.range.clone(), iterator))
                                .1
                        },
                    )
                };

                // Visit each metadata item.
                if let Some((metadata_buffer_range, metadata)) =
                    metadata_iter.and_then(Iterator::next)
                {
                    // Find the multibuffer regions that contain the start and end of
                    // the metadata item's range.
                    if metadata_buffer_range.start > <MBD::TextDimension>::default() {
                        while let Some(region) = cursor.region() {
                            if (region.is_main_buffer
                                && (region.buffer_range.end >= metadata_buffer_range.start
                                    || cursor.is_at_end_of_excerpt()))
                                || (!region.is_main_buffer
                                    && region.buffer_range.start == metadata_buffer_range.start)
                            {
                                break;
                            }
                            cursor.next();
                        }
                    }
                    let start_region = cursor.region()?.clone();
                    while let Some(region) = cursor.region() {
                        if region.is_main_buffer
                            && (region.buffer_range.end > metadata_buffer_range.end
                                || cursor.is_at_end_of_excerpt())
                        {
                            break;
                        }
                        cursor.next();
                    }
                    let end_region = cursor.region();

                    // Convert the metadata item's range into multibuffer coordinates.
                    let mut start_position = start_region.range.start;
                    let region_buffer_start = start_region.buffer_range.start;
                    if start_region.is_main_buffer
                        && metadata_buffer_range.start > region_buffer_start
                    {
                        start_position =
                            start_position + (metadata_buffer_range.start - region_buffer_start);
                        start_position = start_position.min(start_region.range.end);
                    }

                    let mut end_position = self.max_position();
                    if let Some(end_region) = &end_region {
                        end_position = end_region.range.start;
                        debug_assert!(end_region.is_main_buffer);
                        let region_buffer_start = end_region.buffer_range.start;
                        if metadata_buffer_range.end > region_buffer_start {
                            end_position =
                                end_position + (metadata_buffer_range.end - region_buffer_start);
                        }
                        end_position = end_position.min(end_region.range.end);
                    }

                    if start_position <= query_range.end && end_position >= query_range.start {
                        return Some((start_position..end_position, metadata, excerpt));
                    }
                }
                // When there are no more metadata items for this excerpt, move to the next excerpt.
                else {
                    current_excerpt_metadata.take();
                    if let Some((end_excerpt, _)) = &range_end
                        && &excerpt.range == end_excerpt
                    {
                        return None;
                    }
                    cursor.next_excerpt_forwards();
                }
            }
        })
    }
}
