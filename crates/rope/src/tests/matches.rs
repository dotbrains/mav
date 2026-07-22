use super::*;
use rand::prelude::*;
use util::RandomCharIter;

#[test]
fn test_chunks_equals_str() {
    let text = "This is a multi-chunk\n& multi-line test string!";
    let rope = Rope::from(text);
    for start in 0..text.len() {
        for end in start..text.len() {
            let range = start..end;
            let correct_substring = &text[start..end];

            // Test that correct range returns true
            assert!(
                rope.chunks_in_range(range.clone())
                    .equals_str(correct_substring)
            );
            assert!(
                rope.reversed_chunks_in_range(range.clone())
                    .equals_str(correct_substring)
            );

            // Test that all other ranges return false (unless they happen to match)
            for other_start in 0..text.len() {
                for other_end in other_start..text.len() {
                    if other_start == start && other_end == end {
                        continue;
                    }
                    let other_substring = &text[other_start..other_end];

                    // Only assert false if the substrings are actually different
                    if other_substring == correct_substring {
                        continue;
                    }
                    assert!(
                        !rope
                            .chunks_in_range(range.clone())
                            .equals_str(other_substring)
                    );
                    assert!(
                        !rope
                            .reversed_chunks_in_range(range.clone())
                            .equals_str(other_substring)
                    );
                }
            }
        }
    }

    let rope = Rope::from("");
    assert!(rope.chunks_in_range(0..0).equals_str(""));
    assert!(rope.reversed_chunks_in_range(0..0).equals_str(""));
    assert!(!rope.chunks_in_range(0..0).equals_str("foo"));
    assert!(!rope.reversed_chunks_in_range(0..0).equals_str("foo"));
}

#[test]
fn test_starts_with() {
    let text = "Hello, world! 🌍🌎🌏";
    let rope = Rope::from(text);

    assert!(rope.starts_with(""));
    assert!(rope.starts_with("H"));
    assert!(rope.starts_with("Hello"));
    assert!(rope.starts_with("Hello, world! 🌍🌎🌏"));
    assert!(!rope.starts_with("ello"));
    assert!(!rope.starts_with("Hello, world! 🌍🌎🌏!"));

    let empty_rope = Rope::from("");
    assert!(empty_rope.starts_with(""));
    assert!(!empty_rope.starts_with("a"));
}

#[test]
fn test_ends_with() {
    let text = "Hello, world! 🌍🌎🌏";
    let rope = Rope::from(text);

    assert!(rope.ends_with(""));
    assert!(rope.ends_with("🌏"));
    assert!(rope.ends_with("🌍🌎🌏"));
    assert!(rope.ends_with("Hello, world! 🌍🌎🌏"));
    assert!(!rope.ends_with("🌎"));
    assert!(!rope.ends_with("!Hello, world! 🌍🌎🌏"));

    let empty_rope = Rope::from("");
    assert!(empty_rope.ends_with(""));
    assert!(!empty_rope.ends_with("a"));
}

#[test]
fn test_starts_with_ends_with_random() {
    let mut rng = StdRng::seed_from_u64(0);
    for _ in 0..100 {
        let len = rng.random_range(0..100);
        let text: String = RandomCharIter::new(&mut rng).take(len).collect();
        let rope = Rope::from(text.as_str());

        for _ in 0..10 {
            let start = rng.random_range(0..=text.len());
            let start = text.ceil_char_boundary(start);
            let end = rng.random_range(start..=text.len());
            let end = text.ceil_char_boundary(end);
            let prefix = &text[..end];
            let suffix = &text[start..];

            assert_eq!(
                rope.starts_with(prefix),
                text.starts_with(prefix),
                "starts_with mismatch for {:?} in {:?}",
                prefix,
                text
            );
            assert_eq!(
                rope.ends_with(suffix),
                text.ends_with(suffix),
                "ends_with mismatch for {:?} in {:?}",
                suffix,
                text
            );
        }
    }
}
