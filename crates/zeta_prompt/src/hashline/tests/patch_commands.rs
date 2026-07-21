use super::*;

#[test]
fn test_patch_to_edit_commands() {
    struct Case {
        name: &'static str,
        old: &'static str,
        patch: &'static str,
        expected_new: &'static str,
    }

    let cases = [
        Case {
            name: "single_line_replacement",
            old: indoc! {"
            let mut total = 0;
            for product in products {
                total += ;
            }
            total
        "},
            patch: indoc! {"
            @@ -1,5 +1,5 @@
             let mut total = 0;
             for product in products {
            -    total += ;
            +    total += product.price;
             }
             total
        "},
            expected_new: indoc! {"
            let mut total = 0;
            for product in products {
                total += product.price;
            }
            total
        "},
        },
        Case {
            name: "multiline_replacement",
            old: indoc! {"
            fn foo() {
                let x = 1;
                let y = 2;
                let z = 3;
            }
        "},
            patch: indoc! {"
            @@ -1,5 +1,3 @@
             fn foo() {
            -    let x = 1;
            -    let y = 2;
            -    let z = 3;
            +    let sum = 1 + 2 + 3;
             }
        "},
            expected_new: indoc! {"
            fn foo() {
                let sum = 1 + 2 + 3;
            }
        "},
        },
        Case {
            name: "insertion",
            old: indoc! {"
            fn main() {
                let x = 1;
            }
        "},
            patch: indoc! {"
            @@ -1,3 +1,4 @@
             fn main() {
                 let x = 1;
            +    let y = 2;
             }
        "},
            expected_new: indoc! {"
            fn main() {
                let x = 1;
                let y = 2;
            }
        "},
        },
        Case {
            name: "insertion_before_first",
            old: indoc! {"
            let x = 1;
            let y = 2;
        "},
            patch: indoc! {"
            @@ -1,2 +1,3 @@
            +use std::io;
             let x = 1;
             let y = 2;
        "},
            expected_new: indoc! {"
            use std::io;
            let x = 1;
            let y = 2;
        "},
        },
        Case {
            name: "deletion",
            old: indoc! {"
            aaa
            bbb
            ccc
            ddd
        "},
            patch: indoc! {"
            @@ -1,4 +1,2 @@
             aaa
            -bbb
            -ccc
             ddd
        "},
            expected_new: indoc! {"
            aaa
            ddd
        "},
        },
        Case {
            name: "multiple_changes",
            old: indoc! {"
            alpha
            beta
            gamma
            delta
            epsilon
        "},
            patch: indoc! {"
            @@ -1,5 +1,5 @@
            -alpha
            +ALPHA
             beta
             gamma
            -delta
            +DELTA
             epsilon
        "},
            expected_new: indoc! {"
            ALPHA
            beta
            gamma
            DELTA
            epsilon
        "},
        },
        Case {
            name: "replace_with_insertion",
            old: indoc! {r#"
            fn handle() {
                modal_state.close();
                modal_state.dismiss();
        "#},
            patch: indoc! {r#"
            @@ -1,3 +1,4 @@
             fn handle() {
                 modal_state.close();
            +    eprintln!("");
                 modal_state.dismiss();
        "#},
            expected_new: indoc! {r#"
            fn handle() {
                modal_state.close();
                eprintln!("");
                modal_state.dismiss();
        "#},
        },
        Case {
            name: "complete_replacement",
            old: indoc! {"
            aaa
            bbb
            ccc
        "},
            patch: indoc! {"
            @@ -1,3 +1,3 @@
            -aaa
            -bbb
            -ccc
            +xxx
            +yyy
            +zzz
        "},
            expected_new: indoc! {"
            xxx
            yyy
            zzz
        "},
        },
        Case {
            name: "add_function_body",
            old: indoc! {"
            fn foo() {
                modal_state.dismiss();
            }

            fn

            fn handle_keystroke() {
        "},
            patch: indoc! {"
            @@ -1,6 +1,8 @@
             fn foo() {
                 modal_state.dismiss();
             }

            -fn
            +fn handle_submit() {
            +    todo()
            +}

             fn handle_keystroke() {
        "},
            expected_new: indoc! {"
            fn foo() {
                modal_state.dismiss();
            }

            fn handle_submit() {
                todo()
            }

            fn handle_keystroke() {
        "},
        },
        Case {
            name: "with_cursor_offset",
            old: indoc! {r#"
            fn main() {
                println!();
            }
        "#},
            patch: indoc! {r#"
                @@ -1,3 +1,3 @@
                fn main() {
                -    println!();
                +    eprintln!("");
                }
            "#},
            expected_new: indoc! {r#"
                fn main() {
                    eprintln!("<|user_cursor|>");
                }
            "#},
        },
        Case {
            name: "non_local_hunk_header_pure_insertion_repro",
            old: indoc! {"
                aaa
                bbb
            "},
            patch: indoc! {"
                @@ -20,2 +20,3 @@
                aaa
                +xxx
                bbb
            "},
            expected_new: indoc! {"
                aaa
                xxx
                bbb
            "},
        },
        Case {
            name: "empty_patch_produces_no_edits_marker",
            old: indoc! {"
                aaa
                bbb
            "},
            patch: "@@ -20,2 +20,3 @@\n",
            expected_new: indoc! {"
                aaa
                bbb
            "},
        },
    ];

    for case in &cases {
        // The cursor_offset for patch_to_edit_commands is relative to
        // the first hunk's new text (context + additions). We compute
        // it by finding where the marker sits in the expected output
        // (which mirrors the new text of the hunk).
        let cursor_offset = case.expected_new.find(CURSOR_MARKER);

        let commands = hashline::patch_to_edit_commands(case.old, case.patch, cursor_offset)
            .unwrap_or_else(|e| panic!("failed case {}: {e}", case.name));

        assert!(
            hashline::output_has_edit_commands(&commands),
            "case {}: expected edit commands, got: {commands:?}",
            case.name,
        );

        let applied = hashline::apply_edit_commands(case.old, &commands);
        assert_eq!(applied, case.expected_new, "case {}", case.name);
    }
}
