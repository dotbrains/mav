use super::*;

#[test]
fn test_filter_hunks_by_excerpt_region() {
    struct Case {
        name: &'static str,
        diff: &'static str,
        excerpt_start_row: u32,
        excerpt_row_count: u32,
        expected_filtered_diff: &'static str,
        expected_line_offset: i32,
    }

    let cases = [
        Case {
            name: "hunk_entirely_before_excerpt",
            diff: indoc! {"
                     @@ -1,3 +1,4 @@
                      line1
                     +inserted
                      line2
                      line3
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 5,
            expected_filtered_diff: "",
            expected_line_offset: 1,
        },
        Case {
            name: "hunk_entirely_inside_excerpt",
            diff: indoc! {"
                     @@ -12,3 +12,4 @@
                      line12
                     +inserted
                      line13
                      line14
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -2,3 +2,4 @@
                      line12
                     +inserted
                      line13
                      line14
                 "},
            expected_line_offset: 1,
        },
        Case {
            name: "hunk_entirely_after_excerpt",
            diff: indoc! {"
                     @@ -50,3 +50,4 @@
                      line50
                     +inserted
                      line51
                      line52
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 5,
            expected_filtered_diff: "",
            expected_line_offset: 0,
        },
        Case {
            name: "hunk_straddles_excerpt_start",
            diff: indoc! {"
                     @@ -8,5 +8,6 @@
                      line8
                      line9
                     +inserted
                      line10
                      line11
                      line12
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -1,3 +1,3 @@
                      line10
                      line11
                      line12
                 "},
            expected_line_offset: 1,
        },
        Case {
            name: "hunk_straddles_excerpt_end",
            diff: indoc! {"
                     @@ -18,5 +18,6 @@
                      line18
                      line19
                     +inserted
                      line20
                      line21
                      line22
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -8,2 +8,3 @@
                      line18
                      line19
                     +inserted
                 "},
            expected_line_offset: 1,
        },
        Case {
            name: "multiple_hunks_mixed",
            diff: indoc! {"
                     @@ -1,2 +1,3 @@
                      line1
                     +before_excerpt
                      line2
                     @@ -12,2 +13,3 @@
                      line12
                     +inside_excerpt
                      line13
                     @@ -50,2 +52,3 @@
                      line50
                     +after_excerpt
                      line51
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -3,2 +3,3 @@
                      line12
                     +inside_excerpt
                      line13
                 "},
            expected_line_offset: 2,
        },
        Case {
            name: "deletion_before_excerpt",
            diff: indoc! {"
                     @@ -1,4 +1,3 @@
                      line1
                     -deleted
                      line2
                      line3
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 5,
            expected_filtered_diff: "",
            expected_line_offset: -1,
        },
        Case {
            name: "deletion_inside_excerpt",
            diff: indoc! {"
                     @@ -12,4 +12,3 @@
                      line12
                     -deleted
                      line13
                      line14
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -2,4 +2,3 @@
                      line12
                     -deleted
                      line13
                      line14
                 "},
            expected_line_offset: -1,
        },
        Case {
            name: "empty_diff",
            diff: "",
            excerpt_start_row: 10,
            excerpt_row_count: 5,
            expected_filtered_diff: "",
            expected_line_offset: 0,
        },
        Case {
            name: "hunk_spans_entire_excerpt",
            diff: indoc! {"
                     @@ -8,10 +8,12 @@
                      line8
                      line9
                      line10
                      line11
                     +inserted1
                      line12
                      line13
                     +inserted2
                      line14
                      line15
                      line16
                      line17
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 5,
            expected_filtered_diff: indoc! {"
                     @@ -1,3 +1,5 @@
                      line11
                     +inserted1
                      line12
                      line13
                     +inserted2
                 "},
            expected_line_offset: 2,
        },
        Case {
            name: "replacement_inside_excerpt",
            diff: indoc! {"
                     @@ -12,3 +12,3 @@
                      line12
                     -old_text
                     +new_text
                      line14
                 "},
            excerpt_start_row: 10,
            excerpt_row_count: 10,
            expected_filtered_diff: indoc! {"
                     @@ -2,3 +2,3 @@
                      line12
                     -old_text
                     +new_text
                      line14
                 "},
            expected_line_offset: 0,
        },
    ];

    for case in &cases {
        let (filtered, line_offset) =
            filter_diff_hunks_by_excerpt(case.diff, case.excerpt_start_row, case.excerpt_row_count);
        assert_eq!(
            filtered, case.expected_filtered_diff,
            "Test '{}': filtered diff mismatch.\nExpected:\n{}\nGot:\n{}",
            case.name, case.expected_filtered_diff, filtered
        );
        assert_eq!(
            line_offset, case.expected_line_offset,
            "Test '{}': line offset mismatch. Expected {}, got {}",
            case.name, case.expected_line_offset, line_offset
        );
    }
}

#[test]
fn test_excerpt_aware_reversal_tracking() {
    struct Case {
        name: &'static str,
        edit_history_diffs: Vec<&'static str>,
        excerpt_content: &'static str,
        excerpt_start_row: u32,
        predicted_content: &'static str,
        expected_reversal_chars: usize,
        expected_total_chars: usize,
    }

    let cases = [
        Case {
            name: "edit_outside_excerpt_no_reversal",
            edit_history_diffs: vec![indoc! {"
                     @@ -1,2 +1,3 @@
                      line1
                     +added_outside
                      line2
                 "}],
            excerpt_content: indoc! {"
                     line10
                     line11
                     line12
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     modified
                     line12
                 "},
            expected_reversal_chars: 0,
            expected_total_chars: 14,
        },
        Case {
            name: "edit_inside_excerpt_with_reversal",
            edit_history_diffs: vec![indoc! {"
                     @@ -10,3 +10,4 @@
                      line10
                     +user_added
                      line11
                      line12
                 "}],
            excerpt_content: indoc! {"
                     line10
                     user_added
                     line11
                     line12
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     line11
                     line12
                 "},
            expected_reversal_chars: 11,
            expected_total_chars: 11,
        },
        Case {
            name: "straddling_edit_partial_reversal",
            edit_history_diffs: vec![indoc! {"
                     @@ -8,6 +8,8 @@
                      line8
                      line9
                     +before_excerpt
                      line10
                     +inside_excerpt
                      line11
                      line12
                      line13
                 "}],
            excerpt_content: indoc! {"
                     line10
                     inside_excerpt
                     line11
                     line12
                     line13
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     line11
                     line12
                     line13
                 "},
            expected_reversal_chars: 15,
            expected_total_chars: 15,
        },
        Case {
            name: "multiple_edits_mixed_locations",
            edit_history_diffs: vec![
                indoc! {"
                         @@ -1,2 +1,3 @@
                          line1
                         +outside1
                          line2
                     "},
                indoc! {"
                         @@ -11,2 +12,3 @@
                          line11
                         +inside1
                          line12
                     "},
            ],
            excerpt_content: indoc! {"
                     line10
                     line11
                     inside1
                     line12
                     line13
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     line11
                     line12
                     line13
                 "},
            expected_reversal_chars: 8,
            expected_total_chars: 8,
        },
        Case {
            name: "no_edit_history",
            edit_history_diffs: vec![],
            excerpt_content: indoc! {"
                     line10
                     line11
                     line12
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     modified
                     line12
                 "},
            expected_reversal_chars: 0,
            expected_total_chars: 14,
        },
        Case {
            name: "edit_after_excerpt_no_effect",
            edit_history_diffs: vec![indoc! {"
                     @@ -50,2 +50,3 @@
                      line50
                     +added_after
                      line51
                 "}],
            excerpt_content: indoc! {"
                     line10
                     line11
                     line12
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     changed
                     line12
                 "},
            expected_reversal_chars: 0,
            expected_total_chars: 13,
        },
        Case {
            name: "line_offset_tracking_across_hunks",
            edit_history_diffs: vec![
                indoc! {"
                         @@ -1,2 +1,4 @@
                          line1
                         +added1
                         +added2
                          line2
                     "},
                indoc! {"
                         @@ -12,2 +14,3 @@
                          line12
                         +inside_after_offset
                          line13
                     "},
            ],
            excerpt_content: indoc! {"
                     line10
                     line11
                     line12
                     inside_after_offset
                     line13
                 "},
            excerpt_start_row: 10,
            predicted_content: indoc! {"
                     line10
                     line11
                     line12
                     line13
                 "},
            expected_reversal_chars: 20,
            expected_total_chars: 20,
        },
    ];

    for case in &cases {
        let overlap = compute_excerpt_aware_reversal_overlap(
            &case.edit_history_diffs,
            case.excerpt_content,
            case.excerpt_start_row,
            case.predicted_content,
        );
        assert_eq!(
            overlap.chars_reversing_user_edits, case.expected_reversal_chars,
            "Test '{}': expected {} reversal chars, got {}",
            case.name, case.expected_reversal_chars, overlap.chars_reversing_user_edits
        );
        assert_eq!(
            overlap.total_chars_in_prediction, case.expected_total_chars,
            "Test '{}': expected {} total chars, got {}",
            case.name, case.expected_total_chars, overlap.total_chars_in_prediction
        );
    }
}
