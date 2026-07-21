use super::*;

#[test]
fn test_format_cursor_region() {
    struct Case {
        name: &'static str,
        context: &'static str,
        editable_range: Range<usize>,
        cursor_offset: usize,
        expected: &'static str,
    }

    let cases = [
        Case {
            name: "basic_cursor_placement",
            context: "hello world\n",
            editable_range: 0..12,
            cursor_offset: 5,
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:5c|hello<|user_cursor|> world
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "multiline_cursor_on_second_line",
            context: "aaa\nbbb\nccc\n",
            editable_range: 0..12,
            cursor_offset: 5, // byte 5 → 1 byte into "bbb"
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:23|aaa
            1:26|b<|user_cursor|>bb
            2:29|ccc
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "no_trailing_newline_in_context",
            context: "line1\nline2",
            editable_range: 0..11,
            cursor_offset: 3,
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:d9|lin<|user_cursor|>e1
            1:da|line2
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "leading_newline_in_editable_region",
            context: "\nabc\n",
            editable_range: 0..5,
            cursor_offset: 2, // byte 2 = 'a' in "abc" (after leading \n)
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:00|
            1:26|a<|user_cursor|>bc
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "with_suffix",
            context: "abc\ndef",
            editable_range: 0..4, // editable region = "abc\n", suffix = "def"
            cursor_offset: 2,
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:26|ab<|user_cursor|>c
            <|fim_suffix|>
            def
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "unicode_two_byte_chars",
            context: "héllo\n",
            editable_range: 0..7,
            cursor_offset: 3, // byte 3 = after "hé" (h=1 byte, é=2 bytes), before "llo"
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:1b|hé<|user_cursor|>llo
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "unicode_three_byte_chars",
            context: "日本語\n",
            editable_range: 0..10,
            cursor_offset: 6, // byte 6 = after "日本" (3+3 bytes), before "語"
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:80|日本<|user_cursor|>語
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "unicode_four_byte_chars",
            context: "a🌍b\n",
            editable_range: 0..7,
            cursor_offset: 5, // byte 5 = after "a🌍" (1+4 bytes), before "b"
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:6b|a🌍<|user_cursor|>b
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "cursor_at_start_of_region_not_placed",
            context: "abc\n",
            editable_range: 0..4,
            cursor_offset: 0, // cursor_offset(0) > offset(0) is false → cursor not placed
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:26|abc
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "cursor_at_end_of_line_not_placed",
            context: "abc\ndef\n",
            editable_range: 0..8,
            cursor_offset: 3, // byte 3 = the \n after "abc" → falls between lines, not placed
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            0:26|abc
            1:2f|def
            <|fim_suffix|>
            <|fim_middle|>updated
            "},
        },
        Case {
            name: "cursor_offset_relative_to_context_not_editable_region",
            // cursor_offset is relative to `context`, so when editable_range.start > 0,
            // write_cursor_excerpt_section must subtract it before comparing against
            // per-line offsets within the editable region.
            context: "pre\naaa\nbbb\nsuf\n",
            editable_range: 4..12, // editable region = "aaa\nbbb\n"
            cursor_offset: 9,      // byte 9 in context = second 'b' in "bbb"
            expected: indoc! {"
            <|file_sep|>test.rs
            <|fim_prefix|>
            pre
            <|fim_middle|>current
            0:23|aaa
            1:26|b<|user_cursor|>bb
            <|fim_suffix|>
            suf
            <|fim_middle|>updated
            "},
        },
    ];

    for case in &cases {
        let mut prompt = String::new();
        hashline::write_cursor_excerpt_section(
            &mut prompt,
            Path::new("test.rs"),
            case.context,
            &case.editable_range,
            case.cursor_offset,
        );
        assert_eq!(prompt, case.expected, "failed case: {}", case.name);
    }
}
