use super::*;

#[test]
fn test_prev_next_line() {
    let rope = Rope::from("abc\ndef\nghi\njkl");

    let mut chunks = rope.chunks();
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'a');

    assert!(chunks.next_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'd');

    assert!(chunks.next_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'g');

    assert!(chunks.next_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'j');

    assert!(!chunks.next_line());
    assert_eq!(chunks.peek(), None);

    assert!(chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'j');

    assert!(chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'g');

    assert!(chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'd');

    assert!(chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'a');

    assert!(!chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'a');

    // Only return true when the cursor has moved to the start of a line
    let mut chunks = rope.chunks_in_range(5..7);
    chunks.seek(6);
    assert!(!chunks.prev_line());
    assert_eq!(chunks.peek().unwrap().chars().next().unwrap(), 'e');

    assert!(!chunks.next_line());
    assert_eq!(chunks.peek(), None);
}

#[test]
fn test_lines() {
    let rope = Rope::from("abc\ndefg\nhi");
    let mut lines = rope.chunks().lines();
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), Some("defg"));
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), None);

    let rope = Rope::from("abc\ndefg\nhi\n");
    let mut lines = rope.chunks().lines();
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), Some("defg"));
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), None);

    let rope = Rope::from("abc\ndefg\nhi");
    let mut lines = rope.reversed_chunks_in_range(0..rope.len()).lines();
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), Some("defg"));
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), None);

    let rope = Rope::from("abc\ndefg\nhi\n");
    let mut lines = rope.reversed_chunks_in_range(0..rope.len()).lines();
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), Some("defg"));
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), None);

    let rope = Rope::from("abc\nlonger line test\nhi");
    let mut lines = rope.chunks().lines();
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), Some("longer line test"));
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), None);

    let rope = Rope::from("abc\nlonger line test\nhi");
    let mut lines = rope.reversed_chunks_in_range(0..rope.len()).lines();
    assert_eq!(lines.next(), Some("hi"));
    assert_eq!(lines.next(), Some("longer line test"));
    assert_eq!(lines.next(), Some("abc"));
    assert_eq!(lines.next(), None);
}
