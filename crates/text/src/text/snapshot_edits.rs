use super::*;

impl BufferSnapshot {
    pub fn edits_since<'a, D>(
        &'a self,
        since: &'a clock::Global,
    ) -> impl 'a + Iterator<Item = Edit<D>>
    where
        D: TextDimension + Ord,
    {
        self.edits_since_in_range(
            since,
            Anchor::min_for_buffer(self.remote_id)..Anchor::max_for_buffer(self.remote_id),
        )
    }

    pub fn anchored_edits_since<'a, D>(
        &'a self,
        since: &'a clock::Global,
    ) -> impl 'a + Iterator<Item = (Edit<D>, Range<Anchor>)>
    where
        D: TextDimension + Ord,
    {
        self.anchored_edits_since_in_range(
            since,
            Anchor::min_for_buffer(self.remote_id)..Anchor::max_for_buffer(self.remote_id),
        )
    }

    pub fn edits_since_in_range<'a, D>(
        &'a self,
        since: &'a clock::Global,
        range: Range<Anchor>,
    ) -> impl 'a + Iterator<Item = Edit<D>>
    where
        D: TextDimension + Ord,
    {
        self.anchored_edits_since_in_range(since, range)
            .map(|item| item.0)
    }

    pub fn anchored_edits_since_in_range<'a, D>(
        &'a self,
        since: &'a clock::Global,
        range: Range<Anchor>,
    ) -> impl 'a + Iterator<Item = (Edit<D>, Range<Anchor>)>
    where
        D: TextDimension + Ord,
    {
        if *since == self.version {
            return None.into_iter().flatten();
        }
        let mut cursor = self.fragments.filter(&None, move |summary| {
            !since.observed_all(&summary.max_version)
        });
        cursor.next();
        let fragments_cursor = Some(cursor);
        let start_fragment_id = self.fragment_id_for_anchor(&range.start);
        let (start, _, item) = self
            .fragments
            .find::<Dimensions<Option<&Locator>, FragmentTextSummary>, _>(
                &None,
                &Some(start_fragment_id),
                Bias::Left,
            );
        let mut visible_start = start.1.visible;
        let mut deleted_start = start.1.deleted;
        if let Some(fragment) = item {
            let overshoot = (range.start.offset - fragment.insertion_offset) as usize;
            if fragment.visible {
                visible_start += overshoot;
            } else {
                deleted_start += overshoot;
            }
        }
        let end_fragment_id = self.fragment_id_for_anchor(&range.end);

        Some(Edits {
            visible_cursor: self.visible_text.cursor(visible_start),
            deleted_cursor: self.deleted_text.cursor(deleted_start),
            fragments_cursor,
            undos: &self.undo_map,
            since,
            old_end: D::zero(()),
            new_end: D::zero(()),
            range: (start_fragment_id, range.start.offset)..(end_fragment_id, range.end.offset),
            buffer_id: self.remote_id,
        })
        .into_iter()
        .flatten()
    }

    pub fn has_edits_since_in_range(&self, since: &clock::Global, range: Range<Anchor>) -> bool {
        if *since != self.version {
            let start_fragment_id = self.fragment_id_for_anchor(&range.start);
            let end_fragment_id = self.fragment_id_for_anchor(&range.end);
            let mut cursor = self.fragments.filter::<_, usize>(&None, move |summary| {
                !since.observed_all(&summary.max_version)
            });
            cursor.next();
            while let Some(fragment) = cursor.item() {
                if fragment.id > *end_fragment_id {
                    break;
                }
                if fragment.id > *start_fragment_id {
                    let was_visible = fragment.was_visible(since, &self.undo_map);
                    let is_visible = fragment.visible;
                    if was_visible != is_visible {
                        return true;
                    }
                }
                cursor.next();
            }
        }
        false
    }

    pub fn has_edits_since(&self, since: &clock::Global) -> bool {
        if *since != self.version {
            let mut cursor = self.fragments.filter::<_, usize>(&None, move |summary| {
                !since.observed_all(&summary.max_version)
            });
            cursor.next();
            while let Some(fragment) = cursor.item() {
                let was_visible = fragment.was_visible(since, &self.undo_map);
                let is_visible = fragment.visible;
                if was_visible != is_visible {
                    return true;
                }
                cursor.next();
            }
        }
        false
    }

    pub fn range_to_version(&self, range: Range<usize>, version: &clock::Global) -> Range<usize> {
        let mut offsets = self.offsets_to_version([range.start, range.end], version);
        offsets.next().unwrap()..offsets.next().unwrap()
    }

    /// Converts the given sequence of offsets into their corresponding offsets
    /// at a prior version of this buffer.
    pub fn offsets_to_version<'a>(
        &'a self,
        offsets: impl 'a + IntoIterator<Item = usize>,
        version: &'a clock::Global,
    ) -> impl 'a + Iterator<Item = usize> {
        let mut edits = self.edits_since(version).peekable();
        let mut last_old_end = 0;
        let mut last_new_end = 0;
        offsets.into_iter().map(move |new_offset| {
            while let Some(edit) = edits.peek() {
                if edit.new.start > new_offset {
                    break;
                }

                if edit.new.end <= new_offset {
                    last_new_end = edit.new.end;
                    last_old_end = edit.old.end;
                    edits.next();
                    continue;
                }

                let overshoot = new_offset - edit.new.start;
                return (edit.old.start + overshoot).min(edit.old.end);
            }

            last_old_end + new_offset.saturating_sub(last_new_end)
        })
    }

    /// Visually annotates a position or range with the `Debug` representation of a value. The
    /// callsite of this function is used as a key - previous annotations will be removed.
    #[cfg(debug_assertions)]
    #[track_caller]
    pub fn debug<R, V>(&self, ranges: &R, value: V)
    where
        R: debug::ToDebugRanges,
        V: std::fmt::Debug,
    {
        self.debug_with_key(std::panic::Location::caller(), ranges, value);
    }

    /// Visually annotates a position or range with the `Debug` representation of a value. Previous
    /// debug annotations with the same key will be removed. The key is also used to determine the
    /// annotation's color.
    #[cfg(debug_assertions)]
    pub fn debug_with_key<K, R, V>(&self, key: &K, ranges: &R, value: V)
    where
        K: std::hash::Hash + 'static,
        R: debug::ToDebugRanges,
        V: std::fmt::Debug,
    {
        let ranges = ranges
            .to_debug_ranges(self)
            .into_iter()
            .map(|range| self.anchor_after(range.start)..self.anchor_before(range.end))
            .collect();
        debug::GlobalDebugRanges::with_locked(|debug_ranges| {
            debug_ranges.insert(key, ranges, format!("{value:?}").into());
        });
    }
}
