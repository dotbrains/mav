use super::*;
use std::{fmt::Write as _, sync::mpsc};

use gpui::TestAppContext;
use pretty_assertions::{assert_eq, assert_ne};
use rand::{Rng as _, rngs::StdRng};
use text::{Buffer, BufferId, ReplicaId, Rope};
use unindent::Unindent as _;

#[gpui::test(iterations = 100)]
async fn test_patch_for_range_random(cx: &mut TestAppContext, mut rng: StdRng) {
    fn gen_line(rng: &mut StdRng) -> String {
        if rng.random_bool(0.2) {
            "\n".to_owned()
        } else {
            let c = rng.random_range('A'..='Z');
            format!("{c}{c}{c}\n")
        }
    }

    fn gen_text(rng: &mut StdRng, line_count: usize) -> String {
        (0..line_count).map(|_| gen_line(rng)).collect()
    }

    fn gen_edits_from(rng: &mut StdRng, base: &str) -> String {
        let mut old_lines: Vec<&str> = base.lines().collect();
        let mut result = String::new();

        while !old_lines.is_empty() {
            let unchanged_count = rng.random_range(0..=old_lines.len());
            for _ in 0..unchanged_count {
                if old_lines.is_empty() {
                    break;
                }
                result.push_str(old_lines.remove(0));
                result.push('\n');
            }

            if old_lines.is_empty() {
                break;
            }

            let deleted_count = rng.random_range(0..=old_lines.len().min(3));
            for _ in 0..deleted_count {
                if old_lines.is_empty() {
                    break;
                }
                old_lines.remove(0);
            }

            let minimum_added = if deleted_count == 0 { 1 } else { 0 };
            let added_count = rng.random_range(minimum_added..=3);
            for _ in 0..added_count {
                result.push_str(&gen_line(rng));
            }
        }

        result
    }

    fn random_point_in_text(rng: &mut StdRng, lines: &[&str]) -> Point {
        if lines.is_empty() {
            return Point::zero();
        }
        let row = rng.random_range(0..lines.len() as u32);
        let line = lines[row as usize];
        let col = if line.is_empty() {
            0
        } else {
            rng.random_range(0..=line.len() as u32)
        };
        Point::new(row, col)
    }

    fn random_range_in_text(rng: &mut StdRng, lines: &[&str]) -> RangeInclusive<Point> {
        let start = random_point_in_text(rng, lines);
        let end = random_point_in_text(rng, lines);
        if start <= end {
            start..=end
        } else {
            end..=start
        }
    }

    fn points_in_range(range: &RangeInclusive<Point>, lines: &[&str]) -> Vec<Point> {
        let mut points = Vec::new();
        for row in range.start().row..=range.end().row {
            if row as usize >= lines.len() {
                points.push(Point::new(row, 0));
                continue;
            }
            let line = lines[row as usize];
            let start_col = if row == range.start().row {
                range.start().column
            } else {
                0
            };
            let end_col = if row == range.end().row {
                range.end().column
            } else {
                line.len() as u32
            };
            for col in start_col..=end_col {
                points.push(Point::new(row, col));
            }
        }
        points
    }

    let rng = &mut rng;

    let line_count = rng.random_range(5..20);
    let base_text = gen_text(rng, line_count);
    let initial_buffer_text = gen_edits_from(rng, &base_text);

    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        initial_buffer_text.clone(),
    );

    let diff = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);

    let edit_count = rng.random_range(1..=5);
    for _ in 0..edit_count {
        let buffer_text = buffer.text();
        if buffer_text.is_empty() {
            buffer.edit([(0..0, gen_line(rng))]);
        } else {
            let lines: Vec<&str> = buffer_text.lines().collect();
            let start_row = rng.random_range(0..lines.len());
            let end_row = rng.random_range(start_row..=lines.len().min(start_row + 3));

            let start_col = if start_row < lines.len() {
                rng.random_range(0..=lines[start_row].len())
            } else {
                0
            };
            let end_col = if end_row < lines.len() {
                rng.random_range(0..=lines[end_row].len())
            } else {
                0
            };

            let start_offset = buffer
                .point_to_offset(Point::new(start_row as u32, start_col as u32))
                .min(buffer.len());
            let end_offset = buffer
                .point_to_offset(Point::new(end_row as u32, end_col as u32))
                .min(buffer.len());

            let (start, end) = if start_offset <= end_offset {
                (start_offset, end_offset)
            } else {
                (end_offset, start_offset)
            };

            let new_text = if rng.random_bool(0.3) {
                String::new()
            } else {
                let line_count = rng.random_range(0..=2);
                gen_text(rng, line_count)
            };

            buffer.edit([(start..end, new_text)]);
        }
    }

    let buffer_snapshot = buffer.snapshot();

    let buffer_text = buffer_snapshot.text();
    let buffer_lines: Vec<&str> = buffer_text.lines().collect();
    let base_lines: Vec<&str> = base_text.lines().collect();

    let test_count = 10;
    for _ in 0..test_count {
        let range = random_range_in_text(rng, &buffer_lines);
        let points = points_in_range(&range, &buffer_lines);

        let optimized_patch = diff.patch_for_buffer_range(range.clone(), &buffer_snapshot);
        let naive_patch = diff.patch_for_buffer_range_naive(&buffer_snapshot);

        for point in points {
            let optimized_edit = optimized_patch.edit_for_old_position(point);
            let naive_edit = naive_patch.edit_for_old_position(point);

            assert_eq!(
                optimized_edit,
                naive_edit,
                "patch_for_buffer_range mismatch at point {:?} in range {:?}\nbase_text: {:?}\ninitial_buffer: {:?}\ncurrent_buffer: {:?}",
                point,
                range,
                base_text,
                initial_buffer_text,
                buffer_snapshot.text()
            );
        }
    }

    for _ in 0..test_count {
        let range = random_range_in_text(rng, &base_lines);
        let points = points_in_range(&range, &base_lines);

        let optimized_patch = diff.patch_for_base_text_range(range.clone(), &buffer_snapshot);
        let naive_patch = diff.patch_for_base_text_range_naive(&buffer_snapshot);

        for point in points {
            let optimized_edit = optimized_patch.edit_for_old_position(point);
            let naive_edit = naive_patch.edit_for_old_position(point);

            assert_eq!(
                optimized_edit,
                naive_edit,
                "patch_for_base_text_range mismatch at point {:?} in range {:?}\nbase_text: {:?}\ninitial_buffer: {:?}\ncurrent_buffer: {:?}",
                point,
                range,
                base_text,
                initial_buffer_text,
                buffer_snapshot.text()
            );
        }
    }
}

#[gpui::test]
async fn test_set_base_text_with_crlf(cx: &mut gpui::TestAppContext) {
    let base_text_crlf = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n";
    let base_text_lf = "one\ntwo\nthree\nfour\nfive\n";
    assert_ne!(base_text_crlf.len(), base_text_lf.len());

    let buffer_text = "one\nTWO\nthree\nfour\nfive\n";
    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        buffer_text.to_string(),
    );
    let buffer_snapshot = buffer.snapshot();

    let diff = cx.new(|cx| BufferDiff::new(&buffer_snapshot, None, None, cx));
    diff.update(cx, |diff, cx| {
        diff.set_base_text(Some(Arc::from(base_text_crlf)), buffer_snapshot.clone(), cx)
    })
    .await;
    cx.run_until_parked();

    let snapshot = diff.update(cx, |diff, cx| diff.snapshot(cx));
    snapshot.buffer_point_to_base_text_range(Point::new(0, 0), &buffer_snapshot);
    snapshot.buffer_point_to_base_text_range(Point::new(1, 0), &buffer_snapshot);
}
