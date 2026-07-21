use super::*;

impl MultiBuffer {
    pub fn add_diff(&mut self, diff: Entity<BufferDiff>, cx: &mut Context<Self>) {
        let buffer_id = diff.read(cx).buffer_id;

        if let Some(existing_diff) = self.diff_for(buffer_id)
            && diff.entity_id() == existing_diff.entity_id()
        {
            return;
        }

        self.buffer_diff_changed(
            diff.clone(),
            Some(text::Anchor::min_max_range_for_buffer(buffer_id)),
            cx,
        );
        self.diffs.insert(buffer_id, DiffState::new(diff, cx));
    }

    pub fn add_inverted_diff(
        &mut self,
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = diff.read(cx).base_text(cx);
        let base_text_buffer_id = snapshot.remote_id();
        let diff_change_range = 0..snapshot.len();
        self.snapshot.get_mut().has_inverted_diff = true;
        self.inverted_buffer_diff_changed(
            diff.clone(),
            main_buffer.clone(),
            Some(diff_change_range),
            cx,
        );
        self.diffs.insert(
            base_text_buffer_id,
            DiffState::new_inverted(diff, main_buffer, cx),
        );
    }

    pub fn diff_for(&self, buffer_id: BufferId) -> Option<Entity<BufferDiff>> {
        self.diffs.get(&buffer_id).map(|state| state.diff.clone())
    }

    pub fn expand_diff_hunks(&mut self, ranges: Vec<Range<Anchor>>, cx: &mut Context<Self>) {
        self.expand_or_collapse_diff_hunks(ranges, true, cx);
    }

    pub fn collapse_diff_hunks(&mut self, ranges: Vec<Range<Anchor>>, cx: &mut Context<Self>) {
        self.expand_or_collapse_diff_hunks(ranges, false, cx);
    }

    pub fn set_all_diff_hunks_expanded(&mut self, cx: &mut Context<Self>) {
        self.snapshot.get_mut().all_diff_hunks_expanded = true;
        self.expand_or_collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], true, cx);
    }

    pub fn all_diff_hunks_expanded(&self) -> bool {
        self.snapshot.borrow().all_diff_hunks_expanded
    }

    pub fn set_all_diff_hunks_collapsed(&mut self, cx: &mut Context<Self>) {
        self.snapshot.get_mut().all_diff_hunks_expanded = false;
        self.expand_or_collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], false, cx);
    }

    pub fn set_show_deleted_hunks(&mut self, show: bool, cx: &mut Context<Self>) {
        self.snapshot.get_mut().show_deleted_hunks = show;

        self.sync_mut(cx);

        let old_len = self.snapshot.borrow().len();

        let ranges = std::iter::once((Point::zero()..Point::MAX, None));
        let _ = self.expand_or_collapse_diff_hunks_inner(ranges, true, cx);

        let new_len = self.snapshot.borrow().len();

        self.subscriptions.publish(vec![Edit {
            old: MultiBufferOffset(0)..old_len,
            new: MultiBufferOffset(0)..new_len,
        }]);

        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    pub fn set_use_extended_diff_range(&mut self, use_extended: bool, _cx: &mut Context<Self>) {
        self.snapshot.get_mut().use_extended_diff_range = use_extended;
    }

    pub fn has_multiple_hunks(&self, cx: &App) -> bool {
        self.read(cx)
            .diff_hunks_in_range(Anchor::Min..Anchor::Max)
            .nth(1)
            .is_some()
    }

    pub fn single_hunk_is_expanded(&self, range: Range<Anchor>, cx: &App) -> bool {
        let snapshot = self.read(cx);
        let mut cursor = snapshot.diff_transforms.cursor::<MultiBufferOffset>(());
        let offset_range = range.to_offset(&snapshot);
        cursor.seek(&offset_range.start, Bias::Left);
        while let Some(item) = cursor.item() {
            if *cursor.start() >= offset_range.end && *cursor.start() > offset_range.start {
                break;
            }
            if item.hunk_info().is_some() {
                return true;
            }
            cursor.next();
        }
        false
    }

    pub fn has_expanded_diff_hunks_in_ranges(&self, ranges: &[Range<Anchor>], cx: &App) -> bool {
        let snapshot = self.read(cx);
        let mut cursor = snapshot.diff_transforms.cursor::<MultiBufferOffset>(());
        for range in ranges {
            let range = range.to_point(&snapshot);
            let start = snapshot.point_to_offset(Point::new(range.start.row, 0));
            let end = (snapshot.point_to_offset(Point::new(range.end.row + 1, 0)) + 1usize)
                .min(snapshot.len());
            cursor.seek(&start, Bias::Right);
            while let Some(item) = cursor.item() {
                if *cursor.start() >= end {
                    break;
                }
                if item.hunk_info().is_some() {
                    return true;
                }
                cursor.next();
            }
        }
        false
    }

    pub(super) fn expand_or_collapse_diff_hunks_inner(
        &mut self,
        ranges: impl IntoIterator<Item = (Range<Point>, Option<Anchor>)>,
        expand: bool,
        cx: &mut Context<Self>,
    ) -> Vec<Edit<MultiBufferOffset>> {
        if self.snapshot.borrow().all_diff_hunks_expanded && !expand {
            return Vec::new();
        }
        self.sync_mut(cx);
        let mut snapshot = self.snapshot.get_mut();
        let mut excerpt_edits = Vec::new();
        let mut last_hunk_row = None;
        for (range, end_anchor) in ranges {
            for diff_hunk in snapshot.diff_hunks_in_range(range) {
                if let Some(end_anchor) = &end_anchor
                    && let Some(hunk_end_anchor) =
                        snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.end)
                    && hunk_end_anchor.cmp(end_anchor, snapshot).is_gt()
                {
                    continue;
                }
                let hunk_range = diff_hunk.multi_buffer_range;
                if let Some(excerpt_start_anchor) =
                    snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.start)
                    && hunk_range.start.to_point(snapshot) < excerpt_start_anchor.to_point(snapshot)
                {
                    continue;
                }
                if last_hunk_row.is_some_and(|row| row >= diff_hunk.row_range.start) {
                    continue;
                }
                let mut start = snapshot.excerpt_offset_for_anchor(&hunk_range.start);
                let mut end = snapshot.excerpt_offset_for_anchor(&hunk_range.end);
                if let Some(excerpt_end_anchor) =
                    snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.end)
                {
                    let excerpt_end = snapshot.excerpt_offset_for_anchor(&excerpt_end_anchor);
                    start = start.min(excerpt_end);
                    end = end.min(excerpt_end);
                };
                last_hunk_row = Some(diff_hunk.row_range.start);
                excerpt_edits.push(text::Edit {
                    old: start..end,
                    new: start..end,
                });
            }
        }

        Self::sync_diff_transforms(
            &mut snapshot,
            excerpt_edits,
            DiffChangeKind::ExpandOrCollapseHunks { expand },
        )
    }

    pub fn expand_or_collapse_diff_hunks(
        &mut self,
        ranges: Vec<Range<Anchor>>,
        expand: bool,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot.borrow().clone();
        let ranges =
            ranges.iter().map(move |range| {
                let excerpt_end = snapshot.excerpt_containing(range.end..range.end).and_then(
                    |(_, excerpt_range)| snapshot.anchor_in_excerpt(excerpt_range.context.end),
                );
                let range = range.to_point(&snapshot);
                let mut peek_end = range.end;
                if range.end.row < snapshot.max_row().0 {
                    peek_end = Point::new(range.end.row + 1, 0);
                };
                (range.start..peek_end, excerpt_end)
            });
        let edits = self.expand_or_collapse_diff_hunks_inner(ranges, expand, cx);
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }
}

impl MultiBuffer {
    pub fn toggle_single_diff_hunk(&mut self, range: Range<Anchor>, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);
        let excerpt_end = snapshot
            .excerpt_containing(range.end..range.end)
            .and_then(|(_, excerpt_range)| snapshot.anchor_in_excerpt(excerpt_range.context.end));
        let point_range = range.to_point(&snapshot);
        let expand = !self.single_hunk_is_expanded(range, cx);
        let edits =
            self.expand_or_collapse_diff_hunks_inner([(point_range, excerpt_end)], expand, cx);
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }
}
