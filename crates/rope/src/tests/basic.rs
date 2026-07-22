use super::*;
use Bias::{Left, Right};
use rand::prelude::*;
use std::{cmp::Ordering, io::Read};
use util::RandomCharIter;

#[test]
fn test_all_4_byte_chars() {
    let mut rope = Rope::new();
    let text = "🏀".repeat(256);
    rope.push(&text);
    assert_eq!(rope.text(), text);
}

#[test]
fn test_clip() {
    let rope = Rope::from("🧘");

    assert_eq!(rope.clip_offset(1, Bias::Left), 0);
    assert_eq!(rope.clip_offset(1, Bias::Right), 4);
    assert_eq!(rope.clip_offset(5, Bias::Right), 4);

    assert_eq!(
        rope.clip_point(Point::new(0, 1), Bias::Left),
        Point::new(0, 0)
    );
    assert_eq!(
        rope.clip_point(Point::new(0, 1), Bias::Right),
        Point::new(0, 4)
    );
    assert_eq!(
        rope.clip_point(Point::new(0, 5), Bias::Right),
        Point::new(0, 4)
    );

    assert_eq!(
        rope.clip_point_utf16(Unclipped(PointUtf16::new(0, 1)), Bias::Left),
        PointUtf16::new(0, 0)
    );
    assert_eq!(
        rope.clip_point_utf16(Unclipped(PointUtf16::new(0, 1)), Bias::Right),
        PointUtf16::new(0, 2)
    );
    assert_eq!(
        rope.clip_point_utf16(Unclipped(PointUtf16::new(0, 3)), Bias::Right),
        PointUtf16::new(0, 2)
    );

    assert_eq!(
        rope.clip_offset_utf16(OffsetUtf16(1), Bias::Left),
        OffsetUtf16(0)
    );
    assert_eq!(
        rope.clip_offset_utf16(OffsetUtf16(1), Bias::Right),
        OffsetUtf16(2)
    );
    assert_eq!(
        rope.clip_offset_utf16(OffsetUtf16(3), Bias::Right),
        OffsetUtf16(2)
    );
}
