
use super::*;
use gpui::{App, AppContext as _};
use indoc::indoc;
use language::{Buffer, rust_lang};
use util::test::{TextRangeMarker, marked_text_ranges_by};
use zeta_prompt::compute_editable_and_context_ranges;

struct TestCase {
    name: &'static str,
    marked_text: &'static str,
    editable_token_limit: usize,
    context_token_limit: usize,
}

#[gpui::test]
fn test_editable_and_context_ranges(cx: &mut App) {
    // Markers:
    // ˇ = cursor position
    // « » = expected editable range
    // [ ] = expected context range
    let test_cases = vec![
        TestCase {
            name: "small function fits entirely in editable and context",
            marked_text: indoc! {r#"
                    [«fn foo() {
                        let x = 1;ˇ
                        let y = 2;
                    }»]
                "#},
            editable_token_limit: 30,
            context_token_limit: 60,
        },
        TestCase {
            name: "cursor near end of function - editable expands to syntax boundaries",
            marked_text: indoc! {r#"
                    [fn first() {
                        let a = 1;
                        let b = 2;
                    }

                    fn foo() {
                    «    let x = 1;
                        let y = 2;
                        println!("{}", x + y);ˇ
                    }»]
                "#},
            editable_token_limit: 18,
            context_token_limit: 35,
        },
        TestCase {
            name: "cursor at function start - editable expands to syntax boundaries",
            marked_text: indoc! {r#"
                    [fn before() {
                    «    let a = 1;
                    }

                    fn foo() {ˇ
                        let x = 1;
                        let y = 2;
                        let z = 3;
                    }
                    »
                    fn after() {
                        let b = 2;
                    }]
                "#},
            editable_token_limit: 25,
            context_token_limit: 50,
        },
        TestCase {
            name: "tiny budget - just lines around cursor, no syntax expansion",
            marked_text: indoc! {r#"
                    fn outer() {
                    [    let line1 = 1;
                        let line2 = 2;
                    «    let line3 = 3;
                        let line4 = 4;ˇ»
                        let line5 = 5;
                        let line6 = 6;]
                        let line7 = 7;
                    }
                "#},
            editable_token_limit: 12,
            context_token_limit: 24,
        },
        TestCase {
            name: "context extends beyond editable",
            marked_text: indoc! {r#"
                    [fn first() { let a = 1; }
                    «fn second() { let b = 2; }
                    fn third() { let c = 3; }ˇ
                    fn fourth() { let d = 4; }»
                    fn fifth() { let e = 5; }]
                "#},
            editable_token_limit: 25,
            context_token_limit: 45,
        },
        TestCase {
            name: "cursor in first if-block - editable expands to syntax boundaries",
            marked_text: indoc! {r#"
                    [«fn before() { }

                    fn process() {
                        if condition1 {
                            let a = 1;ˇ
                            let b = 2;
                        }
                        if condition2 {»
                            let c = 3;
                            let d = 4;
                        }
                        if condition3 {
                            let e = 5;
                            let f = 6;
                        }
                    }

                    fn after() { }]
                "#},
            editable_token_limit: 35,
            context_token_limit: 60,
        },
        TestCase {
            name: "cursor in middle if-block - editable spans surrounding blocks",
            marked_text: indoc! {r#"
                    [fn before() { }

                    fn process() {
                        if condition1 {
                            let a = 1;
                    «        let b = 2;
                        }
                        if condition2 {
                            let c = 3;ˇ
                            let d = 4;
                        }
                        if condition3 {
                            let e = 5;»
                            let f = 6;
                        }
                    }

                    fn after() { }]
                "#},
            editable_token_limit: 40,
            context_token_limit: 60,
        },
        TestCase {
            name: "cursor near bottom of long function - context reaches function boundary",
            marked_text: indoc! {r#"
                    [fn other() { }

                    fn long_function() {
                        let line1 = 1;
                        let line2 = 2;
                        let line3 = 3;
                        let line4 = 4;
                        let line5 = 5;
                        let line6 = 6;
                    «    let line7 = 7;
                        let line8 = 8;
                        let line9 = 9;
                        let line10 = 10;ˇ
                        let line11 = 11;
                    }

                    fn another() { }»]
                "#},
            editable_token_limit: 40,
            context_token_limit: 55,
        },
        TestCase {
            name: "zero context budget - context equals editable",
            marked_text: indoc! {r#"
                    fn before() {
                        let p = 1;
                        let q = 2;
                    [«}

                    fn foo() {
                        let x = 1;ˇ
                        let y = 2;
                    }
                    »]
                    fn after() {
                        let r = 3;
                        let s = 4;
                    }
                "#},
            editable_token_limit: 15,
            context_token_limit: 0,
        },
    ];

    for test_case in test_cases {
        let cursor_marker: TextRangeMarker = 'ˇ'.into();
        let editable_marker: TextRangeMarker = ('«', '»').into();
        let context_marker: TextRangeMarker = ('[', ']').into();

        let (text, mut ranges) = marked_text_ranges_by(
            test_case.marked_text,
            vec![
                cursor_marker.clone(),
                editable_marker.clone(),
                context_marker.clone(),
            ],
        );

        let cursor_ranges = ranges.remove(&cursor_marker).unwrap_or_default();
        let expected_editable = ranges.remove(&editable_marker).unwrap_or_default();
        let expected_context = ranges.remove(&context_marker).unwrap_or_default();
        assert_eq!(expected_editable.len(), 1, "{}", test_case.name);
        assert_eq!(expected_context.len(), 1, "{}", test_case.name);

        cx.new(|cx: &mut gpui::Context<Buffer>| {
            let text = text.trim_end_matches('\n');
            let buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);
            let snapshot = buffer.snapshot();

            let cursor_offset = cursor_ranges[0].start;

            let (_, excerpt_offset_range, cursor_offset_in_excerpt) =
                compute_cursor_excerpt(&snapshot, cursor_offset);
            let excerpt_text: String = snapshot
                .text_for_range(excerpt_offset_range.clone())
                .collect();
            let syntax_ranges =
                compute_syntax_ranges(&snapshot, cursor_offset, &excerpt_offset_range);

            let (actual_editable, actual_context) = compute_editable_and_context_ranges(
                &excerpt_text,
                cursor_offset_in_excerpt,
                &syntax_ranges,
                test_case.editable_token_limit,
                test_case.context_token_limit,
            );

            let to_buffer_range = |range: Range<usize>| -> Range<usize> {
                (excerpt_offset_range.start + range.start)..(excerpt_offset_range.start + range.end)
            };

            let actual_editable = to_buffer_range(actual_editable);
            let actual_context = to_buffer_range(actual_context);

            let expected_editable_range = expected_editable[0].clone();
            let expected_context_range = expected_context[0].clone();

            let editable_match = actual_editable == expected_editable_range;
            let context_match = actual_context == expected_context_range;

            if !editable_match || !context_match {
                let range_text = |range: &Range<usize>| {
                    snapshot.text_for_range(range.clone()).collect::<String>()
                };

                println!("\n=== FAILED: {} ===", test_case.name);
                if !editable_match {
                    println!("\nExpected editable ({:?}):", expected_editable_range);
                    println!("---\n{}---", range_text(&expected_editable_range));
                    println!("\nActual editable ({:?}):", actual_editable);
                    println!("---\n{}---", range_text(&actual_editable));
                }
                if !context_match {
                    println!("\nExpected context ({:?}):", expected_context_range);
                    println!("---\n{}---", range_text(&expected_context_range));
                    println!("\nActual context ({:?}):", actual_context);
                    println!("---\n{}---", range_text(&actual_context));
                }
                panic!("Test '{}' failed - see output above", test_case.name);
            }

            buffer
        });
    }
}
