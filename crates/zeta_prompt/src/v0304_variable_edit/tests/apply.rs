use super::*;

#[test]
fn test_apply_variable_edit() {
    struct Case {
        name: &'static str,
        original: &'static str,
        model_output: &'static str,
        expected: &'static str,
    }

    let cases = [
        Case {
            name: "simple_single_line_replacement",
            original: indoc! {"
                zero
                one
                two
                three
                four
                five
            "},
            model_output: indoc! {"
                two
                <|fim_middle|>
                THREE
                <|fim_suffix|>
                four
            "},
            expected: indoc! {"
                zero
                one
                two
                THREE
                four
                five
            "},
        },
        Case {
            name: "multi_line_replacement",
            original: indoc! {"
                a
                b
                c
                d
                e
            "},
            model_output: indoc! {"
                a
                <|fim_middle|>
                B
                C
                D
                <|fim_suffix|>
                e
            "},
            expected: indoc! {"
                a
                B
                C
                D
                e
            "},
        },
        Case {
            name: "insertion_between_existing_lines",
            original: indoc! {"
                a
                b
                c
            "},
            model_output: indoc! {"
                a
                <|fim_middle|>
                X
                <|fim_suffix|>
                b
            "},
            expected: indoc! {"
                a
                X
                b
                c
            "},
        },
        Case {
            name: "deletion",
            original: indoc! {"
                a
                b
                c
                d
            "},
            model_output: indoc! {"
                a
                <|fim_middle|>
                <|fim_suffix|>
                c
            "},
            expected: indoc! {"
                a
                c
                d
            "},
        },
        Case {
            name: "replacement_at_start_no_prefix_context",
            original: indoc! {"
                a
                b
                c
            "},
            model_output: indoc! {"
                <|fim_middle|>
                X
                <|fim_suffix|>
                b
            "},
            expected: indoc! {"
                X
                b
                c
            "},
        },
        Case {
            name: "replacement_at_end_no_suffix_context",
            original: indoc! {"
                a
                b
                c
            "},
            model_output: indoc! {"
                b
                <|fim_middle|>
                Z
                <|fim_suffix|>
            "},
            expected: indoc! {"
                a
                b
                Z
            "},
        },
        Case {
            name: "context_with_trailing_newline_is_preserved",
            original: indoc! {"
                a
                b
                c
            "},
            model_output: indoc! {"
                a
                <|fim_middle|>
                B
                <|fim_suffix|>
                c
            "},
            expected: indoc! {"
                a
                B
                c
            "},
        },
        Case {
            name: "cursor_marker_passes_through_untouched",
            original: indoc! {"
                a
                b
                c
            "},
            model_output: indoc! {"
                a
                <|fim_middle|>
                B<|user_cursor|>B
                <|fim_suffix|>
                c
            "},
            expected: indoc! {"
                a
                B<|user_cursor|>B
                c
            "},
        },
        Case {
            name: "multiple_prefix_context_lines",
            original: indoc! {"
                a
                b
                c
                d
                e
            "},
            model_output: indoc! {"
                b
                c
                <|fim_middle|>
                D
                <|fim_suffix|>
                e
            "},
            expected: indoc! {"
                a
                b
                c
                D
                e
            "},
        },
    ];

    for case in cases {
        let (edit_range, replacement) =
            apply_variable_edit(case.original, case.model_output).unwrap();
        let mut edited = case.original.to_string();
        edited.replace_range(edit_range, &replacement);
        assert_eq!(edited, case.expected, "{}", case.name);
    }
}
