use super::*;

pub(super) fn compute_total_edit_range_between_snapshots(
    old_snapshot: &TextBufferSnapshot,
    new_snapshot: &TextBufferSnapshot,
) -> Option<Range<Anchor>> {
    let edits: Vec<Edit<usize>> = new_snapshot
        .edits_since::<usize>(&old_snapshot.version)
        .collect();

    let (first_edit, last_edit) = edits.first().zip(edits.last())?;
    let new_start_point = new_snapshot.offset_to_point(first_edit.new.start);
    let new_end_point = new_snapshot.offset_to_point(last_edit.new.end);

    Some(new_snapshot.anchor_before(new_start_point)..new_snapshot.anchor_before(new_end_point))
}

fn compute_old_range_for_new_range(
    old_snapshot: &TextBufferSnapshot,
    new_snapshot: &TextBufferSnapshot,
    total_edit_range: &Range<Anchor>,
) -> Option<Range<Point>> {
    let new_start_offset = total_edit_range.start.to_offset(new_snapshot);
    let new_end_offset = total_edit_range.end.to_offset(new_snapshot);

    let edits: Vec<Edit<usize>> = new_snapshot
        .edits_since::<usize>(&old_snapshot.version)
        .collect();
    let mut old_start_offset = None;
    let mut old_end_offset = None;
    let mut delta: isize = 0;

    for edit in &edits {
        if old_start_offset.is_none() && new_start_offset <= edit.new.end {
            old_start_offset = Some(if new_start_offset < edit.new.start {
                new_start_offset.checked_add_signed(-delta)?
            } else {
                edit.old.start
            });
        }

        if old_end_offset.is_none() && new_end_offset <= edit.new.end {
            old_end_offset = Some(if new_end_offset < edit.new.start {
                new_end_offset.checked_add_signed(-delta)?
            } else {
                edit.old.end
            });
        }

        delta += edit.new.len() as isize - edit.old.len() as isize;
    }

    let old_start_offset =
        old_start_offset.unwrap_or_else(|| new_start_offset.saturating_add_signed(-delta));
    let old_end_offset =
        old_end_offset.unwrap_or_else(|| new_end_offset.saturating_add_signed(-delta));

    Some(
        old_snapshot.offset_to_point(old_start_offset)
            ..old_snapshot.offset_to_point(old_end_offset),
    )
}

pub(super) fn compute_diff_between_snapshots_in_range(
    old_snapshot: &TextBufferSnapshot,
    new_snapshot: &TextBufferSnapshot,
    total_edit_range: &Range<Anchor>,
) -> Option<(String, Range<usize>, Range<usize>)> {
    let new_start_offset = total_edit_range.start.to_offset(new_snapshot);
    let new_end_offset = total_edit_range.end.to_offset(new_snapshot);
    let new_start_point = new_snapshot.offset_to_point(new_start_offset);
    let new_end_point = new_snapshot.offset_to_point(new_end_offset);
    let old_range = compute_old_range_for_new_range(old_snapshot, new_snapshot, total_edit_range)?;
    let old_start_point = old_range.start;
    let old_end_point = old_range.end;
    let old_start_offset = old_snapshot.point_to_offset(old_start_point);
    let old_end_offset = old_snapshot.point_to_offset(old_end_point);

    const CONTEXT_LINES: u32 = 3;

    let old_context_start_row = old_start_point.row.saturating_sub(CONTEXT_LINES);
    let new_context_start_row = new_start_point.row.saturating_sub(CONTEXT_LINES);
    let old_context_end_row =
        (old_end_point.row + 1 + CONTEXT_LINES).min(old_snapshot.max_point().row);
    let new_context_end_row =
        (new_end_point.row + 1 + CONTEXT_LINES).min(new_snapshot.max_point().row);

    let old_start_line_offset = old_snapshot.point_to_offset(Point::new(old_context_start_row, 0));
    let new_start_line_offset = new_snapshot.point_to_offset(Point::new(new_context_start_row, 0));
    let old_end_line_offset = old_snapshot
        .point_to_offset(Point::new(old_context_end_row + 1, 0).min(old_snapshot.max_point()));
    let new_end_line_offset = new_snapshot
        .point_to_offset(Point::new(new_context_end_row + 1, 0).min(new_snapshot.max_point()));
    let old_edit_range = old_start_line_offset..old_end_line_offset;
    let new_edit_range = new_start_line_offset..new_end_line_offset;

    if new_edit_range.len() > EDIT_HISTORY_DIFF_SIZE_LIMIT
        || old_edit_range.len() > EDIT_HISTORY_DIFF_SIZE_LIMIT
    {
        return None;
    }

    let old_region_text: String = old_snapshot.text_for_range(old_edit_range).collect();
    let new_region_text: String = new_snapshot.text_for_range(new_edit_range).collect();

    let diff = language::unified_diff_with_offsets(
        &old_region_text,
        &new_region_text,
        old_context_start_row,
        new_context_start_row,
    );

    Some((
        diff,
        old_start_offset..old_end_offset,
        new_start_offset..new_end_offset,
    ))
}

pub(super) fn merge_anchor_ranges(
    left: &Range<Anchor>,
    right: &Range<Anchor>,
    snapshot: &TextBufferSnapshot,
) -> Range<Anchor> {
    let start = if left.start.cmp(&right.start, snapshot).is_le() {
        left.start
    } else {
        right.start
    };
    let end = if left.end.cmp(&right.end, snapshot).is_ge() {
        left.end
    } else {
        right.end
    };
    start..end
}
