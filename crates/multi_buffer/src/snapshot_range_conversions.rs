use super::*;

impl MultiBufferSnapshot {
    pub(super) fn excerpts_for_path<'a>(
        &'a self,
        path_key: &'a PathKey,
    ) -> impl Iterator<Item = ExcerptRange<text::Anchor>> + 'a {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(path_key, Bias::Left);
        cursor
            .take_while(move |item| &item.path_key == path_key)
            .map(|excerpt| excerpt.range.clone())
    }

    /// If the given multibuffer range is contained in a single excerpt and contains no deleted hunks,
    /// returns the corresponding buffer range.
    ///
    /// Otherwise, returns None.
    pub fn range_to_buffer_range<MBD>(
        &self,
        range: Range<MBD>,
    ) -> Option<(&BufferSnapshot, Range<MBD::TextDimension>)>
    where
        MBD: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBD as Sub>::Output>,
        MBD::TextDimension: AddAssign<<MBD as Sub>::Output>,
    {
        let mut cursor = self.cursor::<MBD, MBD::TextDimension>();
        cursor.seek(&range.start);

        let start_region = cursor.region()?.clone();

        while let Some(region) = cursor.region()
            && region.range.end < range.end
        {
            if !region.is_main_buffer {
                return None;
            }
            cursor.next();
        }

        let end_region = cursor.region()?;
        if end_region.buffer.remote_id() != start_region.buffer.remote_id() {
            return None;
        }

        let mut buffer_start = start_region.buffer_range.start;
        buffer_start += range.start - start_region.range.start;
        let mut buffer_end = end_region.buffer_range.start;
        buffer_end += range.end - end_region.range.start;

        Some((start_region.buffer, buffer_start..buffer_end))
    }

    /// If the two endpoints of the range lie in the same excerpt, return the corresponding
    /// buffer range. Intervening deleted hunks are allowed.
    pub fn anchor_range_to_buffer_anchor_range(
        &self,
        range: Range<Anchor>,
    ) -> Option<(&BufferSnapshot, Range<text::Anchor>)> {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(&range.start.seek_target(&self), Bias::Left);

        let start_excerpt = cursor.item()?;

        let snapshot = start_excerpt.buffer_snapshot(&self);

        cursor.seek(&range.end.seek_target(&self), Bias::Left);

        let end_excerpt = cursor.item()?;

        if start_excerpt != end_excerpt {
            return None;
        }

        if let Anchor::Excerpt(excerpt_anchor) = range.start
            && (excerpt_anchor.path != start_excerpt.path_key_index
                || excerpt_anchor.buffer_id() != snapshot.remote_id())
        {
            return None;
        }
        if let Anchor::Excerpt(excerpt_anchor) = range.end
            && (excerpt_anchor.path != end_excerpt.path_key_index
                || excerpt_anchor.buffer_id() != snapshot.remote_id())
        {
            return None;
        }

        Some((
            snapshot,
            range.start.text_anchor_in(snapshot)..range.end.text_anchor_in(snapshot),
        ))
    }

    /// Returns all nonempty intersections of the given buffer range with excerpts in the multibuffer in order.
    ///
    /// The multibuffer ranges are split to not intersect deleted hunks.
    pub fn buffer_range_to_excerpt_ranges(
        &self,
        range: Range<text::Anchor>,
    ) -> impl Iterator<Item = Range<Anchor>> {
        assert!(range.start.buffer_id == range.end.buffer_id);

        let buffer_id = range.start.buffer_id;
        self.buffers
            .get(&buffer_id)
            .map(|buffer_state_snapshot| {
                let path_key_index = buffer_state_snapshot.path_key_index;
                let buffer_snapshot = &buffer_state_snapshot.buffer_snapshot;
                let buffer_range = range.to_offset(buffer_snapshot);

                let start = Anchor::in_buffer(path_key_index, range.start).to_offset(self);
                let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
                cursor.seek(&start);
                std::iter::from_fn(move || {
                    while let Some(region) = cursor.region()
                        && !region.is_main_buffer
                    {
                        cursor.next();
                    }

                    let region = cursor.region()?;
                    if region.buffer.remote_id() != buffer_id
                        || region.buffer_range.start > BufferOffset(buffer_range.end)
                    {
                        return None;
                    }

                    let start = region
                        .buffer_range
                        .start
                        .max(BufferOffset(buffer_range.start));
                    let mut end = region.buffer_range.end.min(BufferOffset(buffer_range.end));

                    cursor.next();
                    while let Some(region) = cursor.region()
                        && region.is_main_buffer
                        && region.buffer.remote_id() == buffer_id
                        && region.buffer_range.start <= end
                    {
                        end = end
                            .max(region.buffer_range.end)
                            .min(BufferOffset(buffer_range.end));
                        cursor.next();
                    }

                    let multibuffer_range = Anchor::range_in_buffer(
                        path_key_index,
                        buffer_snapshot.anchor_range_inside(start..end),
                    );
                    Some(multibuffer_range)
                })
            })
            .into_iter()
            .flatten()
    }

    pub fn buffers_with_paths<'a>(
        &'a self,
    ) -> impl 'a + Iterator<Item = (&'a BufferSnapshot, &'a PathKey)> {
        self.buffers
            .values()
            .map(|buffer| (&buffer.buffer_snapshot, &buffer.path_key))
    }
}
