use super::*;

#[test]
fn test_apply_edit_commands() {
    struct Case {
        name: &'static str,
        original: &'static str,
        model_output: &'static str,
        expected: &'static str,
    }

    let cases = vec![
        Case {
            name: "set_single_line",
            original: indoc! {"
            let mut total = 0;
            for product in products {
                total += ;
            }
            total
        "},
            model_output: indoc! {"
            <|set|>2:87
                total += product.price;
        "},
            expected: indoc! {"
            let mut total = 0;
            for product in products {
                total += product.price;
            }
            total
        "},
        },
        Case {
            name: "set_range",
            original: indoc! {"
            fn foo() {
                let x = 1;
                let y = 2;
                let z = 3;
            }
        "},
            model_output: indoc! {"
            <|set|>1:46-3:4a
                let sum = 6;
        "},
            expected: indoc! {"
            fn foo() {
                let sum = 6;
            }
        "},
        },
        Case {
            name: "insert_after_line",
            original: indoc! {"
            fn main() {
                let x = 1;
            }
        "},
            model_output: indoc! {"
            <|insert|>1:46
                let y = 2;
        "},
            expected: indoc! {"
            fn main() {
                let x = 1;
                let y = 2;
            }
        "},
        },
        Case {
            name: "insert_before_first",
            original: indoc! {"
            let x = 1;
            let y = 2;
        "},
            model_output: indoc! {"
            <|insert|>
            use std::io;
        "},
            expected: indoc! {"
            use std::io;
            let x = 1;
            let y = 2;
        "},
        },
        Case {
            name: "set_with_cursor_marker",
            original: indoc! {"
            fn main() {
                println!();
            }
        "},
            model_output: indoc! {"
            <|set|>1:34
                eprintln!(\"<|user_cursor|>\");
        "},
            expected: indoc! {"
            fn main() {
                eprintln!(\"<|user_cursor|>\");
            }
        "},
        },
        Case {
            name: "multiple_set_commands",
            original: indoc! {"
            aaa
            bbb
            ccc
            ddd
        "},
            model_output: indoc! {"
            <|set|>0:23
            AAA
            <|set|>2:29
            CCC
        "},
            expected: indoc! {"
            AAA
            bbb
            CCC
            ddd
        "},
        },
        Case {
            name: "set_range_multiline_replacement",
            original: indoc! {"
            fn handle_submit() {
            }

            fn handle_keystroke() {
        "},
            model_output: indoc! {"
            <|set|>0:3f-1:7d
            fn handle_submit(modal_state: &mut ModalState) {
                <|user_cursor|>
            }
        "},
            expected: indoc! {"
            fn handle_submit(modal_state: &mut ModalState) {
                <|user_cursor|>
            }

            fn handle_keystroke() {
        "},
        },
        Case {
            name: "no_edit_commands_returns_original",
            original: indoc! {"
            hello
            world
        "},
            model_output: "some random text with no commands",
            expected: indoc! {"
            hello
            world
        "},
        },
        Case {
            name: "no_edits_command_returns_original",
            original: indoc! {"
            hello
            world
        "},
            model_output: "<|no_edits|>",
            expected: indoc! {"
            hello
            world
        "},
        },
        Case {
            name: "wrong_hash_set_ignored",
            original: indoc! {"
            aaa
            bbb
        "},
            model_output: indoc! {"
            <|set|>0:ff
            ZZZ
        "},
            expected: indoc! {"
            aaa
            bbb
        "},
        },
        Case {
            name: "insert_and_set_combined",
            original: indoc! {"
            alpha
            beta
            gamma
        "},
            model_output: indoc! {"
            <|set|>0:06
            ALPHA
            <|insert|>1:9c
            beta_extra
        "},
            expected: indoc! {"
            ALPHA
            beta
            beta_extra
            gamma
        "},
        },
        Case {
            name: "no_trailing_newline_preserved",
            original: "hello\nworld",
            model_output: indoc! {"
            <|set|>0:14
            HELLO
        "},
            expected: "HELLO\nworld",
        },
        Case {
            name: "set_range_hash_mismatch_in_end_bound",
            original: indoc! {"
            one
            two
            three
        "},
            model_output: indoc! {"
            <|set|>0:42-2:ff
            ONE_TWO_THREE
        "},
            expected: indoc! {"
            one
            two
            three
        "},
        },
        Case {
            name: "set_range_start_greater_than_end_ignored",
            original: indoc! {"
            a
            b
            c
        "},
            model_output: indoc! {"
            <|set|>2:63-1:62
            X
        "},
            expected: indoc! {"
            a
            b
            c
        "},
        },
        Case {
            name: "insert_out_of_bounds_ignored",
            original: indoc! {"
            x
            y
        "},
            model_output: indoc! {"
            <|insert|>99:aa
            z
        "},
            expected: indoc! {"
            x
            y
        "},
        },
        Case {
            name: "set_out_of_bounds_ignored",
            original: indoc! {"
            x
            y
        "},
            model_output: indoc! {"
            <|set|>99:aa
            z
        "},
            expected: indoc! {"
            x
            y
        "},
        },
        Case {
            name: "malformed_set_command_ignored",
            original: indoc! {"
            alpha
            beta
        "},
            model_output: indoc! {"
            <|set|>not-a-line-ref
            UPDATED
        "},
            expected: indoc! {"
            alpha
            beta
        "},
        },
        Case {
            name: "malformed_insert_hash_treated_as_before_first",
            original: indoc! {"
            alpha
            beta
        "},
            model_output: indoc! {"
            <|insert|>1:nothex
            preamble
        "},
            expected: indoc! {"
            preamble
            alpha
            beta
        "},
        },
        Case {
            name: "set_then_insert_same_target_orders_insert_after_replacement",
            original: indoc! {"
            cat
            dog
        "},
            model_output: indoc! {"
            <|set|>0:38
            CAT
            <|insert|>0:38
            TAIL
        "},
            expected: indoc! {"
            CAT
            TAIL
            dog
        "},
        },
        Case {
            name: "overlapping_set_ranges_last_wins",
            original: indoc! {"
            a
            b
            c
            d
        "},
            model_output: indoc! {"
            <|set|>0:61-2:63
            FIRST
            <|set|>1:62-3:64
            SECOND
        "},
            expected: indoc! {"
            FIRST
            d
        "},
        },
        Case {
            name: "insert_before_first_and_after_line",
            original: indoc! {"
                a
                b
            "},
            model_output: indoc! {"
                <|insert|>
                HEAD
                <|insert|>0:61
                MID
            "},
            expected: indoc! {"
                HEAD
                a
                MID
                b
            "},
        },
    ];

    for case in &cases {
        let result = hashline::apply_edit_commands(case.original, &case.model_output);
        assert_eq!(result, case.expected, "failed case: {}", case.name);
    }
}
