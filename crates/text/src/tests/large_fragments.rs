use super::*;

fn test_new_normalized_splits_large_base_text() {
    // ASCII text that exceeds max_insertion_len
    let text = "abcdefghij".repeat(10); // 100 bytes
    let rope = Rope::from(text.as_str());
    let buffer = Buffer::new_normalized(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        LineEnding::Unix,
        rope,
    );
    assert_eq!(buffer.text(), text);
    buffer.check_invariants();

    // Verify anchors at various positions, including across chunk boundaries
    for offset in [0, 1, 15, 16, 17, 50, 99] {
        let anchor = buffer.anchor_before(offset);
        assert_eq!(
            anchor.to_offset(&buffer),
            offset,
            "anchor_before({offset}) round-tripped incorrectly"
        );
        let anchor = buffer.anchor_after(offset);
        assert_eq!(
            anchor.to_offset(&buffer),
            offset,
            "anchor_after({offset}) round-tripped incorrectly"
        );
    }

    // Verify editing works after a split initialization
    let mut buffer = buffer;
    buffer.edit([(50..60, "XYZ")]);
    let mut expected = text;
    expected.replace_range(50..60, "XYZ");
    assert_eq!(buffer.text(), expected);
    buffer.check_invariants();
}

#[test]
fn test_new_normalized_splits_large_base_text_with_multibyte_chars() {
    // Use multi-byte chars (é is 2 bytes in UTF-8) so that a naive byte-level
    // split would land in the middle of a character.
    let unit = "ééééééééé"; // 9 chars × 2 bytes = 18 bytes
    let text = unit.repeat(6); // 108 bytes
    let rope = Rope::from(text.as_str());
    let buffer = Buffer::new_normalized(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        LineEnding::Unix,
        rope,
    );
    assert_eq!(buffer.text(), text);
    buffer.check_invariants();

    // Every anchor should resolve correctly even though chunks had to be
    // rounded down to a char boundary.
    let snapshot = buffer.snapshot();
    for offset in (0..text.len()).filter(|o| text.is_char_boundary(*o)) {
        let anchor = snapshot.anchor_before(offset);
        assert_eq!(
            anchor.to_offset(snapshot),
            offset,
            "anchor round-trip failed at byte offset {offset}"
        );
    }
}

#[test]
fn test_new_normalized_small_text_unchanged() {
    // Text that fits in a single chunk should produce exactly one fragment,
    // matching the original single-fragment behaviour.
    let text = "hello world";
    let rope = Rope::from(text);
    let buffer = Buffer::new_normalized(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        LineEnding::Unix,
        rope,
    );
    assert_eq!(buffer.text(), text);
    buffer.check_invariants();
    assert_eq!(buffer.snapshot().fragments.items(&None).len(), 1);
}

#[test]
fn test_edit_splits_large_insertion() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdefghij");

    let large_text: Arc<str> = "X".repeat(100).into();
    let edits = vec![(3..7, large_text.clone())];

    buffer.edit(edits);

    let expected = format!("abc{}hij", large_text);
    assert_eq!(buffer.text(), expected);
    buffer.check_invariants();

    // Anchors should resolve correctly throughout the buffer.
    for offset in [0, 3, 50, 103, expected.len()] {
        let anchor = buffer.anchor_before(offset);
        assert_eq!(
            anchor.to_offset(&buffer),
            offset,
            "anchor_before({offset}) round-tripped incorrectly"
        );
    }
}

#[test]
fn test_edit_splits_large_insertion_with_multibyte_chars() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdefghij");

    // 4-byte chars so that naive byte splits would land mid-character.
    let large_text: Arc<str> = "😀".repeat(30).into(); // 30 × 4 = 120 bytes
    let edits = vec![(5..5, large_text.clone())];

    buffer.edit(edits);

    let expected = format!("abcde{}fghij", large_text);
    assert_eq!(buffer.text(), expected);
    buffer.check_invariants();
}

#[test]
fn test_edit_splits_large_insertion_among_multiple_edits() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "ABCDEFGHIJ");

    let large_text: Arc<str> = "x".repeat(60).into();
    // Three edits: small, large, small. The large one must be split while
    // preserving the correct positions of the surrounding edits.
    let edits = vec![
        (1..2, Arc::from("y")),     // replace "B" with "y"
        (4..6, large_text.clone()), // replace "EF" with 60 x's
        (9..9, Arc::from("z")),     // insert "z" before "J"
    ];

    buffer.edit(edits);

    // Original: A B C D E F G H I J
    // After (1..2, "y"):       A y C D E F G H I J
    // After (4..6, large):     A y C D <60 x's> G H I J
    // After (9..9, "z"):       A y C D <60 x's> G H I z J
    let expected = format!("AyCD{}GHIzJ", large_text);
    assert_eq!(buffer.text(), expected);
    buffer.check_invariants();
}

#[test]
fn test_edit_splits_multiple_large_insertions() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "ABCDE");

    let text1: Arc<str> = "a".repeat(40).into();
    let text2: Arc<str> = "b".repeat(40).into();
    let edits = vec![
        (1..2, text1.clone()), // replace "B" with 40 a's
        (3..4, text2.clone()), // replace "D" with 40 b's
    ];

    buffer.edit(edits);

    let expected = format!("A{}C{}E", text1, text2);
    assert_eq!(buffer.text(), expected);
    buffer.check_invariants();
}

#[test]
fn test_edit_undo_after_split() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "hello world");
    buffer.set_group_interval(Duration::from_secs(0));
    let original = buffer.text();

    let large_text: Arc<str> = "Z".repeat(50).into();
    let edits = vec![(5..6, large_text)];
    buffer.edit(edits);
    assert_ne!(buffer.text(), original);
    buffer.check_invariants();

    // Undo should restore the original text even though the edit was split
    // into multiple internal operations grouped in one transaction.
    buffer.undo();
    assert_eq!(buffer.text(), original);
    buffer.check_invariants();
}
