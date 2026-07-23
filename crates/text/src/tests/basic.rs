use super::*;

#[test]
fn test_edit() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abc");
    assert_eq!(buffer.text(), "abc");
    buffer.edit([(3..3, "def")]);
    assert_eq!(buffer.text(), "abcdef");
    buffer.edit([(0..0, "ghi")]);
    assert_eq!(buffer.text(), "ghiabcdef");
    buffer.edit([(5..5, "jkl")]);
    assert_eq!(buffer.text(), "ghiabjklcdef");
    buffer.edit([(6..7, "")]);
    assert_eq!(buffer.text(), "ghiabjlcdef");
    buffer.edit([(4..9, "mno")]);
    assert_eq!(buffer.text(), "ghiamnoef");
}

fn test_point_for_row_and_column_from_external_source() {
    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        "aéøbcdef\nsecond",
    );
    let snapshot = buffer.snapshot();

    assert_eq!(snapshot.point_from_external_input(0, 0), Point::new(0, 0));
    assert_eq!(snapshot.point_from_external_input(0, 4), Point::new(0, 6));
    assert_eq!(
        snapshot.point_from_external_input(0, 100),
        Point::new(0, 10)
    );
    assert_eq!(snapshot.point_from_external_input(1, 3), Point::new(1, 3));
}

fn test_line_endings() {
    assert_eq!(LineEnding::detect(&"🍐✅\n".repeat(1000)), LineEnding::Unix);
    assert_eq!(LineEnding::detect(&"abcd\n".repeat(1000)), LineEnding::Unix);
    assert_eq!(
        LineEnding::detect(&"🍐✅\r\n".repeat(1000)),
        LineEnding::Windows
    );
    assert_eq!(
        LineEnding::detect(&"abcd\r\n".repeat(1000)),
        LineEnding::Windows
    );

    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        "one\r\ntwo\rthree",
    );
    assert_eq!(buffer.text(), "one\ntwo\nthree");
    assert_eq!(buffer.line_ending(), LineEnding::Windows);
    buffer.check_invariants();

    buffer.edit([(buffer.len()..buffer.len(), "\r\nfour")]);
    buffer.edit([(0..0, "zero\r\n")]);
    assert_eq!(buffer.text(), "zero\none\ntwo\nthree\nfour");
    assert_eq!(buffer.line_ending(), LineEnding::Windows);
    buffer.check_invariants();
}

#[test]
fn test_line_len() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");
    buffer.edit([(0..0, "abcd\nefg\nhij")]);
    buffer.edit([(12..12, "kl\nmno")]);
    buffer.edit([(18..18, "\npqrs\n")]);
    buffer.edit([(18..21, "\nPQ")]);

    assert_eq!(buffer.line_len(0), 4);
    assert_eq!(buffer.line_len(1), 3);
    assert_eq!(buffer.line_len(2), 5);
    assert_eq!(buffer.line_len(3), 3);
    assert_eq!(buffer.line_len(4), 4);
    assert_eq!(buffer.line_len(5), 0);
}

#[test]
fn test_common_prefix_at_position() {
    let text = "a = str; b = δα";
    let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text);

    let offset1 = offset_after(text, "str");
    let offset2 = offset_after(text, "δα");

    // the preceding word is a prefix of the suggestion
    assert_eq!(
        buffer.common_prefix_at(offset1, "string"),
        range_of(text, "str"),
    );
    // a suffix of the preceding word is a prefix of the suggestion
    assert_eq!(
        buffer.common_prefix_at(offset1, "tree"),
        range_of(text, "tr"),
    );
    // the preceding word is a substring of the suggestion, but not a prefix
    assert_eq!(
        buffer.common_prefix_at(offset1, "astro"),
        empty_range_after(text, "str"),
    );

    // prefix matching is case insensitive.
    assert_eq!(
        buffer.common_prefix_at(offset1, "Strαngε"),
        range_of(text, "str"),
    );
    assert_eq!(
        buffer.common_prefix_at(offset2, "ΔΑΜΝ"),
        range_of(text, "δα"),
    );

    fn offset_after(text: &str, part: &str) -> usize {
        text.find(part).unwrap() + part.len()
    }

    fn empty_range_after(text: &str, part: &str) -> Range<usize> {
        let offset = offset_after(text, part);
        offset..offset
    }

    fn range_of(text: &str, part: &str) -> Range<usize> {
        let start = text.find(part).unwrap();
        start..start + part.len()
    }
}

#[test]
fn test_text_summary_for_range() {
    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        "ab\nefg\nhklm\nnopqrs\ntuvwxyz",
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(0..2),
        TextSummary {
            len: 2,
            chars: 2,
            len_utf16: OffsetUtf16(2),
            lines: Point::new(0, 2),
            first_line_chars: 2,
            last_line_chars: 2,
            last_line_len_utf16: 2,
            longest_row: 0,
            longest_row_chars: 2,
        }
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(1..3),
        TextSummary {
            len: 2,
            chars: 2,
            len_utf16: OffsetUtf16(2),
            lines: Point::new(1, 0),
            first_line_chars: 1,
            last_line_chars: 0,
            last_line_len_utf16: 0,
            longest_row: 0,
            longest_row_chars: 1,
        }
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(1..12),
        TextSummary {
            len: 11,
            chars: 11,
            len_utf16: OffsetUtf16(11),
            lines: Point::new(3, 0),
            first_line_chars: 1,
            last_line_chars: 0,
            last_line_len_utf16: 0,
            longest_row: 2,
            longest_row_chars: 4,
        }
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(0..20),
        TextSummary {
            len: 20,
            chars: 20,
            len_utf16: OffsetUtf16(20),
            lines: Point::new(4, 1),
            first_line_chars: 2,
            last_line_chars: 1,
            last_line_len_utf16: 1,
            longest_row: 3,
            longest_row_chars: 6,
        }
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(0..22),
        TextSummary {
            len: 22,
            chars: 22,
            len_utf16: OffsetUtf16(22),
            lines: Point::new(4, 3),
            first_line_chars: 2,
            last_line_chars: 3,
            last_line_len_utf16: 3,
            longest_row: 3,
            longest_row_chars: 6,
        }
    );
    assert_eq!(
        buffer.text_summary_for_range::<TextSummary, _>(7..22),
        TextSummary {
            len: 15,
            chars: 15,
            len_utf16: OffsetUtf16(15),
            lines: Point::new(2, 3),
            first_line_chars: 4,
            last_line_chars: 3,
            last_line_len_utf16: 3,
            longest_row: 1,
            longest_row_chars: 6,
        }
    );
}

fn test_chars_at() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");
    buffer.edit([(0..0, "abcd\nefgh\nij")]);
    buffer.edit([(12..12, "kl\nmno")]);
    buffer.edit([(18..18, "\npqrs")]);
    buffer.edit([(18..21, "\nPQ")]);

    let chars = buffer.chars_at(Point::new(0, 0));
    assert_eq!(chars.collect::<String>(), "abcd\nefgh\nijkl\nmno\nPQrs");

    let chars = buffer.chars_at(Point::new(1, 0));
    assert_eq!(chars.collect::<String>(), "efgh\nijkl\nmno\nPQrs");

    let chars = buffer.chars_at(Point::new(2, 0));
    assert_eq!(chars.collect::<String>(), "ijkl\nmno\nPQrs");

    let chars = buffer.chars_at(Point::new(3, 0));
    assert_eq!(chars.collect::<String>(), "mno\nPQrs");

    let chars = buffer.chars_at(Point::new(4, 0));
    assert_eq!(chars.collect::<String>(), "PQrs");

    // Regression test:
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");
    buffer.edit([(0..0, "[workspace]\nmembers = [\n    \"xray_core\",\n    \"xray_server\",\n    \"xray_cli\",\n    \"xray_wasm\",\n]\n")]);
    buffer.edit([(60..60, "\n")]);

    let chars = buffer.chars_at(Point::new(6, 0));
    assert_eq!(chars.collect::<String>(), "    \"xray_wasm\",\n]\n");
}
