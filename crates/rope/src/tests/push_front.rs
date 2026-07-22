use super::*;
use rand::prelude::*;
use util::RandomCharIter;

#[test]
fn test_push_front_empty_text_on_empty_rope() {
    let mut rope = Rope::new();
    rope.push_front("");
    assert_eq!(rope.text(), "");
    assert_eq!(rope.len(), 0);
}

#[test]
fn test_push_front_empty_text_on_nonempty_rope() {
    let mut rope = Rope::from("hello");
    rope.push_front("");
    assert_eq!(rope.text(), "hello");
}

#[test]
fn test_push_front_on_empty_rope() {
    let mut rope = Rope::new();
    rope.push_front("hello");
    assert_eq!(rope.text(), "hello");
    assert_eq!(rope.len(), 5);
    assert_eq!(rope.max_point(), Point::new(0, 5));
}

#[test]
fn test_push_front_single_space() {
    let mut rope = Rope::from("hint");
    rope.push_front(" ");
    assert_eq!(rope.text(), " hint");
    assert_eq!(rope.len(), 5);
}

#[gpui::test(iterations = 50)]
fn test_push_front_random(mut rng: StdRng) {
    let initial_len = rng.random_range(0..=64);
    let initial_text: String = RandomCharIter::new(&mut rng).take(initial_len).collect();
    let mut rope = Rope::from(initial_text.as_str());

    let mut expected = initial_text;

    for _ in 0..rng.random_range(1..=10) {
        let prefix_len = rng.random_range(0..=32);
        let prefix: String = RandomCharIter::new(&mut rng).take(prefix_len).collect();

        rope.push_front(&prefix);
        expected.insert_str(0, &prefix);

        assert_eq!(
            rope.text(),
            expected,
            "text mismatch after push_front({:?})",
            prefix
        );
        assert_eq!(rope.len(), expected.len());

        let actual_summary = rope.summary();
        let expected_summary = TextSummary::from(expected.as_str());
        assert_eq!(
            actual_summary.len, expected_summary.len,
            "len mismatch for {:?}",
            expected
        );
        assert_eq!(
            actual_summary.lines, expected_summary.lines,
            "lines mismatch for {:?}",
            expected
        );
        assert_eq!(
            actual_summary.chars, expected_summary.chars,
            "chars mismatch for {:?}",
            expected
        );
        assert_eq!(
            actual_summary.longest_row, expected_summary.longest_row,
            "longest_row mismatch for {:?}",
            expected
        );

        // Verify offset-to-point and point-to-offset round-trip at boundaries.
        for (ix, _) in expected.char_indices().chain(Some((expected.len(), '\0'))) {
            assert_eq!(
                rope.point_to_offset(rope.offset_to_point(ix)),
                ix,
                "offset round-trip failed at {} for {:?}",
                ix,
                expected
            );
        }
    }
}

#[gpui::test(iterations = 50)]
fn test_push_front_large_prefix(mut rng: StdRng) {
    let initial_len = rng.random_range(0..=32);
    let initial_text: String = RandomCharIter::new(&mut rng).take(initial_len).collect();
    let mut rope = Rope::from(initial_text.as_str());

    let prefix_len = rng.random_range(64..=256);
    let prefix: String = RandomCharIter::new(&mut rng).take(prefix_len).collect();

    rope.push_front(&prefix);
    let expected = format!("{}{}", prefix, initial_text);

    assert_eq!(rope.text(), expected);
    assert_eq!(rope.len(), expected.len());

    let actual_summary = rope.summary();
    let expected_summary = TextSummary::from(expected.as_str());
    assert_eq!(actual_summary.len, expected_summary.len);
    assert_eq!(actual_summary.lines, expected_summary.lines);
    assert_eq!(actual_summary.chars, expected_summary.chars);
}
