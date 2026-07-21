use super::*;

#[test]
fn test_patch_to_variable_edit() {
    struct Case {
        name: &'static str,
        old: &'static str,
        patch: &'static str,
        cursor_offset: Option<usize>,
        expected_variable_edit: &'static str,
        expected_after_apply: &'static str,
    }

    let cases = [
        Case {
            name: "simple_replacement",
            old: indoc! {"
                zero
                one
                two
                three
                four
                five
            "},
            patch: indoc! {"
                @@ -3,3 +3,3 @@
                 two
                -three
                +THREE
                 four
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                one
                two
                <|fim_middle|>
                THREE
                <|fim_suffix|>
                four
                five
            "},
            expected_after_apply: indoc! {"
                zero
                one
                two
                THREE
                four
                five
            "},
        },
        Case {
            name: "insertion",
            old: indoc! {"
                a
                b
                c
                d
                e
            "},
            patch: indoc! {"
                @@ -2,0 +3,1 @@
                 b
                +X
                 c
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                a
                b
                <|fim_middle|>
                X
                <|fim_suffix|>
                c
                d
            "},
            expected_after_apply: indoc! {"
                a
                b
                X
                c
                d
                e
            "},
        },
        Case {
            name: "deletion",
            old: indoc! {"
                a
                b
                c
                d
                e
            "},
            patch: indoc! {"
                @@ -2,3 +2,2 @@
                 b
                -c
                 d
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                a
                b
                <|fim_middle|>
                <|fim_suffix|>
                d
                e
            "},
            expected_after_apply: indoc! {"
                a
                b
                d
                e
            "},
        },
        Case {
            name: "edit_near_start",
            old: indoc! {"
                first
                second
                third
                fourth
            "},
            patch: indoc! {"
                @@ -1,1 +1,1 @@
                -first
                +FIRST
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                <|fim_middle|>
                FIRST
                <|fim_suffix|>
                second
                third
            "},
            expected_after_apply: indoc! {"
                FIRST
                second
                third
                fourth
            "},
        },
        Case {
            name: "edit_near_end",
            old: indoc! {"
                first
                second
                third
                fourth
            "},
            patch: indoc! {"
                @@ -4,1 +4,1 @@
                -fourth
                +FOURTH
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                second
                third
                <|fim_middle|>
                FOURTH
                <|fim_suffix|>
            "},
            expected_after_apply: indoc! {"
                first
                second
                third
                FOURTH
            "},
        },
        Case {
            name: "cursor_at_start_of_replacement",
            old: indoc! {"
                zero
                one
                two
                three
                four
                five
            "},
            patch: indoc! {"
                @@ -3,3 +3,3 @@
                 two
                -three
                +THREE
                 four
            "},
            cursor_offset: Some(4),
            expected_variable_edit: indoc! {"
                one
                two
                <|fim_middle|>
                <|user_cursor|>THREE
                <|fim_suffix|>
                four
                five
            "},
            expected_after_apply: indoc! {"
                zero
                one
                two
                <|user_cursor|>THREE
                four
                five
            "},
        },
        Case {
            name: "cursor_in_middle_of_replacement",
            old: indoc! {"
                zero
                one
                two
                three
                four
                five
            "},
            patch: indoc! {"
                @@ -3,3 +3,3 @@
                 two
                -three
                +THREE
                 four
            "},
            cursor_offset: Some(6),
            expected_variable_edit: indoc! {"
                one
                two
                <|fim_middle|>
                TH<|user_cursor|>REE
                <|fim_suffix|>
                four
                five
            "},
            expected_after_apply: indoc! {"
                zero
                one
                two
                TH<|user_cursor|>REE
                four
                five
            "},
        },
        Case {
            name: "expands_context_when_two_lines_not_unique_before_and_after",
            old: indoc! {"
                one
                a
                b
                c
                d
                two
                a
                b
                c
                d
                three
                a
                b
                c
                d
                four
            "},
            patch: indoc! {"
                @@ -4,5 +4,5 @@
                 two
                 a
                 b
                -c
                +C
                 d
                 three
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                two
                a
                b
                <|fim_middle|>
                C
                <|fim_suffix|>
                d
                three
            "},
            expected_after_apply: indoc! {"
                one
                a
                b
                c
                d
                two
                a
                b
                C
                d
                three
                a
                b
                c
                d
                four
            "},
        },
        Case {
            name: "expands_context_when_two_lines_not_unique_before_and_after",
            old: indoc! {"
                {
                    {
                        one();
                    }
                }
                {
                    {
                        two();
                    }
                }
                {
                    {
                        three();
                    }
                }
                {
                    {
                        four();
                    }
                }
            "},
            patch: indoc! {"
                @@ -4,5 +4,5 @@
                     {
                -        two();
                +        TWO();
                     }
            "},
            cursor_offset: None,
            expected_variable_edit: indoc! {"
                        one();
                    }
                }
                {
                    {
                <|fim_middle|>
                        TWO();
                <|fim_suffix|>
                    }
                }
                {
                    {
                        three();
            "},
            expected_after_apply: indoc! {"
                {
                    {
                        one();
                    }
                }
                {
                    {
                        TWO();
                    }
                }
                {
                    {
                        three();
                    }
                }
                {
                    {
                        four();
                    }
                }
            "},
        },
    ];

    for case in cases {
        let output = patch_to_variable_edit_output(case.old, case.patch, case.cursor_offset)
            .unwrap_or_else(|error| panic!("failed converting patch for {}: {error}", case.name));
        assert_eq!(
            output, case.expected_variable_edit,
            "patch->variable_edit mismatch for {}",
            case.name
        );

        let (edit_range, replacement) =
            apply_variable_edit(case.old, &output).unwrap_or_else(|error| {
                panic!("failed applying variable_edit for {}: {error}", case.name)
            });
        let mut edited_by_variable_edit = case.old.to_string();
        edited_by_variable_edit.replace_range(edit_range, &replacement);
        assert_eq!(
            edited_by_variable_edit, case.expected_after_apply,
            "variable_edit apply mismatch for {}",
            case.name
        );

        let (expected_edit_range, expected_replacement) =
            apply_variable_edit(case.old, case.expected_variable_edit).unwrap_or_else(|error| {
                panic!(
                    "failed applying expected variable_edit for {}: {error}",
                    case.name
                )
            });
        let mut edited_by_expected_variable_edit = case.old.to_string();
        edited_by_expected_variable_edit.replace_range(expected_edit_range, &expected_replacement);
        assert_eq!(
            edited_by_expected_variable_edit, case.expected_after_apply,
            "expected variable_edit apply mismatch for {}",
            case.name
        );
    }
}
