use super::*;

#[test]
fn test_splice_included_ranges() {
    let ranges = vec![ts_range(20..30), ts_range(50..60), ts_range(80..90)];

    let (new_ranges, change) = splice_included_ranges(
        ranges.clone(),
        &[54..56, 58..68],
        &[ts_range(50..54), ts_range(59..67)],
    );
    assert_eq!(
        new_ranges,
        &[
            ts_range(20..30),
            ts_range(50..54),
            ts_range(59..67),
            ts_range(80..90),
        ]
    );
    assert_eq!(change, 1..3);

    let (new_ranges, change) = splice_included_ranges(ranges.clone(), &[70..71, 91..100], &[]);
    assert_eq!(
        new_ranges,
        &[ts_range(20..30), ts_range(50..60), ts_range(80..90)]
    );
    assert_eq!(change, 2..3);

    let (new_ranges, change) =
        splice_included_ranges(ranges.clone(), &[], &[ts_range(0..2), ts_range(70..75)]);
    assert_eq!(
        new_ranges,
        &[
            ts_range(0..2),
            ts_range(20..30),
            ts_range(50..60),
            ts_range(70..75),
            ts_range(80..90)
        ]
    );
    assert_eq!(change, 0..4);

    let (new_ranges, change) =
        splice_included_ranges(ranges.clone(), &[30..50], &[ts_range(25..55)]);
    assert_eq!(new_ranges, &[ts_range(25..55), ts_range(80..90)]);
    assert_eq!(change, 0..1);

    // does not create overlapping ranges
    let (new_ranges, change) = splice_included_ranges(ranges, &[0..18], &[ts_range(20..32)]);
    assert_eq!(
        new_ranges,
        &[ts_range(20..32), ts_range(50..60), ts_range(80..90)]
    );
    assert_eq!(change, 0..1);

    fn ts_range(range: Range<usize>) -> tree_sitter::Range {
        tree_sitter::Range {
            start_byte: range.start,
            start_point: tree_sitter::Point {
                row: 0,
                column: range.start,
            },
            end_byte: range.end,
            end_point: tree_sitter::Point {
                row: 0,
                column: range.end,
            },
        }
    }
}
