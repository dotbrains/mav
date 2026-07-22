use super::*;

impl BufferDiffSnapshot {
    fn stage_or_unstage_hunks_impl(
        &mut self,
        unstaged_diff: &Self,
        stage: bool,
        hunks: &[DiffHunk],
        buffer: &text::BufferSnapshot,
        file_exists: bool,
    ) -> Option<Rope> {
        let head_text = self
            .base_text_exists
            .then(|| self.base_text.as_rope().clone());
        let index_text = unstaged_diff
            .base_text_exists
            .then(|| unstaged_diff.base_text.as_rope().clone());

        // If the file doesn't exist in either HEAD or the index, then the
        // entire file must be either created or deleted in the index.
        let (index_text, head_text) = match (index_text, head_text) {
            (Some(index_text), Some(head_text)) if file_exists || !stage => (index_text, head_text),
            (index_text, head_text) => {
                let (new_index_text, new_status) = if stage {
                    log::debug!("stage all");
                    (
                        file_exists.then(|| buffer.as_rope().clone()),
                        DiffHunkSecondaryStatus::SecondaryHunkRemovalPending,
                    )
                } else {
                    log::debug!("unstage all");
                    (
                        head_text,
                        DiffHunkSecondaryStatus::SecondaryHunkAdditionPending,
                    )
                };

                let hunk = PendingHunk {
                    buffer_range: Anchor::min_max_range_for_buffer(buffer.remote_id()),
                    diff_base_byte_range: 0..index_text.map_or(0, |rope| rope.len()),
                    buffer_version: buffer.version().clone(),
                    new_status,
                };
                self.pending_hunks = SumTree::from_item(hunk, buffer);
                return new_index_text;
            }
        };

        let mut pending_hunks = SumTree::new(buffer);
        let mut old_pending_hunks = self.pending_hunks.cursor::<DiffHunkSummary>(buffer);

        // first, merge new hunks into pending_hunks
        for DiffHunk {
            buffer_range,
            diff_base_byte_range,
            secondary_status,
            ..
        } in hunks.iter().cloned()
        {
            let preceding_pending_hunks = old_pending_hunks.slice(&buffer_range.start, Bias::Left);
            pending_hunks.append(preceding_pending_hunks, buffer);

            // Skip all overlapping or adjacent old pending hunks
            while old_pending_hunks.item().is_some_and(|old_hunk| {
                old_hunk
                    .buffer_range
                    .start
                    .cmp(&buffer_range.end, buffer)
                    .is_le()
            }) {
                old_pending_hunks.next();
            }

            if (stage && secondary_status == DiffHunkSecondaryStatus::NoSecondaryHunk)
                || (!stage && secondary_status == DiffHunkSecondaryStatus::HasSecondaryHunk)
            {
                continue;
            }

            pending_hunks.push(
                PendingHunk {
                    buffer_range,
                    diff_base_byte_range,
                    buffer_version: buffer.version().clone(),
                    new_status: if stage {
                        DiffHunkSecondaryStatus::SecondaryHunkRemovalPending
                    } else {
                        DiffHunkSecondaryStatus::SecondaryHunkAdditionPending
                    },
                },
                buffer,
            );
        }
        // append the remainder
        pending_hunks.append(old_pending_hunks.suffix(), buffer);

        let mut unstaged_hunk_cursor = unstaged_diff.hunks.cursor::<DiffHunkSummary>(buffer);
        unstaged_hunk_cursor.next();

        // then, iterate over all pending hunks (both new ones and the existing ones) and compute the edits
        let mut prev_unstaged_hunk_buffer_end = 0;
        let mut prev_unstaged_hunk_base_text_end = 0;
        let mut edits = Vec::<(Range<usize>, String)>::new();
        let mut pending_hunks_iter = pending_hunks.iter().cloned().peekable();
        while let Some(PendingHunk {
            buffer_range,
            diff_base_byte_range,
            new_status,
            ..
        }) = pending_hunks_iter.next()
        {
            // Advance unstaged_hunk_cursor to skip unstaged hunks before current hunk
            let skipped_unstaged = unstaged_hunk_cursor.slice(&buffer_range.start, Bias::Left);

            if let Some(unstaged_hunk) = skipped_unstaged.last() {
                prev_unstaged_hunk_base_text_end = unstaged_hunk.diff_base_byte_range.end;
                prev_unstaged_hunk_buffer_end = unstaged_hunk.buffer_range.end.to_offset(buffer);
            }

            // Find where this hunk is in the index if it doesn't overlap
            let mut buffer_offset_range = buffer_range.to_offset(buffer);
            let start_overshoot = buffer_offset_range.start - prev_unstaged_hunk_buffer_end;
            let mut index_start = prev_unstaged_hunk_base_text_end + start_overshoot;

            loop {
                // Merge this hunk with any overlapping unstaged hunks.
                if let Some(unstaged_hunk) = unstaged_hunk_cursor.item() {
                    let unstaged_hunk_offset_range = unstaged_hunk.buffer_range.to_offset(buffer);
                    if unstaged_hunk_offset_range.start <= buffer_offset_range.end {
                        prev_unstaged_hunk_base_text_end = unstaged_hunk.diff_base_byte_range.end;
                        prev_unstaged_hunk_buffer_end = unstaged_hunk_offset_range.end;

                        index_start = index_start.min(unstaged_hunk.diff_base_byte_range.start);
                        buffer_offset_range.start = buffer_offset_range
                            .start
                            .min(unstaged_hunk_offset_range.start);
                        buffer_offset_range.end =
                            buffer_offset_range.end.max(unstaged_hunk_offset_range.end);

                        unstaged_hunk_cursor.next();
                        continue;
                    }
                }

                // If any unstaged hunks were merged, then subsequent pending hunks may
                // now overlap this hunk. Merge them.
                if let Some(next_pending_hunk) = pending_hunks_iter.peek() {
                    let next_pending_hunk_offset_range =
                        next_pending_hunk.buffer_range.to_offset(buffer);
                    if next_pending_hunk_offset_range.start <= buffer_offset_range.end {
                        buffer_offset_range.end = buffer_offset_range
                            .end
                            .max(next_pending_hunk_offset_range.end);
                        pending_hunks_iter.next();
                        continue;
                    }
                }

                break;
            }

            let end_overshoot = buffer_offset_range
                .end
                .saturating_sub(prev_unstaged_hunk_buffer_end);
            let index_end = prev_unstaged_hunk_base_text_end + end_overshoot;

            // Clamp to the index text bounds. The overshoot mapping assumes that
            // text between unstaged hunks is identical in the buffer and index.
            // When the buffer has been edited since the diff was computed, anchor
            // positions shift while diff_base_byte_range values don't, which can
            // cause index_end to exceed index_text.len().
            // See `test_stage_all_with_stale_buffer` which would hit an assert
            // without these min calls
            let index_end = index_end.min(index_text.len());
            let index_start = index_start.min(index_end);
            let index_byte_range = index_start..index_end;

            let replacement_text = match new_status {
                DiffHunkSecondaryStatus::SecondaryHunkRemovalPending => {
                    log::debug!("staging hunk {:?}", buffer_offset_range);
                    buffer
                        .text_for_range(buffer_offset_range)
                        .collect::<String>()
                }
                DiffHunkSecondaryStatus::SecondaryHunkAdditionPending => {
                    log::debug!("unstaging hunk {:?}", buffer_offset_range);
                    head_text
                        .chunks_in_range(diff_base_byte_range.clone())
                        .collect::<String>()
                }
                _ => {
                    debug_assert!(false);
                    continue;
                }
            };

            edits.push((index_byte_range, replacement_text));
        }
        drop(pending_hunks_iter);
        drop(old_pending_hunks);
        self.pending_hunks = pending_hunks;

        #[cfg(debug_assertions)] // invariants: non-overlapping and sorted
        {
            for window in edits.windows(2) {
                let (range_a, range_b) = (&window[0].0, &window[1].0);
                debug_assert!(range_a.end < range_b.start);
            }
        }

        let mut new_index_text = Rope::new();
        let mut index_cursor = index_text.cursor(0);

        for (old_range, replacement_text) in edits {
            new_index_text.append(index_cursor.slice(old_range.start));
            index_cursor.seek_forward(old_range.end);
            new_index_text.push(&replacement_text);
        }
        new_index_text.append(index_cursor.suffix());
        Some(new_index_text)
    }
}

impl BufferDiffSnapshot {
    fn hunks_intersecting_range_impl<'a>(
        &'a self,
        filter: impl 'a + Fn(&DiffHunkSummary) -> bool,
        buffer: &'a text::BufferSnapshot,
        secondary: Option<&'a Self>,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let anchor_iter = self
            .hunks
            .filter::<_, DiffHunkSummary>(buffer, filter)
            .flat_map(move |hunk| {
                [
                    (
                        hunk.buffer_range.start,
                        (
                            hunk.buffer_range.start,
                            hunk.diff_base_byte_range.start,
                            hunk,
                        ),
                    ),
                    (
                        hunk.buffer_range.end,
                        (hunk.buffer_range.end, hunk.diff_base_byte_range.end, hunk),
                    ),
                ]
            });

        let mut pending_hunks_cursor = self.pending_hunks.cursor::<DiffHunkSummary>(buffer);
        pending_hunks_cursor.next();

        let mut secondary_cursor = None;
        if let Some(secondary) = secondary.as_ref() {
            let mut cursor = secondary.hunks.cursor::<DiffHunkSummary>(buffer);
            cursor.next();
            secondary_cursor = Some(cursor);
        }

        let max_point = buffer.max_point();
        let mut summaries = buffer.summaries_for_anchors_with_payload::<Point, _, _>(anchor_iter);
        iter::from_fn(move || {
            loop {
                let (start_point, (start_anchor, start_base, hunk)) = summaries.next()?;
                let (mut end_point, (mut end_anchor, end_base, _)) = summaries.next()?;

                let base_word_diffs = hunk.base_word_diffs.clone();
                let buffer_word_diffs = hunk.buffer_word_diffs.clone();

                if !start_anchor.is_valid(buffer) {
                    continue;
                }

                if end_point.column > 0 && end_point < max_point {
                    end_point.row += 1;
                    end_point.column = 0;
                    end_anchor = buffer.anchor_before(end_point);
                }

                let mut secondary_status = DiffHunkSecondaryStatus::NoSecondaryHunk;

                let mut has_pending = false;
                if start_anchor
                    .cmp(&pending_hunks_cursor.start().buffer_range.start, buffer)
                    .is_gt()
                {
                    pending_hunks_cursor.seek_forward(&start_anchor, Bias::Left);
                }

                if let Some(pending_hunk) = pending_hunks_cursor.item() {
                    let mut pending_range = pending_hunk.buffer_range.to_point(buffer);
                    if pending_range.end.column > 0 {
                        pending_range.end.row += 1;
                        pending_range.end.column = 0;
                    }

                    if pending_range == (start_point..end_point)
                        && !buffer.has_edits_since_in_range(
                            &pending_hunk.buffer_version,
                            start_anchor..end_anchor,
                        )
                    {
                        has_pending = true;
                        secondary_status = pending_hunk.new_status;
                    }
                }

                if let (Some(secondary_cursor), false) = (secondary_cursor.as_mut(), has_pending) {
                    if start_anchor
                        .cmp(&secondary_cursor.start().buffer_range.start, buffer)
                        .is_gt()
                    {
                        secondary_cursor.seek_forward(&start_anchor, Bias::Left);
                    }

                    if let Some(secondary_hunk) = secondary_cursor.item() {
                        let mut secondary_range = secondary_hunk.buffer_range.to_point(buffer);
                        if secondary_range.end.column > 0 {
                            secondary_range.end.row += 1;
                            secondary_range.end.column = 0;
                        }
                        if secondary_range.is_empty()
                            && secondary_hunk.diff_base_byte_range.is_empty()
                        {
                            // ignore
                        } else if secondary_range == (start_point..end_point) {
                            secondary_status = DiffHunkSecondaryStatus::HasSecondaryHunk;
                        } else if secondary_range.start <= end_point {
                            secondary_status = DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk;
                        }
                    }
                }

                return Some(DiffHunk {
                    range: start_point..end_point,
                    diff_base_byte_range: start_base..end_base,
                    buffer_range: start_anchor..end_anchor,
                    base_word_diffs,
                    buffer_word_diffs,
                    secondary_status,
                });
            }
        })
    }

    fn hunks_intersecting_range_rev_impl<'a>(
        &'a self,
        filter: impl 'a + Fn(&DiffHunkSummary) -> bool,
        buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let mut cursor = self.hunks.filter::<_, DiffHunkSummary>(buffer, filter);

        iter::from_fn(move || {
            cursor.prev();

            let hunk = cursor.item()?;
            let range = hunk.buffer_range.to_point(buffer);

            Some(DiffHunk {
                range,
                diff_base_byte_range: hunk.diff_base_byte_range.clone(),
                buffer_range: hunk.buffer_range.clone(),
                // The secondary status is not used by callers of this method.
                secondary_status: DiffHunkSecondaryStatus::NoSecondaryHunk,
                base_word_diffs: hunk.base_word_diffs.clone(),
                buffer_word_diffs: hunk.buffer_word_diffs.clone(),
            })
        })
    }
}
