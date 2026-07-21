use super::*;

pub(super) fn check_multibuffer(
    multibuffer: &MultiBuffer,
    reference: &ReferenceMultibuffer,
    anchors: &[Anchor],
    cx: &App,
    rng: &mut StdRng,
) {
    let snapshot = multibuffer.snapshot(cx);
    let actual_text = snapshot.text();
    let actual_boundary_rows = snapshot
        .excerpt_boundaries_in_range(MultiBufferOffset(0)..)
        .map(|b| b.row)
        .collect::<HashSet<_>>();
    let actual_row_infos = snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>();

    let anchors_to_check = anchors
        .iter()
        .filter_map(|anchor| {
            snapshot
                .anchor_to_buffer_anchor(*anchor)
                .map(|(anchor, _)| anchor)
        })
        // Intentionally mix in some anchors that are (in general) not contained in any excerpt
        .chain(
            reference
                .excerpts
                .iter()
                .map(|excerpt| excerpt.buffer.read(cx).remote_id())
                .dedup()
                .flat_map(|buffer_id| {
                    [
                        text::Anchor::min_for_buffer(buffer_id),
                        text::Anchor::max_for_buffer(buffer_id),
                    ]
                }),
        )
        .map(|anchor| snapshot.anchor_in_buffer(anchor).unwrap())
        .collect::<Vec<_>>();

    let (expected_text, expected_row_infos, expected_boundary_rows, _) =
        reference.expected_content(cx);
    let expected_anchor_offsets = anchors_to_check
        .iter()
        .map(|anchor| reference.anchor_to_offset(anchor, cx).unwrap())
        .collect::<Vec<_>>();

    let has_diff = actual_row_infos
        .iter()
        .any(|info| info.diff_status.is_some())
        || expected_row_infos
            .iter()
            .any(|info| info.diff_status.is_some());
    let actual_diff = format_diff(
        &actual_text,
        &actual_row_infos,
        &actual_boundary_rows,
        Some(has_diff),
    );
    let expected_diff = format_diff(
        &expected_text,
        &expected_row_infos,
        &expected_boundary_rows,
        Some(has_diff),
    );

    log::info!("Multibuffer content:\n{}", actual_diff);

    assert_eq!(
        actual_row_infos.len(),
        actual_text.split('\n').count(),
        "line count: {}",
        actual_text.split('\n').count()
    );
    pretty_assertions::assert_eq!(actual_diff, expected_diff);
    pretty_assertions::assert_eq!(actual_text, expected_text);
    pretty_assertions::assert_eq!(actual_row_infos, expected_row_infos);

    for _ in 0..5 {
        let start_row = rng.random_range(0..=expected_row_infos.len());
        assert_eq!(
            snapshot
                .row_infos(MultiBufferRow(start_row as u32))
                .collect::<Vec<_>>(),
            &expected_row_infos[start_row..],
            "buffer_rows({})",
            start_row
        );
    }

    assert_eq!(
        snapshot.widest_line_number(),
        expected_row_infos
            .into_iter()
            .filter_map(|info| {
                // For inverted diffs, deleted rows are visible and should be counted.
                // Only filter out deleted rows that are NOT from inverted diffs.
                let is_inverted_diff = info
                    .buffer_id
                    .is_some_and(|id| reference.inverted_diffs.contains_key(&id));
                if info.diff_status.is_some_and(|status| status.is_deleted()) && !is_inverted_diff {
                    None
                } else {
                    info.buffer_row
                }
            })
            .max()
            .unwrap()
            + 1
    );
    for i in 0..snapshot.len().0 {
        let (_, excerpt_range) = snapshot
            .excerpt_containing(MultiBufferOffset(i)..MultiBufferOffset(i))
            .unwrap();
        reference
            .excerpts
            .iter()
            .find(|reference_excerpt| reference_excerpt.range == excerpt_range.context)
            .expect("corresponding excerpt should exist in reference multibuffer");
    }

    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);

    for (row, line) in expected_text.split('\n').enumerate() {
        assert_eq!(
            snapshot.line_len(MultiBufferRow(row as u32)),
            line.len() as u32,
            "line_len({}).",
            row
        );
    }

    let text_rope = Rope::from(expected_text.as_str());
    for _ in 0..10 {
        let end_ix = text_rope.clip_offset(rng.random_range(0..=text_rope.len()), Bias::Right);
        let start_ix = text_rope.clip_offset(rng.random_range(0..=end_ix), Bias::Left);

        let text_for_range = snapshot
            .text_for_range(MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix))
            .collect::<String>();
        assert_eq!(
            text_for_range,
            &expected_text[start_ix..end_ix],
            "incorrect text for range {:?}",
            start_ix..end_ix
        );

        let expected_summary =
            MBTextSummary::from(TextSummary::from(&expected_text[start_ix..end_ix]));
        assert_eq!(
            snapshot.text_summary_for_range::<MBTextSummary, _>(
                MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix)
            ),
            expected_summary,
            "incorrect summary for range {:?}",
            start_ix..end_ix
        );
    }

    // Anchor resolution
    let summaries = snapshot.summaries_for_anchors::<MultiBufferOffset, _>(anchors);
    assert_eq!(anchors.len(), summaries.len());
    for (anchor, resolved_offset) in anchors.iter().zip(summaries) {
        assert!(resolved_offset <= snapshot.len());
        assert_eq!(
            snapshot.summary_for_anchor::<MultiBufferOffset>(anchor),
            resolved_offset,
            "anchor: {:?}",
            anchor
        );
    }

    let actual_anchor_offsets = anchors_to_check
        .into_iter()
        .map(|anchor| anchor.to_offset(&snapshot))
        .collect::<Vec<_>>();
    assert_eq!(
        actual_anchor_offsets, expected_anchor_offsets,
        "buffer anchor resolves to wrong offset"
    );

    for _ in 0..10 {
        let end_ix = text_rope.clip_offset(rng.random_range(0..=text_rope.len()), Bias::Right);
        assert_eq!(
            snapshot
                .reversed_chars_at(MultiBufferOffset(end_ix))
                .collect::<String>(),
            expected_text[..end_ix].chars().rev().collect::<String>(),
        );
    }

    for _ in 0..10 {
        let end_ix = rng.random_range(0..=text_rope.len());
        let end_ix = text_rope.floor_char_boundary(end_ix);
        let start_ix = rng.random_range(0..=end_ix);
        let start_ix = text_rope.floor_char_boundary(start_ix);
        assert_eq!(
            snapshot
                .bytes_in_range(MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix))
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            expected_text.as_bytes()[start_ix..end_ix].to_vec(),
            "bytes_in_range({:?})",
            start_ix..end_ix,
        );
    }
}

pub(super) fn check_multibuffer_edits(
    snapshot: &MultiBufferSnapshot,
    old_snapshot: &MultiBufferSnapshot,
    subscription: Subscription<MultiBufferOffset>,
) {
    let edits = subscription.consume().into_inner();

    log::info!(
        "applying subscription edits to old text: {:?}: {:#?}",
        old_snapshot.text(),
        edits,
    );

    let mut text = old_snapshot.text();
    for edit in edits {
        let new_text: String = snapshot
            .text_for_range(edit.new.start..edit.new.end)
            .collect();
        text.replace_range(
            (edit.new.start.0..edit.new.start.0 + (edit.old.end.0 - edit.old.start.0)).clone(),
            &new_text,
        );
        pretty_assertions::assert_eq!(
            &text[0..edit.new.end.0],
            snapshot
                .text_for_range(MultiBufferOffset(0)..edit.new.end)
                .collect::<String>()
        );
    }
    pretty_assertions::assert_eq!(text, snapshot.text());
}
