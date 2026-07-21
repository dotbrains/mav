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

impl MultiBufferSnapshot {
    pub fn diff_hunk_before<T: ToOffset>(&self, position: T) -> Option<MultiBufferRow> {
        let offset = position.to_offset(self);

        let mut cursor = self
            .cursor::<DimensionPair<MultiBufferOffset, Point>, DimensionPair<BufferOffset, Point>>(
            );
        cursor.seek(&DimensionPair {
            key: offset,
            value: None,
        });
        cursor.seek_to_start_of_current_excerpt();
        let excerpt = cursor.excerpt()?;

        let buffer = excerpt.buffer_snapshot(self);
        let excerpt_start = excerpt.range.context.start.to_offset(buffer);
        let excerpt_end = excerpt.range.context.end.to_offset(buffer);
        let current_position = match self.anchor_before(offset) {
            Anchor::Min => 0,
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor.text_anchor().to_offset(buffer),
            Anchor::Max => unreachable!(),
        };

        if let Some(diff) = self.diff_state(excerpt.buffer_id) {
            if let Some(main_buffer) = &diff.main_buffer {
                for hunk in diff
                    .hunks_intersecting_base_text_range_rev(excerpt_start..excerpt_end, main_buffer)
                {
                    if hunk.diff_base_byte_range.end >= current_position {
                        continue;
                    }
                    let hunk_start = buffer.anchor_after(hunk.diff_base_byte_range.start);
                    let start =
                        Anchor::in_buffer(excerpt.path_key_index, hunk_start).to_point(self);
                    return Some(MultiBufferRow(start.row));
                }
            } else {
                let excerpt_end = buffer.anchor_before(excerpt_end.min(current_position));
                for hunk in diff
                    .hunks_intersecting_range_rev(excerpt.range.context.start..excerpt_end, buffer)
                {
                    let hunk_end = hunk.buffer_range.end.to_offset(buffer);
                    if hunk_end >= current_position {
                        continue;
                    }
                    let start = Anchor::in_buffer(excerpt.path_key_index, hunk.buffer_range.start)
                        .to_point(self);
                    return Some(MultiBufferRow(start.row));
                }
            }
        }

        loop {
            cursor.prev_excerpt();
            let excerpt = cursor.excerpt()?;
            let buffer = excerpt.buffer_snapshot(self);

            let Some(diff) = self.diff_state(excerpt.buffer_id) else {
                continue;
            };
            if let Some(main_buffer) = &diff.main_buffer {
                let Some(hunk) = diff
                    .hunks_intersecting_base_text_range_rev(
                        excerpt.range.context.to_offset(buffer),
                        main_buffer,
                    )
                    .next()
                else {
                    continue;
                };
                let hunk_start = buffer.anchor_after(hunk.diff_base_byte_range.start);
                let start = Anchor::in_buffer(excerpt.path_key_index, hunk_start).to_point(self);
                return Some(MultiBufferRow(start.row));
            } else {
                let Some(hunk) = diff
                    .hunks_intersecting_range_rev(excerpt.range.context.clone(), buffer)
                    .next()
                else {
                    continue;
                };
                let start = Anchor::in_buffer(excerpt.path_key_index, hunk.buffer_range.start)
                    .to_point(self);
                return Some(MultiBufferRow(start.row));
            }
        }
    }

    pub fn has_diff_hunks(&self) -> bool {
        self.diffs.iter().any(|diff| !diff.is_empty())
    }
}
