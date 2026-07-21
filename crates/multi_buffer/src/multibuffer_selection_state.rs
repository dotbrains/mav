use super::*;

impl MultiBuffer {
    pub fn set_active_selections(
        &self,
        selections: &[Selection<Anchor>],
        line_mode: bool,
        cursor_shape: CursorShape,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(cx);
        let mut selections_by_buffer: HashMap<BufferId, Vec<Selection<text::Anchor>>> =
            Default::default();

        for selection in selections {
            for (buffer_snapshot, buffer_range, _) in
                snapshot.range_to_buffer_ranges(selection.start..selection.end)
            {
                selections_by_buffer
                    .entry(buffer_snapshot.remote_id())
                    .or_default()
                    .push(Selection {
                        id: selection.id,
                        start: buffer_snapshot
                            .anchor_at(buffer_range.start, selection.start.bias()),
                        end: buffer_snapshot.anchor_at(buffer_range.end, selection.end.bias()),
                        reversed: selection.reversed,
                        goal: selection.goal,
                    });
            }
        }

        for (buffer_id, buffer_state) in self.buffers.iter() {
            if !selections_by_buffer.contains_key(buffer_id) {
                buffer_state
                    .buffer
                    .update(cx, |buffer, cx| buffer.remove_active_selections(cx));
            }
        }

        for (buffer_id, selections) in selections_by_buffer {
            self.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                buffer.set_active_selections(selections.into(), line_mode, cursor_shape, cx);
            });
        }
    }

    pub fn remove_active_selections(&self, cx: &mut Context<Self>) {
        for buffer in self.buffers.values() {
            buffer
                .buffer
                .update(cx, |buffer, cx| buffer.remove_active_selections(cx));
        }
    }

    #[instrument(skip_all)]
    pub(super) fn merge_excerpt_ranges<'a>(
        expanded_ranges: impl IntoIterator<Item = &'a ExcerptRange<Point>> + 'a,
    ) -> Vec<ExcerptRange<Point>> {
        let mut sorted: Vec<_> = expanded_ranges.into_iter().collect();
        sorted.sort_by_key(|range| range.context.start);
        let mut merged_ranges: Vec<ExcerptRange<Point>> = Vec::new();
        for range in sorted {
            if let Some(last_range) = merged_ranges.last_mut() {
                if last_range.context.end >= range.context.start
                    || last_range.context.end.row + 1 == range.context.start.row
                {
                    last_range.context.end = range.context.end.max(last_range.context.end);
                    continue;
                }
            }
            merged_ranges.push(range.clone());
        }
        merged_ranges
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.sync_mut(cx);
        let removed_buffer_ids = std::mem::take(&mut self.buffers).into_keys().collect();
        self.diffs.clear();
        let MultiBufferSnapshot {
            excerpts,
            diffs,
            diff_transforms: _,
            non_text_state_update_count: _,
            edit_count: _,
            is_dirty,
            has_deleted_file,
            has_conflict,
            has_inverted_diff,
            singleton: _,
            trailing_excerpt_update_count,
            all_diff_hunks_expanded: _,
            show_deleted_hunks: _,
            use_extended_diff_range: _,
            show_headers: _,
            path_keys: _,
            buffers,
        } = self.snapshot.get_mut();
        let start = ExcerptDimension(MultiBufferOffset::ZERO);
        let prev_len = ExcerptDimension(excerpts.summary().text.len);
        *excerpts = Default::default();
        *buffers = Default::default();
        *diffs = Default::default();
        *trailing_excerpt_update_count += 1;
        *is_dirty = false;
        *has_deleted_file = false;
        *has_conflict = false;
        *has_inverted_diff = false;

        let edits = Self::sync_diff_transforms(
            self.snapshot.get_mut(),
            vec![Edit {
                old: start..prev_len,
                new: start..start,
            }],
            DiffChangeKind::BufferEdited,
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
        cx.emit(Event::BuffersRemoved { removed_buffer_ids });
        cx.notify();
    }
}
