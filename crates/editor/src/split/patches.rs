use super::*;

pub(crate) fn patches_for_lhs_range(
    rhs_snapshot: &MultiBufferSnapshot,
    lhs_snapshot: &MultiBufferSnapshot,
    lhs_bounds: Range<MultiBufferPoint>,
) -> Vec<CompanionExcerptPatch> {
    patches_for_range(
        lhs_snapshot,
        rhs_snapshot,
        lhs_bounds,
        |diff, range, buffer| diff.patch_for_base_text_range(range, buffer),
    )
}

pub(crate) fn patches_for_rhs_range(
    lhs_snapshot: &MultiBufferSnapshot,
    rhs_snapshot: &MultiBufferSnapshot,
    rhs_bounds: Range<MultiBufferPoint>,
) -> Vec<CompanionExcerptPatch> {
    patches_for_range(
        rhs_snapshot,
        lhs_snapshot,
        rhs_bounds,
        |diff, range, buffer| diff.patch_for_buffer_range(range, buffer),
    )
}

pub(super) fn buffer_range_to_base_text_range(
    rhs_range: &Range<Point>,
    diff_snapshot: &BufferDiffSnapshot,
    rhs_buffer_snapshot: &text::BufferSnapshot,
) -> Range<Point> {
    let start = diff_snapshot
        .buffer_point_to_base_text_range(Point::new(rhs_range.start.row, 0), rhs_buffer_snapshot)
        .start;
    let end = diff_snapshot
        .buffer_point_to_base_text_range(Point::new(rhs_range.end.row, 0), rhs_buffer_snapshot)
        .end;
    let end_column = diff_snapshot.base_text().line_len(end.row);
    Point::new(start.row, 0)..Point::new(end.row, end_column)
}

pub(super) fn translate_lhs_selections_to_rhs(
    selections_by_buffer: &HashMap<BufferId, (Vec<Range<BufferOffset>>, Option<u32>)>,
    splittable: &SplittableEditor,
    cx: &App,
) -> HashMap<Entity<Buffer>, (Vec<Range<BufferOffset>>, Option<u32>)> {
    let Some(lhs) = &splittable.lhs else {
        return HashMap::default();
    };
    let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);

    let mut translated: HashMap<Entity<Buffer>, (Vec<Range<BufferOffset>>, Option<u32>)> =
        HashMap::default();

    for (lhs_buffer_id, (ranges, scroll_offset)) in selections_by_buffer {
        let Some(diff) = lhs_snapshot.diff_for_buffer_id(*lhs_buffer_id) else {
            continue;
        };
        let rhs_buffer_id = diff.buffer_id();

        let Some(rhs_buffer) = splittable
            .rhs_editor
            .read(cx)
            .buffer()
            .read(cx)
            .buffer(rhs_buffer_id)
        else {
            continue;
        };

        let Some(diff) = splittable
            .rhs_editor
            .read(cx)
            .buffer()
            .read(cx)
            .diff_for(rhs_buffer_id)
        else {
            continue;
        };

        let diff_snapshot = diff.read(cx).snapshot(cx);
        let rhs_buffer_snapshot = rhs_buffer.read(cx).snapshot();
        let base_text_buffer = diff.read(cx).base_text_buffer();
        let base_text_snapshot = base_text_buffer.read(cx).snapshot();

        let translated_ranges: Vec<Range<BufferOffset>> = ranges
            .iter()
            .map(|range| {
                let start_point = base_text_snapshot.offset_to_point(range.start.0);
                let end_point = base_text_snapshot.offset_to_point(range.end.0);

                let rhs_start = diff_snapshot
                    .base_text_point_to_buffer_point(start_point, &rhs_buffer_snapshot);
                let rhs_end =
                    diff_snapshot.base_text_point_to_buffer_point(end_point, &rhs_buffer_snapshot);

                BufferOffset(rhs_buffer_snapshot.point_to_offset(rhs_start))
                    ..BufferOffset(rhs_buffer_snapshot.point_to_offset(rhs_end))
            })
            .collect();

        translated.insert(rhs_buffer, (translated_ranges, *scroll_offset));
    }

    translated
}

pub(super) fn translate_lhs_hunks_to_rhs(
    lhs_hunks: &[MultiBufferDiffHunk],
    splittable: &SplittableEditor,
    cx: &App,
) -> Vec<MultiBufferDiffHunk> {
    let Some(lhs) = &splittable.lhs else {
        return vec![];
    };
    let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);
    let rhs_snapshot = splittable.rhs_multibuffer.read(cx).snapshot(cx);
    let rhs_hunks: Vec<MultiBufferDiffHunk> = rhs_snapshot.diff_hunks().collect();

    let mut translated = Vec::new();
    for lhs_hunk in lhs_hunks {
        let Some(diff) = lhs_snapshot.diff_for_buffer_id(lhs_hunk.buffer_id) else {
            continue;
        };
        let rhs_buffer_id = diff.buffer_id();
        if let Some(rhs_hunk) = rhs_hunks.iter().find(|rhs_hunk| {
            rhs_hunk.buffer_id == rhs_buffer_id
                && rhs_hunk.diff_base_byte_range == lhs_hunk.diff_base_byte_range
        }) {
            translated.push(rhs_hunk.clone());
        }
    }
    translated
}

pub(crate) fn patches_for_range<F>(
    source_snapshot: &MultiBufferSnapshot,
    target_snapshot: &MultiBufferSnapshot,
    source_bounds: Range<MultiBufferPoint>,
    translate_fn: F,
) -> Vec<CompanionExcerptPatch>
where
    F: Fn(&BufferDiffSnapshot, RangeInclusive<Point>, &text::BufferSnapshot) -> Patch<Point>,
{
    struct PendingExcerpt<'a> {
        source_buffer_snapshot: &'a language::BufferSnapshot,
        source_excerpt_range: ExcerptRange<text::Anchor>,
        buffer_point_range: Range<Point>,
    }

    let mut result = Vec::new();
    let mut current_buffer_id: Option<BufferId> = None;
    let mut pending_excerpts: Vec<PendingExcerpt<'_>> = Vec::new();
    let mut union_context_start: Option<Point> = None;
    let mut union_context_end: Option<Point> = None;

    let flush_buffer = |pending: &mut Vec<PendingExcerpt>,
                        union_start: Point,
                        union_end: Point,
                        result: &mut Vec<CompanionExcerptPatch>| {
        let Some(first) = pending.first() else {
            return;
        };

        let source_buffer_id = first.source_buffer_snapshot.remote_id();
        let Some(diff) = source_snapshot.diff_for_buffer_id(source_buffer_id) else {
            pending.clear();
            return;
        };
        let source_is_lhs = source_buffer_id == diff.base_text().remote_id();
        let target_buffer_id = if source_is_lhs {
            diff.buffer_id()
        } else {
            diff.base_text().remote_id()
        };
        let Some(target_buffer) = target_snapshot.buffer_for_id(target_buffer_id) else {
            pending.clear();
            return;
        };
        let rhs_buffer = if source_is_lhs {
            target_buffer
        } else {
            &first.source_buffer_snapshot
        };

        let patch = translate_fn(diff, union_start..=union_end, rhs_buffer);

        let mut source_excerpts = source_snapshot
            .excerpts_for_buffer(source_buffer_id)
            .peekable();
        let mut target_excerpts = target_snapshot
            .excerpts_for_buffer(target_buffer_id)
            .peekable();

        for excerpt in pending.drain(..) {
            while let Some(source_excerpt_range) = source_excerpts.peek()
                && source_excerpt_range != &excerpt.source_excerpt_range
            {
                source_excerpts.next();
                target_excerpts.next();
            }
            if let Some(source_excerpt_range) = source_excerpts.peek()
                && let Some(target_excerpt_range) = target_excerpts.peek()
            {
                result.push(patch_for_excerpt(
                    source_snapshot,
                    target_snapshot,
                    &excerpt.source_buffer_snapshot,
                    target_buffer,
                    source_excerpt_range.clone(),
                    target_excerpt_range.clone(),
                    &patch,
                    excerpt.buffer_point_range,
                ));
            }
        }
    };

    for (buffer_snapshot, source_range, source_excerpt_range) in
        source_snapshot.range_to_buffer_ranges(source_bounds)
    {
        let buffer_id = buffer_snapshot.remote_id();

        if current_buffer_id != Some(buffer_id) {
            if let (Some(start), Some(end)) = (union_context_start.take(), union_context_end.take())
            {
                flush_buffer(&mut pending_excerpts, start, end, &mut result);
            }
            current_buffer_id = Some(buffer_id);
        }

        let buffer_point_range = source_range.to_point(&buffer_snapshot);
        let source_context_range = source_excerpt_range.context.to_point(&buffer_snapshot);

        union_context_start = Some(union_context_start.map_or(source_context_range.start, |s| {
            s.min(source_context_range.start)
        }));
        union_context_end = Some(union_context_end.map_or(source_context_range.end, |e| {
            e.max(source_context_range.end)
        }));

        pending_excerpts.push(PendingExcerpt {
            source_buffer_snapshot: buffer_snapshot,
            source_excerpt_range,
            buffer_point_range,
        });
    }

    if let (Some(start), Some(end)) = (union_context_start, union_context_end) {
        flush_buffer(&mut pending_excerpts, start, end, &mut result);
    }

    result
}

pub(crate) fn patch_for_excerpt(
    source_snapshot: &MultiBufferSnapshot,
    target_snapshot: &MultiBufferSnapshot,
    source_buffer_snapshot: &language::BufferSnapshot,
    target_buffer_snapshot: &language::BufferSnapshot,
    source_excerpt_range: ExcerptRange<text::Anchor>,
    target_excerpt_range: ExcerptRange<text::Anchor>,
    patch: &Patch<Point>,
    source_edited_range: Range<Point>,
) -> CompanionExcerptPatch {
    let source_buffer_range = source_excerpt_range
        .context
        .to_point(source_buffer_snapshot);
    let source_multibuffer_range = (source_snapshot
        .anchor_in_buffer(source_excerpt_range.context.start)
        .expect("buffer should exist in multibuffer")
        ..source_snapshot
            .anchor_in_buffer(source_excerpt_range.context.end)
            .expect("buffer should exist in multibuffer"))
        .to_point(source_snapshot);
    let target_buffer_range = target_excerpt_range
        .context
        .to_point(target_buffer_snapshot);
    let target_multibuffer_range = (target_snapshot
        .anchor_in_buffer(target_excerpt_range.context.start)
        .expect("buffer should exist in multibuffer")
        ..target_snapshot
            .anchor_in_buffer(target_excerpt_range.context.end)
            .expect("buffer should exist in multibuffer"))
        .to_point(target_snapshot);

    let edits = patch
        .edits()
        .iter()
        .skip_while(|edit| edit.old.end < source_buffer_range.start)
        .take_while(|edit| edit.old.start <= source_buffer_range.end)
        .map(|edit| {
            let clamped_source_start = edit.old.start.max(source_buffer_range.start);
            let clamped_source_end = edit.old.end.min(source_buffer_range.end);
            let source_multibuffer_start =
                source_multibuffer_range.start + (clamped_source_start - source_buffer_range.start);
            let source_multibuffer_end =
                source_multibuffer_range.start + (clamped_source_end - source_buffer_range.start);
            let clamped_target_start = edit
                .new
                .start
                .max(target_buffer_range.start)
                .min(target_buffer_range.end);
            let clamped_target_end = edit
                .new
                .end
                .max(target_buffer_range.start)
                .min(target_buffer_range.end);
            let target_multibuffer_start =
                target_multibuffer_range.start + (clamped_target_start - target_buffer_range.start);
            let target_multibuffer_end =
                target_multibuffer_range.start + (clamped_target_end - target_buffer_range.start);
            text::Edit {
                old: source_multibuffer_start..source_multibuffer_end,
                new: target_multibuffer_start..target_multibuffer_end,
            }
        });

    let edits = [text::Edit {
        old: source_multibuffer_range.start..source_multibuffer_range.start,
        new: target_multibuffer_range.start..target_multibuffer_range.start,
    }]
    .into_iter()
    .chain(edits);

    let mut merged_edits: Vec<text::Edit<Point>> = Vec::new();
    for edit in edits {
        if let Some(last) = merged_edits.last_mut() {
            if edit.new.start <= last.new.end || edit.old.start <= last.old.end {
                last.old.end = last.old.end.max(edit.old.end);
                last.new.end = last.new.end.max(edit.new.end);
                continue;
            }
        }
        merged_edits.push(edit);
    }

    let edited_range = source_multibuffer_range.start
        + (source_edited_range.start - source_buffer_range.start)
        ..source_multibuffer_range.start + (source_edited_range.end - source_buffer_range.start);

    let source_excerpt_end =
        source_multibuffer_range.start + (source_buffer_range.end - source_buffer_range.start);
    let target_excerpt_end =
        target_multibuffer_range.start + (target_buffer_range.end - target_buffer_range.start);

    CompanionExcerptPatch {
        patch: Patch::new(merged_edits),
        edited_range,
        source_excerpt_range: source_multibuffer_range.start..source_excerpt_end,
        target_excerpt_range: target_multibuffer_range.start..target_excerpt_end,
    }
}
