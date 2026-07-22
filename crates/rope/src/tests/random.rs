use super::*;
use Bias::{Left, Right};
use rand::prelude::*;
use std::{cmp::Ordering, env, io::Read};
use util::RandomCharIter;

#[gpui::test(iterations = 100)]
fn test_random_rope(mut rng: StdRng) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let mut expected = String::new();
    let mut actual = Rope::new();
    for _ in 0..operations {
        let end_ix = clip_offset(&expected, rng.random_range(0..=expected.len()), Right);
        let start_ix = clip_offset(&expected, rng.random_range(0..=end_ix), Left);
        let len = rng.random_range(0..=64);
        let new_text: String = RandomCharIter::new(&mut rng).take(len).collect();

        let mut new_actual = Rope::new();
        let mut cursor = actual.cursor(0);
        new_actual.append(cursor.slice(start_ix));
        new_actual.push(&new_text);
        cursor.seek_forward(end_ix);
        new_actual.append(cursor.suffix());
        actual = new_actual;

        expected.replace_range(start_ix..end_ix, &new_text);

        assert_eq!(actual.text(), expected);
        log::info!("text: {:?}", expected);

        for _ in 0..5 {
            let end_ix = clip_offset(&expected, rng.random_range(0..=expected.len()), Right);
            let start_ix = clip_offset(&expected, rng.random_range(0..=end_ix), Left);

            let actual_text = actual.chunks_in_range(start_ix..end_ix).collect::<String>();
            assert_eq!(actual_text, &expected[start_ix..end_ix]);

            let mut actual_text = String::new();
            actual
                .bytes_in_range(start_ix..end_ix)
                .read_to_string(&mut actual_text)
                .unwrap();
            assert_eq!(actual_text, &expected[start_ix..end_ix]);

            assert_eq!(
                actual
                    .reversed_chunks_in_range(start_ix..end_ix)
                    .collect::<Vec<&str>>()
                    .into_iter()
                    .rev()
                    .collect::<String>(),
                &expected[start_ix..end_ix]
            );

            let mut expected_line_starts: Vec<_> = expected[start_ix..end_ix]
                .match_indices('\n')
                .map(|(index, _)| start_ix + index + 1)
                .collect();

            let mut chunks = actual.chunks_in_range(start_ix..end_ix);

            let mut actual_line_starts = Vec::new();
            while chunks.next_line() {
                actual_line_starts.push(chunks.offset());
            }
            assert_eq!(
                actual_line_starts,
                expected_line_starts,
                "actual line starts != expected line starts when using next_line() for {:?} ({:?})",
                &expected[start_ix..end_ix],
                start_ix..end_ix
            );

            if start_ix < end_ix && (start_ix == 0 || expected.as_bytes()[start_ix - 1] == b'\n') {
                expected_line_starts.insert(0, start_ix);
            }
            // Remove the last index if it starts at the end of the range.
            if expected_line_starts.last() == Some(&end_ix) {
                expected_line_starts.pop();
            }

            let mut actual_line_starts = Vec::new();
            while chunks.prev_line() {
                actual_line_starts.push(chunks.offset());
            }
            actual_line_starts.reverse();
            assert_eq!(
                actual_line_starts,
                expected_line_starts,
                "actual line starts != expected line starts when using prev_line() for {:?} ({:?})",
                &expected[start_ix..end_ix],
                start_ix..end_ix
            );

            // Check that next_line/prev_line work correctly from random positions
            let mut offset = rng.random_range(start_ix..=end_ix);
            while !expected.is_char_boundary(offset) {
                offset -= 1;
            }
            chunks.seek(offset);

            for _ in 0..5 {
                if rng.random() {
                    let expected_next_line_start = expected[offset..end_ix]
                        .find('\n')
                        .map(|newline_ix| offset + newline_ix + 1);

                    let moved = chunks.next_line();
                    assert_eq!(
                        moved,
                        expected_next_line_start.is_some(),
                        "unexpected result from next_line after seeking to {} in range {:?} ({:?})",
                        offset,
                        start_ix..end_ix,
                        &expected[start_ix..end_ix]
                    );
                    if let Some(expected_next_line_start) = expected_next_line_start {
                        assert_eq!(
                            chunks.offset(),
                            expected_next_line_start,
                            "invalid position after seeking to {} in range {:?} ({:?})",
                            offset,
                            start_ix..end_ix,
                            &expected[start_ix..end_ix]
                        );
                    } else {
                        assert_eq!(
                            chunks.offset(),
                            end_ix,
                            "invalid position after seeking to {} in range {:?} ({:?})",
                            offset,
                            start_ix..end_ix,
                            &expected[start_ix..end_ix]
                        );
                    }
                } else {
                    let search_end = if offset > 0 && expected.as_bytes()[offset - 1] == b'\n' {
                        offset - 1
                    } else {
                        offset
                    };

                    let expected_prev_line_start = expected[..search_end]
                        .rfind('\n')
                        .and_then(|newline_ix| {
                            let line_start_ix = newline_ix + 1;
                            if line_start_ix >= start_ix {
                                Some(line_start_ix)
                            } else {
                                None
                            }
                        })
                        .or({
                            if offset > 0 && start_ix == 0 {
                                Some(0)
                            } else {
                                None
                            }
                        });

                    let moved = chunks.prev_line();
                    assert_eq!(
                        moved,
                        expected_prev_line_start.is_some(),
                        "unexpected result from prev_line after seeking to {} in range {:?} ({:?})",
                        offset,
                        start_ix..end_ix,
                        &expected[start_ix..end_ix]
                    );
                    if let Some(expected_prev_line_start) = expected_prev_line_start {
                        assert_eq!(
                            chunks.offset(),
                            expected_prev_line_start,
                            "invalid position after seeking to {} in range {:?} ({:?})",
                            offset,
                            start_ix..end_ix,
                            &expected[start_ix..end_ix]
                        );
                    } else {
                        assert_eq!(
                            chunks.offset(),
                            start_ix,
                            "invalid position after seeking to {} in range {:?} ({:?})",
                            offset,
                            start_ix..end_ix,
                            &expected[start_ix..end_ix]
                        );
                    }
                }

                assert!((start_ix..=end_ix).contains(&chunks.offset()));
                if rng.random() {
                    offset = rng.random_range(start_ix..=end_ix);
                    while !expected.is_char_boundary(offset) {
                        offset -= 1;
                    }
                    chunks.seek(offset);
                } else {
                    chunks.next();
                    offset = chunks.offset();
                    assert!((start_ix..=end_ix).contains(&chunks.offset()));
                }
            }
        }

        let mut offset_utf16 = OffsetUtf16(0);
        let mut point = Point::new(0, 0);
        let mut point_utf16 = PointUtf16::new(0, 0);
        for (ix, ch) in expected.char_indices().chain(Some((expected.len(), '\0'))) {
            assert_eq!(actual.offset_to_point(ix), point, "offset_to_point({})", ix);
            assert_eq!(
                actual.offset_to_point_utf16(ix),
                point_utf16,
                "offset_to_point_utf16({})",
                ix
            );
            assert_eq!(
                actual.point_to_offset(point),
                ix,
                "point_to_offset({:?})",
                point
            );
            assert_eq!(
                actual.point_utf16_to_offset(point_utf16),
                ix,
                "point_utf16_to_offset({:?})",
                point_utf16
            );
            assert_eq!(
                actual.offset_to_offset_utf16(ix),
                offset_utf16,
                "offset_to_offset_utf16({:?})",
                ix
            );
            assert_eq!(
                actual.offset_utf16_to_offset(offset_utf16),
                ix,
                "offset_utf16_to_offset({:?})",
                offset_utf16
            );
            if ch == '\n' {
                point += Point::new(1, 0);
                point_utf16 += PointUtf16::new(1, 0);
            } else {
                point.column += ch.len_utf8() as u32;
                point_utf16.column += ch.len_utf16() as u32;
            }
            offset_utf16.0 += ch.len_utf16();
        }

        let mut offset_utf16 = OffsetUtf16(0);
        let mut point_utf16 = Unclipped(PointUtf16::zero());
        for unit in expected.encode_utf16() {
            let left_offset = actual.clip_offset_utf16(offset_utf16, Bias::Left);
            let right_offset = actual.clip_offset_utf16(offset_utf16, Bias::Right);
            assert!(right_offset >= left_offset);
            // Ensure translating UTF-16 offsets to UTF-8 offsets doesn't panic.
            actual.offset_utf16_to_offset(left_offset);
            actual.offset_utf16_to_offset(right_offset);

            let left_point = actual.clip_point_utf16(point_utf16, Bias::Left);
            let right_point = actual.clip_point_utf16(point_utf16, Bias::Right);
            assert!(right_point >= left_point);
            // Ensure translating valid UTF-16 points to offsets doesn't panic.
            actual.point_utf16_to_offset(left_point);
            actual.point_utf16_to_offset(right_point);

            offset_utf16.0 += 1;
            if unit == b'\n' as u16 {
                point_utf16.0 += PointUtf16::new(1, 0);
            } else {
                point_utf16.0 += PointUtf16::new(0, 1);
            }
        }

        for _ in 0..5 {
            let end_ix = clip_offset(&expected, rng.random_range(0..=expected.len()), Right);
            let start_ix = clip_offset(&expected, rng.random_range(0..=end_ix), Left);
            assert_eq!(
                actual.cursor(start_ix).summary::<TextSummary>(end_ix),
                TextSummary::from(&expected[start_ix..end_ix])
            );
        }

        let mut expected_longest_rows = Vec::new();
        let mut longest_line_len = -1_isize;
        for (row, line) in expected.split('\n').enumerate() {
            let row = row as u32;
            assert_eq!(
                actual.line_len(row),
                line.len() as u32,
                "invalid line len for row {}",
                row
            );

            let line_char_count = line.chars().count() as isize;
            match line_char_count.cmp(&longest_line_len) {
                Ordering::Less => {}
                Ordering::Equal => expected_longest_rows.push(row),
                Ordering::Greater => {
                    longest_line_len = line_char_count;
                    expected_longest_rows.clear();
                    expected_longest_rows.push(row);
                }
            }
        }

        let longest_row = actual.summary().longest_row;
        assert!(
            expected_longest_rows.contains(&longest_row),
            "incorrect longest row {}. expected {:?} with length {}",
            longest_row,
            expected_longest_rows,
            longest_line_len,
        );
    }
}
