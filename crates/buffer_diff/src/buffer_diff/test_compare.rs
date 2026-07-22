use super::*;
use std::{fmt::Write as _, sync::mpsc};

use gpui::TestAppContext;
use pretty_assertions::{assert_eq, assert_ne};
use rand::{Rng as _, rngs::StdRng};
use text::{Buffer, BufferId, ReplicaId, Rope};
use unindent::Unindent as _;

#[gpui::test]
async fn test_buffer_diff_compare(cx: &mut TestAppContext) {
    let base_text = "
            zero
            one
            two
            three
            four
            five
            six
            seven
            eight
            nine
        "
    .unindent();

    let buffer_text_1 = "
            one
            three
            four
            five
            SIX
            seven
            eight
            NINE
        "
    .unindent();

    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text_1);

    let empty_diff = cx.update(|cx| BufferDiff::new(&buffer, None, None, cx).snapshot(cx));
    let diff_1 = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_1.hunks,
        &empty_diff.hunks,
        &buffer,
        &buffer,
        &diff_1.base_text(),
        &diff_1.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(0, 0)..Point::new(8, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_1.base_text()),
        Point::new(0, 0)..Point::new(10, 0)
    );

    // Edit does affects the diff because it recalculates word diffs.
    buffer.edit_via_marked_text(
        &"
                one
                three
                four
                five
                «SIX.5»
                seven
                eight
                NINE
            "
        .unindent(),
    );
    let diff_2 = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_2.hunks,
        &diff_1.hunks,
        &buffer,
        &buffer,
        diff_2.base_text(),
        diff_2.base_text(),
    );
    assert_eq!(
        changed_range.unwrap().to_point(&buffer),
        Point::new(4, 0)..Point::new(5, 0),
    );
    assert_eq!(
        base_text_changed_range
            .unwrap()
            .to_point(diff_2.base_text()),
        Point::new(6, 0)..Point::new(7, 0),
    );

    // Edit turns a deletion hunk into a modification.
    buffer.edit_via_marked_text(
        &"
                one
                «THREE»
                four
                five
                SIX.5
                seven
                eight
                NINE
            "
        .unindent(),
    );
    let diff_3 = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_3.hunks,
        &diff_2.hunks,
        &buffer,
        &buffer,
        diff_3.base_text(),
        diff_3.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(1, 0)..Point::new(2, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_3.base_text()),
        Point::new(2, 0)..Point::new(4, 0)
    );

    // Edit turns a modification hunk into a deletion.
    buffer.edit_via_marked_text(
        &"
                one
                THREE
                four
                five«»
                seven
                eight
                NINE
            "
        .unindent(),
    );
    let diff_4 = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_4.hunks,
        &diff_3.hunks,
        &buffer,
        &buffer,
        diff_4.base_text(),
        diff_4.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(3, 4)..Point::new(4, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_4.base_text()),
        Point::new(6, 0)..Point::new(7, 0)
    );

    // Edit introduces a new insertion hunk.
    buffer.edit_via_marked_text(
        &"
                one
                THREE
                four«
                FOUR.5
                »five
                seven
                eight
                NINE
            "
        .unindent(),
    );
    let diff_5 = BufferDiffSnapshot::new_sync(buffer.snapshot(), base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_5.hunks,
        &diff_4.hunks,
        &buffer,
        &buffer,
        diff_5.base_text(),
        diff_5.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(3, 0)..Point::new(4, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_5.base_text()),
        Point::new(5, 0)..Point::new(5, 0)
    );

    // Edit removes a hunk.
    buffer.edit_via_marked_text(
        &"
                one
                THREE
                four
                FOUR.5
                five
                seven
                eight
                «nine»
            "
        .unindent(),
    );
    let diff_6 = BufferDiffSnapshot::new_sync(buffer.snapshot(), base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_6.hunks,
        &diff_5.hunks,
        &buffer,
        &buffer,
        diff_6.base_text(),
        diff_6.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(7, 0)..Point::new(8, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_6.base_text()),
        Point::new(9, 0)..Point::new(10, 0)
    );

    buffer.edit_via_marked_text(
        &"
                one
                THREE
                four«»
                five
                seven
                eight
                «NINE»
            "
        .unindent(),
    );

    let diff_7 = BufferDiffSnapshot::new_sync(buffer.snapshot(), base_text.clone(), cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_7.hunks,
        &diff_6.hunks,
        &buffer,
        &buffer,
        diff_7.base_text(),
        diff_7.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(2, 4)..Point::new(7, 0));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_7.base_text()),
        Point::new(5, 0)..Point::new(10, 0)
    );

    buffer.edit_via_marked_text(
        &"
                one
                THREE
                four
                five«»seven
                eight
                NINE
            "
        .unindent(),
    );

    let diff_8 = BufferDiffSnapshot::new_sync(buffer.snapshot(), base_text, cx);
    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &diff_8.hunks,
        &diff_7.hunks,
        &buffer,
        &buffer,
        diff_8.base_text(),
        diff_8.base_text(),
    );
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(3, 0)..Point::new(3, 4));
    let base_text_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_text_range.to_point(diff_8.base_text()),
        Point::new(5, 0)..Point::new(8, 0)
    );
}

#[gpui::test(iterations = 100)]
async fn test_staging_and_unstaging_hunks(cx: &mut TestAppContext, mut rng: StdRng) {
    fn gen_line(rng: &mut StdRng) -> String {
        if rng.random_bool(0.2) {
            "\n".to_owned()
        } else {
            let c = rng.random_range('A'..='Z');
            format!("{c}{c}{c}\n")
        }
    }

    fn gen_working_copy(rng: &mut StdRng, head: &str) -> String {
        let mut old_lines = {
            let mut old_lines = Vec::new();
            let old_lines_iter = head.lines();
            for line in old_lines_iter {
                assert!(!line.ends_with("\n"));
                old_lines.push(line.to_owned());
            }
            if old_lines.last().is_some_and(|line| line.is_empty()) {
                old_lines.pop();
            }
            old_lines.into_iter()
        };
        let mut result = String::new();
        let unchanged_count = rng.random_range(0..=old_lines.len());
        result += &old_lines
            .by_ref()
            .take(unchanged_count)
            .fold(String::new(), |mut s, line| {
                writeln!(&mut s, "{line}").unwrap();
                s
            });
        while old_lines.len() > 0 {
            let deleted_count = rng.random_range(0..=old_lines.len());
            let _advance = old_lines
                .by_ref()
                .take(deleted_count)
                .map(|line| line.len() + 1)
                .sum::<usize>();
            let minimum_added = if deleted_count == 0 { 1 } else { 0 };
            let added_count = rng.random_range(minimum_added..=5);
            let addition = (0..added_count).map(|_| gen_line(rng)).collect::<String>();
            result += &addition;

            if old_lines.len() > 0 {
                let blank_lines = old_lines.clone().take_while(|line| line.is_empty()).count();
                if blank_lines == old_lines.len() {
                    break;
                };
                let unchanged_count = rng.random_range((blank_lines + 1).max(1)..=old_lines.len());
                result +=
                    &old_lines
                        .by_ref()
                        .take(unchanged_count)
                        .fold(String::new(), |mut s, line| {
                            writeln!(&mut s, "{line}").unwrap();
                            s
                        });
            }
        }
        result
    }

    fn uncommitted_diff(
        working_copy: &language::BufferSnapshot,
        index_text: &Rope,
        head_text: String,
        cx: &mut TestAppContext,
    ) -> Entity<BufferDiff> {
        let secondary = cx.new(|cx| {
            BufferDiff::new_with_base_text(&index_text.to_string(), &working_copy.text, cx)
        });
        cx.new(|cx| {
            let mut diff = BufferDiff::new_with_base_text(&head_text, &working_copy.text, cx);
            diff.secondary_diff = Some(secondary);
            diff
        })
    }

    let operations = std::env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let rng = &mut rng;
    let head_text = ('a'..='z').fold(String::new(), |mut s, c| {
        writeln!(&mut s, "{c}{c}{c}").unwrap();
        s
    });
    let working_copy = gen_working_copy(rng, &head_text);
    let working_copy = cx.new(|cx| {
        language::Buffer::local_normalized(
            Rope::from(working_copy.as_str()),
            text::LineEnding::default(),
            cx,
        )
    });
    let working_copy = working_copy.read_with(cx, |working_copy, _| working_copy.snapshot());
    let mut index_text = if rng.random() {
        Rope::from(head_text.as_str())
    } else {
        working_copy.as_rope().clone()
    };

    let mut diff = uncommitted_diff(&working_copy, &index_text, head_text.clone(), cx);
    let mut hunks = diff.update(cx, |diff, cx| {
        diff.snapshot(cx)
            .hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(diff.buffer_id),
                &working_copy,
            )
            .collect::<Vec<_>>()
    });
    if hunks.is_empty() {
        return;
    }

    for _ in 0..operations {
        let i = rng.random_range(0..hunks.len());
        let hunk = &mut hunks[i];
        let hunk_to_change = hunk.clone();
        let stage = match hunk.secondary_status {
            DiffHunkSecondaryStatus::HasSecondaryHunk => {
                hunk.secondary_status = DiffHunkSecondaryStatus::NoSecondaryHunk;
                true
            }
            DiffHunkSecondaryStatus::NoSecondaryHunk => {
                hunk.secondary_status = DiffHunkSecondaryStatus::HasSecondaryHunk;
                false
            }
            _ => unreachable!(),
        };

        index_text = diff.update(cx, |diff, cx| {
            diff.stage_or_unstage_hunks(stage, &[hunk_to_change], &working_copy, true, cx)
                .unwrap()
        });

        diff = uncommitted_diff(&working_copy, &index_text, head_text.clone(), cx);
        let found_hunks = diff.update(cx, |diff, cx| {
            diff.snapshot(cx)
                .hunks_intersecting_range(
                    Anchor::min_max_range_for_buffer(diff.buffer_id),
                    &working_copy,
                )
                .collect::<Vec<_>>()
        });
        assert_eq!(hunks.len(), found_hunks.len());

        for (expected_hunk, found_hunk) in hunks.iter().zip(&found_hunks) {
            assert_eq!(
                expected_hunk.buffer_range.to_point(&working_copy),
                found_hunk.buffer_range.to_point(&working_copy)
            );
            assert_eq!(
                expected_hunk.diff_base_byte_range,
                found_hunk.diff_base_byte_range
            );
            assert_eq!(expected_hunk.secondary_status, found_hunk.secondary_status);
        }
        hunks = found_hunks;
    }
}
