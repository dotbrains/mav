use super::*;
use crate::{MultiBuffer, ToPoint, display_map::inlay_map::InlayMap};
use rand::prelude::*;
use std::{env, mem};
use sum_tree::Bias::{Left, Right};
use text::Patch;
use util::RandomCharIter;

#[gpui::test(iterations = 100)]
fn test_random_folds(cx: &mut gpui::App, mut rng: StdRng) {
    init_test(cx);
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let len = rng.random_range(0..10);
    let text = RandomCharIter::new(&mut rng).take(len).collect::<String>();
    let buffer = if rng.random() {
        MultiBuffer::build_simple(&text, cx)
    } else {
        MultiBuffer::build_random(&mut rng, cx)
    };
    let mut buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;

    let (mut initial_snapshot, _) = map.read(inlay_snapshot, vec![]);
    let mut snapshot_edits = Vec::new();

    let mut next_inlay_id = 0;
    for _ in 0..operations {
        log::info!("text: {:?}", buffer_snapshot.text());
        let mut buffer_edits = Vec::new();
        let mut inlay_edits = Vec::new();
        match rng.random_range(0..=100) {
            0..=39 => {
                snapshot_edits.extend(map.randomly_mutate(&mut rng));
            }
            40..=59 => {
                let (_, edits) = inlay_map.randomly_mutate(&mut next_inlay_id, &mut rng);
                inlay_edits = edits;
            }
            _ => buffer.update(cx, |buffer, cx| {
                let subscription = buffer.subscribe();
                let edit_count = rng.random_range(1..=5);
                buffer.randomly_mutate(&mut rng, edit_count, cx);
                buffer_snapshot = buffer.snapshot(cx);
                let edits = subscription.consume().into_inner();
                log::info!("editing {:?}", edits);
                buffer_edits.extend(edits);
            }),
        };

        let (inlay_snapshot, new_inlay_edits) =
            inlay_map.sync(buffer_snapshot.clone(), buffer_edits);
        log::info!("inlay text {:?}", inlay_snapshot.text());

        let inlay_edits = Patch::new(inlay_edits)
            .compose(new_inlay_edits)
            .into_inner();
        let (snapshot, edits) = map.read(inlay_snapshot.clone(), inlay_edits);
        snapshot_edits.push((snapshot.clone(), edits));

        let mut expected_text: String = inlay_snapshot.text().to_string();
        for fold_range in map.merged_folds().into_iter().rev() {
            let fold_inlay_start = inlay_snapshot.to_inlay_offset(fold_range.start);
            let fold_inlay_end = inlay_snapshot.to_inlay_offset(fold_range.end);
            expected_text.replace_range(fold_inlay_start.0.0..fold_inlay_end.0.0, "⋯");
        }

        assert_eq!(snapshot.text(), expected_text);
        log::info!(
            "fold text {:?} ({} lines)",
            expected_text,
            expected_text.matches('\n').count() + 1
        );

        let mut prev_row = 0;
        let mut expected_buffer_rows = Vec::new();
        for fold_range in map.merged_folds() {
            let fold_start = inlay_snapshot
                .to_point(inlay_snapshot.to_inlay_offset(fold_range.start))
                .row();
            let fold_end = inlay_snapshot
                .to_point(inlay_snapshot.to_inlay_offset(fold_range.end))
                .row();
            expected_buffer_rows.extend(
                inlay_snapshot
                    .row_infos(prev_row)
                    .take((1 + fold_start - prev_row) as usize),
            );
            prev_row = 1 + fold_end;
        }
        expected_buffer_rows.extend(inlay_snapshot.row_infos(prev_row));

        assert_eq!(
            expected_buffer_rows.len(),
            expected_text.matches('\n').count() + 1,
            "wrong expected buffer rows {:?}. text: {:?}",
            expected_buffer_rows,
            expected_text
        );

        for (output_row, line) in expected_text.lines().enumerate() {
            let line_len = snapshot.line_len(output_row as u32);
            assert_eq!(line_len, line.len() as u32);
        }

        let longest_row = snapshot.longest_row();
        let longest_char_column = expected_text
            .split('\n')
            .nth(longest_row as usize)
            .unwrap()
            .chars()
            .count();
        let mut fold_point = FoldPoint::new(0, 0);
        let mut fold_offset = FoldOffset(MultiBufferOffset(0));
        let mut char_column = 0;
        for c in expected_text.chars() {
            let inlay_point = fold_point.to_inlay_point(&snapshot);
            let inlay_offset = fold_offset.to_inlay_offset(&snapshot);
            assert_eq!(
                snapshot.to_fold_point(inlay_point, Right),
                fold_point,
                "{:?} -> fold point",
                inlay_point,
            );
            assert_eq!(
                inlay_snapshot.to_offset(inlay_point),
                inlay_offset,
                "inlay_snapshot.to_offset({:?})",
                inlay_point,
            );
            assert_eq!(
                fold_point.to_offset(&snapshot),
                fold_offset,
                "fold_point.to_offset({:?})",
                fold_point,
            );

            if c == '\n' {
                *fold_point.row_mut() += 1;
                *fold_point.column_mut() = 0;
                char_column = 0;
            } else {
                *fold_point.column_mut() += c.len_utf8() as u32;
                char_column += 1;
            }
            fold_offset.0 += c.len_utf8();
            if char_column > longest_char_column {
                panic!(
                    "invalid longest row {:?} (chars {}), found row {:?} (chars: {})",
                    longest_row,
                    longest_char_column,
                    fold_point.row(),
                    char_column
                );
            }
        }

        for _ in 0..5 {
            let mut start = snapshot.clip_offset(
                FoldOffset(rng.random_range(MultiBufferOffset(0)..=snapshot.len().0)),
                Bias::Left,
            );
            let mut end = snapshot.clip_offset(
                FoldOffset(rng.random_range(MultiBufferOffset(0)..=snapshot.len().0)),
                Bias::Right,
            );
            if start > end {
                mem::swap(&mut start, &mut end);
            }

            let text = &expected_text[start.0.0..end.0.0];
            assert_eq!(
                snapshot
                    .chunks(
                        start..end,
                        LanguageAwareStyling {
                            tree_sitter: false,
                            diagnostics: false,
                        },
                        Highlights::default()
                    )
                    .map(|c| c.text)
                    .collect::<String>(),
                text,
            );
        }

        let mut fold_row = 0;
        while fold_row < expected_buffer_rows.len() as u32 {
            assert_eq!(
                snapshot.row_infos(fold_row).collect::<Vec<_>>(),
                expected_buffer_rows[(fold_row as usize)..],
                "wrong buffer rows starting at fold row {}",
                fold_row,
            );
            fold_row += 1;
        }

        let folded_buffer_rows = map
            .merged_folds()
            .iter()
            .flat_map(|fold_range| {
                let start_row = fold_range.start.to_point(&buffer_snapshot).row;
                let end = fold_range.end.to_point(&buffer_snapshot);
                if end.column == 0 {
                    start_row..end.row
                } else {
                    start_row..end.row + 1
                }
            })
            .collect::<HashSet<_>>();
        for row in 0..=buffer_snapshot.max_point().row {
            assert_eq!(
                snapshot.is_line_folded(MultiBufferRow(row)),
                folded_buffer_rows.contains(&row),
                "expected buffer row {}{} to be folded",
                row,
                if folded_buffer_rows.contains(&row) {
                    ""
                } else {
                    " not"
                }
            );
        }

        for _ in 0..5 {
            let end = buffer_snapshot.clip_offset(
                rng.random_range(MultiBufferOffset(0)..=buffer_snapshot.len()),
                Right,
            );
            let start =
                buffer_snapshot.clip_offset(rng.random_range(MultiBufferOffset(0)..=end), Left);
            let expected_folds = map
                .snapshot
                .folds
                .items(&buffer_snapshot)
                .into_iter()
                .filter(|fold| {
                    let start = buffer_snapshot.anchor_before(start);
                    let end = buffer_snapshot.anchor_after(end);
                    start.cmp(&fold.range.end, &buffer_snapshot) == Ordering::Less
                        && end.cmp(&fold.range.start, &buffer_snapshot) == Ordering::Greater
                })
                .collect::<Vec<_>>();

            assert_eq!(
                snapshot
                    .folds_in_range(start..end)
                    .cloned()
                    .collect::<Vec<_>>(),
                expected_folds
            );
        }

        let text = snapshot.text();
        for _ in 0..5 {
            let start_row = rng.random_range(0..=snapshot.max_point().row());
            let start_column = rng.random_range(0..=snapshot.line_len(start_row));
            let end_row = rng.random_range(0..=snapshot.max_point().row());
            let end_column = rng.random_range(0..=snapshot.line_len(end_row));
            let mut start =
                snapshot.clip_point(FoldPoint::new(start_row, start_column), Bias::Left);
            let mut end = snapshot.clip_point(FoldPoint::new(end_row, end_column), Bias::Right);
            if start > end {
                mem::swap(&mut start, &mut end);
            }

            let lines = start..end;
            let bytes = start.to_offset(&snapshot)..end.to_offset(&snapshot);
            assert_eq!(
                snapshot.text_summary_for_range(lines),
                MBTextSummary::from(&text[bytes.start.0.0..bytes.end.0.0])
            )
        }

        let mut text = initial_snapshot.text();
        for (snapshot, edits) in snapshot_edits.drain(..) {
            let new_text = snapshot.text();
            for edit in edits {
                let old_bytes = edit.new.start.0.0..edit.new.start.0.0 + edit.old_len();
                let new_bytes = edit.new.start.0.0..edit.new.end.0.0;
                text.replace_range(old_bytes, &new_text[new_bytes]);
            }

            assert_eq!(text, new_text);
            initial_snapshot = snapshot;
        }
    }
}

impl FoldMap {
    fn merged_folds(&self) -> Vec<Range<MultiBufferOffset>> {
        let inlay_snapshot = self.snapshot.inlay_snapshot.clone();
        let buffer = &inlay_snapshot.buffer;
        let mut folds = self.snapshot.folds.items(buffer);
        // Ensure sorting doesn't change how folds get merged and displayed.
        folds.sort_by(|a, b| a.range.cmp(&b.range, buffer));
        let mut folds = folds
            .iter()
            .map(|fold| fold.range.start.to_offset(buffer)..fold.range.end.to_offset(buffer))
            .peekable();

        let mut merged_folds = Vec::new();
        while let Some(mut fold_range) = folds.next() {
            while let Some(next_range) = folds.peek() {
                if fold_range.end >= next_range.start {
                    if next_range.end > fold_range.end {
                        fold_range.end = next_range.end;
                    }
                    folds.next();
                } else {
                    break;
                }
            }
            if fold_range.end > fold_range.start {
                merged_folds.push(fold_range);
            }
        }
        merged_folds
    }

    pub fn randomly_mutate(&mut self, rng: &mut impl Rng) -> Vec<(FoldSnapshot, Vec<FoldEdit>)> {
        let mut snapshot_edits = Vec::new();
        match rng.random_range(0..=100) {
            0..=39 if !self.snapshot.folds.is_empty() => {
                let inlay_snapshot = self.snapshot.inlay_snapshot.clone();
                let buffer = &inlay_snapshot.buffer;
                let mut to_unfold = Vec::new();
                for _ in 0..rng.random_range(1..=3) {
                    let end = buffer
                        .clip_offset(rng.random_range(MultiBufferOffset(0)..=buffer.len()), Right);
                    let start =
                        buffer.clip_offset(rng.random_range(MultiBufferOffset(0)..=end), Left);
                    to_unfold.push(start..end);
                }
                let inclusive = rng.random();
                log::info!("unfolding {:?} (inclusive: {})", to_unfold, inclusive);
                let (mut writer, snapshot, edits) = self.write(inlay_snapshot, vec![]);
                snapshot_edits.push((snapshot, edits));
                let (snapshot, edits) = writer.unfold_intersecting(to_unfold, inclusive);
                snapshot_edits.push((snapshot, edits));
            }
            _ => {
                let inlay_snapshot = self.snapshot.inlay_snapshot.clone();
                let buffer = &inlay_snapshot.buffer;
                let mut to_fold = Vec::new();
                for _ in 0..rng.random_range(1..=2) {
                    let end = buffer
                        .clip_offset(rng.random_range(MultiBufferOffset(0)..=buffer.len()), Right);
                    let start =
                        buffer.clip_offset(rng.random_range(MultiBufferOffset(0)..=end), Left);
                    to_fold.push((start..end, FoldPlaceholder::test()));
                }
                log::info!("folding {:?}", to_fold);
                let (mut writer, snapshot, edits) = self.write(inlay_snapshot, vec![]);
                snapshot_edits.push((snapshot, edits));
                let (snapshot, edits) = writer.fold(to_fold);
                snapshot_edits.push((snapshot, edits));
            }
        }
        snapshot_edits
    }
}
