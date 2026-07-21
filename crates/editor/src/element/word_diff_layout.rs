use super::*;

impl EditorElement {
    pub(super) fn layout_word_diff_highlights(
        display_hunks: &[(DisplayDiffHunk, Option<Hitbox>)],
        row_infos: &[RowInfo],
        start_row: DisplayRow,
        snapshot: &EditorSnapshot,
        highlighted_ranges: &mut Vec<(Range<DisplayPoint>, Hsla)>,
        cx: &mut App,
    ) {
        let colors = cx.theme().colors();

        let visible_start =
            DisplayPoint::new(start_row, 0).to_offset(&snapshot.display_snapshot, Bias::Left);
        let visible_end = DisplayPoint::new(DisplayRow(start_row.0 + row_infos.len() as u32), 0)
            .to_offset(&snapshot.display_snapshot, Bias::Right);

        // Gather the word diffs that intersect the viewport. A hunk stores the
        // word diffs for its entire range, so without this filter a large hunk
        // that is only partially scrolled into view would cost work
        // proportional to its whole size every frame.
        let mut visible_word_diffs: Vec<&Range<MultiBufferOffset>> = display_hunks
            .iter()
            .filter_map(|(hunk, _)| match hunk {
                DisplayDiffHunk::Unfolded {
                    word_diffs, status, ..
                } if status.is_modified() => Some(word_diffs),
                _ => None,
            })
            .flatten()
            .filter(|word_diff| word_diff.start < visible_end && word_diff.end > visible_start)
            .collect();

        // The converter walks each display-map layer with a forward-only cursor,
        // so it must receive ranges in non-decreasing order. Word diffs are
        // disjoint, so sorting by offset yields a monotonic sequence.
        visible_word_diffs.sort_unstable_by_key(|word_diff| (word_diff.start, word_diff.end));

        let mut converter = snapshot.display_snapshot.display_point_converter();
        for word_diff in visible_word_diffs {
            for range in converter.map(word_diff.start..word_diff.end) {
                let start_row_offset = range.start.row().0.saturating_sub(start_row.0) as usize;

                let Some(diff_status) = row_infos
                    .get(start_row_offset)
                    .and_then(|row_info| row_info.diff_status)
                else {
                    continue;
                };

                let background_color = match diff_status.kind {
                    DiffHunkStatusKind::Added => colors.version_control_word_added,
                    DiffHunkStatusKind::Deleted => colors.version_control_word_deleted,
                    DiffHunkStatusKind::Modified => {
                        debug_panic!("modified diff status for row info");
                        continue;
                    }
                };

                highlighted_ranges.push((range, background_color));
            }
        }
    }
}
