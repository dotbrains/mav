use super::*;

pub(super) fn format_diff(
    text: &str,
    row_infos: &Vec<RowInfo>,
    boundary_rows: &HashSet<MultiBufferRow>,
    has_diff: Option<bool>,
) -> String {
    let has_diff =
        has_diff.unwrap_or_else(|| row_infos.iter().any(|info| info.diff_status.is_some()));
    text.split('\n')
        .enumerate()
        .zip(row_infos)
        .map(|((ix, line), info)| {
            let marker = match info.diff_status.map(|status| status.kind) {
                Some(DiffHunkStatusKind::Added) => "+ ",
                Some(DiffHunkStatusKind::Deleted) => "- ",
                Some(DiffHunkStatusKind::Modified) => unreachable!(),
                None => {
                    if has_diff && !line.is_empty() {
                        "  "
                    } else {
                        ""
                    }
                }
            };
            let boundary_row = if boundary_rows.contains(&MultiBufferRow(ix as u32)) {
                if has_diff {
                    "  ----------\n"
                } else {
                    "---------\n"
                }
            } else {
                ""
            };
            let expand = info
                .expand_info
                .as_ref()
                .map(|expand_info| match expand_info.direction {
                    ExpandExcerptDirection::Up => " [↑]",
                    ExpandExcerptDirection::Down => " [↓]",
                    ExpandExcerptDirection::UpAndDown => " [↕]",
                })
                .unwrap_or_default();

            format!("{boundary_row}{marker}{line}{expand}")
            // let mbr = info
            //     .multibuffer_row
            //     .map(|row| format!("{:0>3}", row.0))
            //     .unwrap_or_else(|| "???".to_string());
            // let byte_range = format!("{byte_range_start:0>3}..{byte_range_end:0>3}");
            // format!("{boundary_row}Row: {mbr}, Bytes: {byte_range} | {marker}{line}{expand}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// fn format_transforms(snapshot: &MultiBufferSnapshot) -> String {
//     snapshot
//         .diff_transforms
//         .iter()
//         .map(|transform| {
//             let (kind, summary) = match transform {
//                 DiffTransform::DeletedHunk { summary, .. } => ("   Deleted", (*summary).into()),
//                 DiffTransform::FilteredInsertedHunk { summary, .. } => ("  Filtered", *summary),
//                 DiffTransform::InsertedHunk { summary, .. } => ("  Inserted", *summary),
//                 DiffTransform::Unmodified { summary, .. } => ("Unmodified", *summary),
//             };
//             format!("{kind}(len: {}, lines: {:?})", summary.len, summary.lines)
//         })
//         .join("\n")
// }

// fn format_excerpts(snapshot: &MultiBufferSnapshot) -> String {
//     snapshot
//         .excerpts
//         .iter()
//         .map(|excerpt| {
//             format!(
//                 "Excerpt(buffer_range = {:?}, lines = {:?}, has_trailing_newline = {:?})",
//                 excerpt.range.context.to_point(&excerpt.buffer),
//                 excerpt.text_summary.lines,
//                 excerpt.has_trailing_newline
//             )
//         })
//         .join("\n")
// }

#[track_caller]
pub(super) fn assert_excerpts_match(
    multibuffer: &Entity<MultiBuffer>,
    cx: &mut TestAppContext,
    expected: &str,
) {
    let mut output = String::new();
    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        for excerpt in multibuffer.snapshot(cx).excerpts() {
            output.push_str("-----\n");
            output.extend(
                snapshot
                    .buffer_for_id(excerpt.context.start.buffer_id)
                    .unwrap()
                    .text_for_range(excerpt.context),
            );
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }
    });
    assert_eq!(output, expected);
}

#[track_caller]
pub(super) fn assert_new_snapshot(
    multibuffer: &Entity<MultiBuffer>,
    snapshot: &mut MultiBufferSnapshot,
    subscription: &mut Subscription<MultiBufferOffset>,
    cx: &mut TestAppContext,
    expected_diff: &str,
) {
    let new_snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    let actual_text = new_snapshot.text();
    let line_infos = new_snapshot
        .row_infos(MultiBufferRow(0))
        .collect::<Vec<_>>();
    let actual_diff = format_diff(&actual_text, &line_infos, &Default::default(), None);
    pretty_assertions::assert_eq!(actual_diff, expected_diff);
    check_edits(
        snapshot,
        &new_snapshot,
        &subscription.consume().into_inner(),
    );
    *snapshot = new_snapshot;
}

#[track_caller]
fn check_edits(
    old_snapshot: &MultiBufferSnapshot,
    new_snapshot: &MultiBufferSnapshot,
    edits: &[Edit<MultiBufferOffset>],
) {
    let mut text = old_snapshot.text();
    let new_text = new_snapshot.text();
    for edit in edits.iter().rev() {
        if !text.is_char_boundary(edit.old.start.0)
            || !text.is_char_boundary(edit.old.end.0)
            || !new_text.is_char_boundary(edit.new.start.0)
            || !new_text.is_char_boundary(edit.new.end.0)
        {
            panic!(
                "invalid edits: {:?}\nold text: {:?}\nnew text: {:?}",
                edits, text, new_text
            );
        }

        text.replace_range(
            edit.old.start.0..edit.old.end.0,
            &new_text[edit.new.start.0..edit.new.end.0],
        );
    }

    pretty_assertions::assert_eq!(text, new_text, "invalid edits: {:?}", edits);
}

#[track_caller]
pub(super) fn assert_chunks_in_ranges(snapshot: &MultiBufferSnapshot) {
    let full_text = snapshot.text();
    for ix in 0..full_text.len() {
        let mut chunks = snapshot.chunks(
            MultiBufferOffset(0)..snapshot.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
        );
        chunks.seek(MultiBufferOffset(ix)..snapshot.len());
        let tail = chunks.map(|chunk| chunk.text).collect::<String>();
        assert_eq!(tail, &full_text[ix..], "seek to range: {:?}", ix..);
    }
}

#[track_caller]
pub(super) fn assert_consistent_line_numbers(snapshot: &MultiBufferSnapshot) {
    let all_line_numbers = snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>();
    for start_row in 1..all_line_numbers.len() {
        let line_numbers = snapshot
            .row_infos(MultiBufferRow(start_row as u32))
            .collect::<Vec<_>>();
        assert_eq!(
            line_numbers,
            all_line_numbers[start_row..],
            "start_row: {start_row}"
        );
    }
}

#[track_caller]
pub(super) fn assert_position_translation(snapshot: &MultiBufferSnapshot) {
    let text = Rope::from(snapshot.text());

    let mut left_anchors = Vec::new();
    let mut right_anchors = Vec::new();
    let mut offsets = Vec::new();
    let mut points = Vec::new();
    for offset in 0..=text.len() + 1 {
        let offset = MultiBufferOffset(offset);
        let clipped_left = snapshot.clip_offset(offset, Bias::Left);
        let clipped_right = snapshot.clip_offset(offset, Bias::Right);
        assert_eq!(
            clipped_left.0,
            text.clip_offset(offset.0, Bias::Left),
            "clip_offset({offset:?}, Left)"
        );
        assert_eq!(
            clipped_right.0,
            text.clip_offset(offset.0, Bias::Right),
            "clip_offset({offset:?}, Right)"
        );
        assert_eq!(
            snapshot.offset_to_point(clipped_left),
            text.offset_to_point(clipped_left.0),
            "offset_to_point({})",
            clipped_left.0
        );
        assert_eq!(
            snapshot.offset_to_point(clipped_right),
            text.offset_to_point(clipped_right.0),
            "offset_to_point({})",
            clipped_right.0
        );
        let anchor_after = snapshot.anchor_after(clipped_left);
        assert_eq!(
            anchor_after.to_offset(snapshot),
            clipped_left,
            "anchor_after({}).to_offset {anchor_after:?}",
            clipped_left.0
        );
        let anchor_before = snapshot.anchor_before(clipped_left);
        assert_eq!(
            anchor_before.to_offset(snapshot),
            clipped_left,
            "anchor_before({}).to_offset",
            clipped_left.0
        );
        left_anchors.push(anchor_before);
        right_anchors.push(anchor_after);
        offsets.push(clipped_left);
        points.push(text.offset_to_point(clipped_left.0));
    }

    for row in 0..text.max_point().row {
        for column in 0..text.line_len(row) + 1 {
            let point = Point { row, column };
            let clipped_left = snapshot.clip_point(point, Bias::Left);
            let clipped_right = snapshot.clip_point(point, Bias::Right);
            assert_eq!(
                clipped_left,
                text.clip_point(point, Bias::Left),
                "clip_point({point:?}, Left)"
            );
            assert_eq!(
                clipped_right,
                text.clip_point(point, Bias::Right),
                "clip_point({point:?}, Right)"
            );
            assert_eq!(
                snapshot.point_to_offset(clipped_left).0,
                text.point_to_offset(clipped_left),
                "point_to_offset({clipped_left:?})"
            );
            assert_eq!(
                snapshot.point_to_offset(clipped_right).0,
                text.point_to_offset(clipped_right),
                "point_to_offset({clipped_right:?})"
            );
        }
    }

    assert_eq!(
        snapshot.summaries_for_anchors::<MultiBufferOffset, _>(&left_anchors),
        offsets,
        "left_anchors <-> offsets"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<Point, _>(&left_anchors),
        points,
        "left_anchors <-> points"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<MultiBufferOffset, _>(&right_anchors),
        offsets,
        "right_anchors <-> offsets"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<Point, _>(&right_anchors),
        points,
        "right_anchors <-> points"
    );

    for (anchors, bias) in [(&left_anchors, Bias::Left), (&right_anchors, Bias::Right)] {
        for (ix, (offset, anchor)) in offsets.iter().zip(anchors).enumerate() {
            if ix > 0 && *offset == MultiBufferOffset(252) && offset > &offsets[ix - 1] {
                let prev_anchor = left_anchors[ix - 1];
                assert!(
                    anchor.cmp(&prev_anchor, snapshot).is_gt(),
                    "anchor({}, {bias:?}).cmp(&anchor({}, {bias:?}).is_gt()",
                    offsets[ix],
                    offsets[ix - 1],
                );
                assert!(
                    prev_anchor.cmp(anchor, snapshot).is_lt(),
                    "anchor({}, {bias:?}).cmp(&anchor({}, {bias:?}).is_lt()",
                    offsets[ix - 1],
                    offsets[ix],
                );
            }
        }
    }

    if let Some((buffer, offset)) = snapshot.point_to_buffer_offset(snapshot.max_point()) {
        assert!(offset.0 <= buffer.len());
    }
    if let Some((buffer, point)) = snapshot.point_to_buffer_point(snapshot.max_point()) {
        assert!(point <= buffer.max_point());
    }
}

pub(super) fn assert_line_indents(snapshot: &MultiBufferSnapshot) {
    let max_row = snapshot.max_point().row;
    let buffer_id = snapshot.excerpts().next().unwrap().context.start.buffer_id;
    let text = text::Buffer::new(ReplicaId::LOCAL, buffer_id, snapshot.text());
    let mut line_indents = text
        .line_indents_in_row_range(0..max_row + 1)
        .collect::<Vec<_>>();
    for start_row in 0..snapshot.max_point().row {
        pretty_assertions::assert_eq!(
            snapshot
                .line_indents(MultiBufferRow(start_row), |_| true)
                .map(|(row, indent, _)| (row.0, indent))
                .collect::<Vec<_>>(),
            &line_indents[(start_row as usize)..],
            "line_indents({start_row})"
        );
    }

    line_indents.reverse();
    pretty_assertions::assert_eq!(
        snapshot
            .reversed_line_indents(MultiBufferRow(max_row), |_| true)
            .map(|(row, indent, _)| (row.0, indent))
            .collect::<Vec<_>>(),
        &line_indents[..],
        "reversed_line_indents({max_row})"
    );
}
