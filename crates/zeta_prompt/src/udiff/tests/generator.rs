use super::*;

#[test]
fn test_unified_diff_with_context_matches_expected_context_window() {
    let old_text = "line1\nline2\nline3\nline4\nline5\nCHANGE_ME\nline7\nline8\n";
    let new_text = "line1\nline2\nline3\nline4\nline5\nCHANGED\nline7\nline8\n";

    let diff_default = unified_diff_with_context(old_text, new_text, 0, 0, 3);
    assert_eq!(
        diff_default,
        "@@ -3,6 +3,6 @@\n line3\n line4\n line5\n-CHANGE_ME\n+CHANGED\n line7\n line8\n"
    );

    let diff_full_context = unified_diff_with_context(old_text, new_text, 0, 0, 8);
    assert_eq!(
        diff_full_context,
        "@@ -1,8 +1,8 @@\n line1\n line2\n line3\n line4\n line5\n-CHANGE_ME\n+CHANGED\n line7\n line8\n"
    );

    let diff_no_context = unified_diff_with_context(old_text, new_text, 0, 0, 0);
    assert_eq!(diff_no_context, "@@ -6,1 +6,1 @@\n-CHANGE_ME\n+CHANGED\n");
}
