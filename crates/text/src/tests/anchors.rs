use super::*;

#[test]
fn test_anchors() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");
    buffer.edit([(0..0, "abc")]);
    let left_anchor = buffer.anchor_before(2);
    let right_anchor = buffer.anchor_after(2);

    buffer.edit([(1..1, "def\n")]);
    assert_eq!(buffer.text(), "adef\nbc");
    assert_eq!(left_anchor.to_offset(&buffer), 6);
    assert_eq!(right_anchor.to_offset(&buffer), 6);
    assert_eq!(left_anchor.to_point(&buffer), Point { row: 1, column: 1 });
    assert_eq!(right_anchor.to_point(&buffer), Point { row: 1, column: 1 });

    buffer.edit([(2..3, "")]);
    assert_eq!(buffer.text(), "adf\nbc");
    assert_eq!(left_anchor.to_offset(&buffer), 5);
    assert_eq!(right_anchor.to_offset(&buffer), 5);
    assert_eq!(left_anchor.to_point(&buffer), Point { row: 1, column: 1 });
    assert_eq!(right_anchor.to_point(&buffer), Point { row: 1, column: 1 });

    buffer.edit([(5..5, "ghi\n")]);
    assert_eq!(buffer.text(), "adf\nbghi\nc");
    assert_eq!(left_anchor.to_offset(&buffer), 5);
    assert_eq!(right_anchor.to_offset(&buffer), 9);
    assert_eq!(left_anchor.to_point(&buffer), Point { row: 1, column: 1 });
    assert_eq!(right_anchor.to_point(&buffer), Point { row: 2, column: 0 });

    buffer.edit([(7..9, "")]);
    assert_eq!(buffer.text(), "adf\nbghc");
    assert_eq!(left_anchor.to_offset(&buffer), 5);
    assert_eq!(right_anchor.to_offset(&buffer), 7);
    assert_eq!(left_anchor.to_point(&buffer), Point { row: 1, column: 1 },);
    assert_eq!(right_anchor.to_point(&buffer), Point { row: 1, column: 3 });

    // Ensure anchoring to a point is equivalent to anchoring to an offset.
    assert_eq!(
        buffer.anchor_before(Point { row: 0, column: 0 }),
        buffer.anchor_before(0)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 0, column: 1 }),
        buffer.anchor_before(1)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 0, column: 2 }),
        buffer.anchor_before(2)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 0, column: 3 }),
        buffer.anchor_before(3)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 1, column: 0 }),
        buffer.anchor_before(4)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 1, column: 1 }),
        buffer.anchor_before(5)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 1, column: 2 }),
        buffer.anchor_before(6)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 1, column: 3 }),
        buffer.anchor_before(7)
    );
    assert_eq!(
        buffer.anchor_before(Point { row: 1, column: 4 }),
        buffer.anchor_before(8)
    );

    // Comparison between anchors.
    let anchor_at_offset_0 = buffer.anchor_before(0);
    let anchor_at_offset_1 = buffer.anchor_before(1);
    let anchor_at_offset_2 = buffer.anchor_before(2);

    assert_eq!(
        anchor_at_offset_0.cmp(&anchor_at_offset_0, &buffer),
        Ordering::Equal
    );
    assert_eq!(
        anchor_at_offset_1.cmp(&anchor_at_offset_1, &buffer),
        Ordering::Equal
    );
    assert_eq!(
        anchor_at_offset_2.cmp(&anchor_at_offset_2, &buffer),
        Ordering::Equal
    );

    assert_eq!(
        anchor_at_offset_0.cmp(&anchor_at_offset_1, &buffer),
        Ordering::Less
    );
    assert_eq!(
        anchor_at_offset_1.cmp(&anchor_at_offset_2, &buffer),
        Ordering::Less
    );
    assert_eq!(
        anchor_at_offset_0.cmp(&anchor_at_offset_2, &buffer),
        Ordering::Less
    );

    assert_eq!(
        anchor_at_offset_1.cmp(&anchor_at_offset_0, &buffer),
        Ordering::Greater
    );
    assert_eq!(
        anchor_at_offset_2.cmp(&anchor_at_offset_1, &buffer),
        Ordering::Greater
    );
    assert_eq!(
        anchor_at_offset_2.cmp(&anchor_at_offset_0, &buffer),
        Ordering::Greater
    );
}

#[test]
fn test_anchors_at_start_and_end() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");
    let before_start_anchor = buffer.anchor_before(0);
    let after_end_anchor = buffer.anchor_after(0);

    buffer.edit([(0..0, "abc")]);
    assert_eq!(buffer.text(), "abc");
    assert_eq!(before_start_anchor.to_offset(&buffer), 0);
    assert_eq!(after_end_anchor.to_offset(&buffer), 3);

    let after_start_anchor = buffer.anchor_after(0);
    let before_end_anchor = buffer.anchor_before(3);

    buffer.edit([(3..3, "def")]);
    buffer.edit([(0..0, "ghi")]);
    assert_eq!(buffer.text(), "ghiabcdef");
    assert_eq!(before_start_anchor.to_offset(&buffer), 0);
    assert_eq!(after_start_anchor.to_offset(&buffer), 3);
    assert_eq!(before_end_anchor.to_offset(&buffer), 6);
    assert_eq!(after_end_anchor.to_offset(&buffer), 9);
}
