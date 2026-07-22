use super::*;

impl BufferDiffSnapshot {
    /// Returns the last hunk whose start is less than or equal to the given position.
    fn hunk_before_base_text_offset<'a>(
        &self,
        target: usize,
        cursor: &mut sum_tree::Cursor<'a, '_, InternalDiffHunk, DiffHunkSummary>,
    ) -> Option<&'a InternalDiffHunk> {
        cursor.seek_forward(&target, Bias::Left);
        if cursor
            .item()
            .is_none_or(|hunk| target < hunk.diff_base_byte_range.start)
        {
            cursor.prev();
        }
        cursor
            .item()
            .filter(|hunk| target >= hunk.diff_base_byte_range.start)
    }

    fn hunk_before_buffer_anchor<'a>(
        &self,
        target: Anchor,
        cursor: &mut sum_tree::Cursor<'a, '_, InternalDiffHunk, DiffHunkSummary>,
        buffer: &text::BufferSnapshot,
    ) -> Option<&'a InternalDiffHunk> {
        cursor.seek_forward(&target, Bias::Left);
        if cursor
            .item()
            .is_none_or(|hunk| target.cmp(&hunk.buffer_range.start, buffer).is_lt())
        {
            cursor.prev();
        }
        cursor
            .item()
            .filter(|hunk| target.cmp(&hunk.buffer_range.start, buffer).is_ge())
    }

    /// Returns a patch mapping the provided main buffer snapshot to the base text of this diff.
    ///
    /// The returned patch is guaranteed to be accurate for all main buffer points in the provided range,
    /// but not necessarily for points outside that range.
    pub fn patch_for_buffer_range<'a>(
        &'a self,
        range: RangeInclusive<Point>,
        buffer: &'a text::BufferSnapshot,
    ) -> Patch<Point> {
        if !self.base_text_exists {
            return Patch::new(vec![Edit {
                old: Point::zero()..buffer.max_point(),
                new: Point::zero()..Point::zero(),
            }]);
        }

        let mut edits_since_diff = Patch::new(
            buffer
                .edits_since::<Point>(&self.buffer_snapshot.version)
                .collect::<Vec<_>>(),
        );
        edits_since_diff.invert();

        let mut start_point = edits_since_diff.old_to_new(*range.start());
        if let Some(first_edit) = edits_since_diff.edits().first() {
            start_point = start_point.min(first_edit.new.start);
        }

        let original_snapshot = self.original_buffer_snapshot();
        let base_text = self.base_text();

        let mut cursor = self.hunks.cursor(original_snapshot);
        self.hunk_before_buffer_anchor(
            original_snapshot.anchor_before(start_point),
            &mut cursor,
            original_snapshot,
        );
        if cursor.item().is_none() {
            cursor.next();
        }

        let mut prefix_edit = cursor.prev_item().map(|prev_hunk| Edit {
            old: Point::zero()..prev_hunk.buffer_range.end.to_point(original_snapshot),
            new: Point::zero()..prev_hunk.diff_base_byte_range.end.to_point(base_text),
        });

        let mut range_end = edits_since_diff.old_to_new(*range.end());
        if let Some(last_edit) = edits_since_diff.edits().last() {
            range_end = range_end.max(last_edit.new.end);
        }
        let range_end = original_snapshot.anchor_before(range_end);

        let hunk_iter = std::iter::from_fn(move || {
            if let Some(edit) = prefix_edit.take() {
                return Some(edit);
            }
            let hunk = cursor.item()?;
            if hunk
                .buffer_range
                .start
                .cmp(&range_end, original_snapshot)
                .is_gt()
            {
                return None;
            }
            let edit = Edit {
                old: hunk.buffer_range.to_point(original_snapshot),
                new: hunk.diff_base_byte_range.to_point(base_text),
            };
            cursor.next();
            Some(edit)
        });

        edits_since_diff.compose(hunk_iter)
    }

    #[cfg(test)]
    pub(crate) fn patch_for_buffer_range_naive<'a>(
        &'a self,
        buffer: &'a text::BufferSnapshot,
    ) -> Patch<Point> {
        let original_snapshot = self.original_buffer_snapshot();

        let edits_since: Vec<Edit<Point>> = buffer
            .edits_since::<Point>(original_snapshot.version())
            .collect();
        let mut inverted_edits_since = Patch::new(edits_since);
        inverted_edits_since.invert();

        inverted_edits_since.compose(
            self.hunks
                .iter()
                .map(|hunk| {
                    let old_start = hunk.buffer_range.start.to_point(original_snapshot);
                    let old_end = hunk.buffer_range.end.to_point(original_snapshot);
                    let new_start = self
                        .base_text()
                        .offset_to_point(hunk.diff_base_byte_range.start);
                    let new_end = self
                        .base_text()
                        .offset_to_point(hunk.diff_base_byte_range.end);
                    Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    }
                })
                .chain(if !self.base_text_exists && self.hunks.is_empty() {
                    Some(Edit {
                        old: Point::zero()..original_snapshot.max_point(),
                        new: Point::zero()..Point::zero(),
                    })
                } else {
                    None
                }),
        )
    }

    /// Returns a patch mapping the base text of this diff to the provided main buffer snapshot.
    ///
    /// The returned patch is guaranteed to be accurate for all base text points in the provided range,
    /// but not necessarily for points outside that range.
    pub fn patch_for_base_text_range<'a>(
        &'a self,
        range: RangeInclusive<Point>,
        buffer: &'a text::BufferSnapshot,
    ) -> Patch<Point> {
        if !self.base_text_exists {
            return Patch::new(vec![Edit {
                old: Point::zero()..Point::zero(),
                new: Point::zero()..buffer.max_point(),
            }]);
        }

        let edits_since_diff = buffer
            .edits_since::<Point>(&self.buffer_snapshot.version)
            .collect::<Vec<_>>();

        let mut hunk_patch = Vec::new();
        let mut cursor = self.hunks.cursor(self.original_buffer_snapshot());
        let hunk_before = self
            .hunk_before_base_text_offset(range.start().to_offset(self.base_text()), &mut cursor);

        if let Some(hunk) = hunk_before
            && let Some(first_edit) = edits_since_diff.first()
            && hunk
                .buffer_range
                .start
                .to_point(self.original_buffer_snapshot())
                > first_edit.old.start
        {
            cursor.reset();
            self.hunk_before_buffer_anchor(
                self.original_buffer_snapshot()
                    .anchor_before(first_edit.old.start),
                &mut cursor,
                self.original_buffer_snapshot(),
            );
        }
        if cursor.item().is_none() {
            cursor.next();
        }
        if let Some(prev_hunk) = cursor.prev_item() {
            hunk_patch.push(Edit {
                old: Point::zero()
                    ..prev_hunk
                        .diff_base_byte_range
                        .end
                        .to_point(self.base_text()),
                new: Point::zero()
                    ..prev_hunk
                        .buffer_range
                        .end
                        .to_point(self.original_buffer_snapshot()),
            })
        }
        let range_end = range.end().to_offset(self.base_text());
        while let Some(hunk) = cursor.item()
            && (hunk.diff_base_byte_range.start <= range_end
                || edits_since_diff.last().is_some_and(|last_edit| {
                    hunk.buffer_range
                        .start
                        .to_point(self.original_buffer_snapshot())
                        <= last_edit.old.end
                }))
        {
            hunk_patch.push(Edit {
                old: hunk.diff_base_byte_range.to_point(self.base_text()),
                new: hunk.buffer_range.to_point(self.original_buffer_snapshot()),
            });
            cursor.next();
        }

        Patch::new(hunk_patch).compose(edits_since_diff)
    }

    #[cfg(test)]
    pub(crate) fn patch_for_base_text_range_naive<'a>(
        &'a self,
        buffer: &'a text::BufferSnapshot,
    ) -> Patch<Point> {
        let original_snapshot = self.original_buffer_snapshot();

        let mut hunk_edits: Vec<Edit<Point>> = Vec::new();
        for hunk in self.hunks.iter() {
            let old_start = self
                .base_text()
                .offset_to_point(hunk.diff_base_byte_range.start);
            let old_end = self
                .base_text()
                .offset_to_point(hunk.diff_base_byte_range.end);
            let new_start = hunk.buffer_range.start.to_point(original_snapshot);
            let new_end = hunk.buffer_range.end.to_point(original_snapshot);
            hunk_edits.push(Edit {
                old: old_start..old_end,
                new: new_start..new_end,
            });
        }
        if !self.base_text_exists && hunk_edits.is_empty() {
            hunk_edits.push(Edit {
                old: Point::zero()..Point::zero(),
                new: Point::zero()..original_snapshot.max_point(),
            })
        }
        let hunk_patch = Patch::new(hunk_edits);

        hunk_patch.compose(buffer.edits_since::<Point>(original_snapshot.version()))
    }

    pub fn buffer_point_to_base_text_range(
        &self,
        point: Point,
        buffer: &text::BufferSnapshot,
    ) -> Range<Point> {
        let patch = self.patch_for_buffer_range(point..=point, buffer);
        let edit = patch.edit_for_old_position(point);
        edit.new
    }

    pub fn base_text_point_to_buffer_range(
        &self,
        point: Point,
        buffer: &text::BufferSnapshot,
    ) -> Range<Point> {
        let patch = self.patch_for_base_text_range(point..=point, buffer);
        let edit = patch.edit_for_old_position(point);
        edit.new
    }

    pub fn buffer_point_to_base_text_point(
        &self,
        point: Point,
        buffer: &text::BufferSnapshot,
    ) -> Point {
        let patch = self.patch_for_buffer_range(point..=point, buffer);
        let edit = patch.edit_for_old_position(point);
        if point == edit.old.end {
            edit.new.end
        } else {
            edit.new.start
        }
    }

    pub fn base_text_point_to_buffer_point(
        &self,
        point: Point,
        buffer: &text::BufferSnapshot,
    ) -> Point {
        let patch = self.patch_for_base_text_range(point..=point, buffer);
        let edit = patch.edit_for_old_position(point);
        if point == edit.old.end {
            edit.new.end
        } else {
            edit.new.start
        }
    }
}
