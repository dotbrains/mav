use super::*;

impl MultiBufferSnapshot {
    pub fn diff_hunks(&self) -> impl Iterator<Item = MultiBufferDiffHunk> + '_ {
        self.diff_hunks_in_range(Anchor::Min..Anchor::Max)
    }

    pub fn diff_hunks_in_range<T: ToPoint>(
        &self,
        range: Range<T>,
    ) -> impl Iterator<Item = MultiBufferDiffHunk> + '_ {
        let query_range = range.start.to_point(self)..range.end.to_point(self);
        self.lift_buffer_metadata(query_range.clone(), move |buffer, buffer_range| {
            let diff = self.diff_state(buffer.remote_id())?;
            let iter = if let Some(main_buffer) = &diff.main_buffer {
                let buffer_start = buffer.point_to_offset(buffer_range.start);
                let buffer_end = buffer.point_to_offset(buffer_range.end);
                itertools::Either::Left(
                    diff.hunks_intersecting_base_text_range(buffer_start..buffer_end, main_buffer)
                        .map(move |hunk| (hunk, buffer, true)),
                )
            } else {
                let buffer_start = buffer.anchor_before(buffer_range.start);
                let buffer_end = buffer.anchor_after(buffer_range.end);
                itertools::Either::Right(
                    diff.hunks_intersecting_range(buffer_start..buffer_end, buffer)
                        .map(move |hunk| (hunk, buffer, false)),
                )
            };
            Some(iter.filter_map(|(hunk, buffer, is_inverted)| {
                if hunk.is_created_file() && !self.all_diff_hunks_expanded {
                    return None;
                }
                let range = if is_inverted {
                    hunk.diff_base_byte_range.to_point(&buffer)
                } else {
                    hunk.range.clone()
                };
                Some((range, (hunk, is_inverted)))
            }))
        })
        .filter_map(move |(range, (hunk, is_inverted), excerpt)| {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            if range.start != range.end && range.end == query_range.start && !hunk.range.is_empty()
            {
                return None;
            }
            let end_row = if range.end.column == 0 {
                range.end.row
            } else {
                range.end.row + 1
            };

            let word_diffs =
                (!hunk.base_word_diffs.is_empty() || !hunk.buffer_word_diffs.is_empty())
                    .then(|| {
                        let mut word_diffs = Vec::new();

                        if self.show_deleted_hunks || is_inverted {
                            let hunk_start_offset = if is_inverted {
                                Anchor::in_buffer(
                                    excerpt.path_key_index,
                                    buffer_snapshot.anchor_after(hunk.diff_base_byte_range.start),
                                )
                                .to_offset(self)
                            } else {
                                Anchor::in_buffer(excerpt.path_key_index, hunk.buffer_range.start)
                                    .to_offset(self)
                            };

                            word_diffs.extend(hunk.base_word_diffs.iter().map(|diff| {
                                hunk_start_offset + diff.start..hunk_start_offset + diff.end
                            }));
                        }

                        if !is_inverted {
                            word_diffs.extend(hunk.buffer_word_diffs.into_iter().map(|diff| {
                                Anchor::range_in_buffer(excerpt.path_key_index, diff)
                                    .to_offset(self)
                            }));
                        }
                        word_diffs
                    })
                    .unwrap_or_default();

            let buffer_range = if is_inverted {
                buffer_snapshot.anchor_after(hunk.diff_base_byte_range.start)
                    ..buffer_snapshot.anchor_before(hunk.diff_base_byte_range.end)
            } else {
                hunk.buffer_range.clone()
            };
            let status_kind = if hunk.buffer_range.start == hunk.buffer_range.end {
                DiffHunkStatusKind::Deleted
            } else if hunk.diff_base_byte_range.is_empty() {
                DiffHunkStatusKind::Added
            } else {
                DiffHunkStatusKind::Modified
            };
            let multi_buffer_range =
                Anchor::range_in_buffer(excerpt.path_key_index, buffer_range.clone());
            Some(MultiBufferDiffHunk {
                row_range: MultiBufferRow(range.start.row)..MultiBufferRow(end_row),
                buffer_id: buffer_snapshot.remote_id(),
                buffer_range,
                word_diffs,
                diff_base_byte_range: BufferOffset(hunk.diff_base_byte_range.start)
                    ..BufferOffset(hunk.diff_base_byte_range.end),
                status: DiffHunkStatus {
                    kind: status_kind,
                    secondary: hunk.secondary_status,
                },
                excerpt_range: excerpt.range.clone(),
                multi_buffer_range,
            })
        })
    }
}
