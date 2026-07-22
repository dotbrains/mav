use super::*;

#[test]
fn test_lenient_diff_application() {
    struct Case {
        name: &'static str,
        diff: &'static str,
        content: &'static str,
        expected_result: &'static str,
    }

    let cases = [
        Case {
            name: "hunk_context_not_found_skipped",
            diff: indoc! {"
                     @@ -1,3 +1,4 @@
                      context_not_in_content
                     +added_line
                      more_context
                      final_context
                 "},
            content: indoc! {"
                     completely
                     different
                     content
                 "},
            expected_result: indoc! {"
                     completely
                     different
                     content
                 "},
        },
        Case {
            name: "hunk_context_found_applied",
            diff: indoc! {"
                     @@ -1,3 +1,4 @@
                      line1
                     +inserted
                      line2
                      line3
                 "},
            content: indoc! {"
                     line1
                     line2
                     line3
                 "},
            expected_result: indoc! {"
                     line1
                     inserted
                     line2
                     line3
                 "},
        },
        Case {
            name: "multiple_hunks_partial_match",
            diff: indoc! {"
                     @@ -1,2 +1,3 @@
                      not_found
                     +skipped
                      also_not_found
                     @@ -5,2 +6,3 @@
                      line5
                     +applied
                      line6
                 "},
            content: indoc! {"
                     line1
                     line2
                     line3
                     line4
                     line5
                     line6
                 "},
            expected_result: indoc! {"
                     line1
                     line2
                     line3
                     line4
                     line5
                     applied
                     line6
                 "},
        },
        Case {
            name: "empty_diff",
            diff: "",
            content: indoc! {"
                     unchanged
                     content
                 "},
            expected_result: indoc! {"
                     unchanged
                     content
                 "},
        },
    ];

    for case in &cases {
        let result = apply_diff_to_string_lenient(case.diff, case.content);
        assert_eq!(
            result, case.expected_result,
            "Test '{}': expected:\n{}\ngot:\n{}",
            case.name, case.expected_result, result
        );
    }
}
