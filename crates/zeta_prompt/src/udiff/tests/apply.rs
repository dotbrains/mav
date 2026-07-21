use super::*;
use indoc::indoc;

#[test]
fn test_apply_diff_to_string_no_trailing_newline() {
    // Text without trailing newline; diff generated without
    // `\ No newline at end of file` marker.
    let text = "line1\nline2\nline3";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             line1
            -line2
            +replaced
             line3
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "line1\nreplaced\nline3");
}

#[test]
fn test_apply_diff_to_string_trailing_newline_present() {
    // When text has a trailing newline, exact matching still works and
    // the fallback is never needed.
    let text = "line1\nline2\nline3\n";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             line1
            -line2
            +replaced
             line3
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "line1\nreplaced\nline3\n");
}

#[test]
fn test_apply_diff_to_string_deletion_at_end_no_trailing_newline() {
    // Deletion of the last line when text has no trailing newline.
    // The edit range must be clamped so it doesn't index past the
    // end of the text.
    let text = "line1\nline2\nline3";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,2 @@
             line1
             line2
            -line3
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "line1\nline2\n");
}

#[test]
fn test_apply_diff_to_string_replace_last_line_no_trailing_newline() {
    // Replace the last line when text has no trailing newline.
    let text = "aaa\nbbb\nccc";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             aaa
             bbb
            -ccc
            +ddd
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "aaa\nbbb\nddd");
}

#[test]
fn test_apply_diff_to_string_multibyte_no_trailing_newline() {
    // Multi-byte UTF-8 characters near the end; ensures char boundary
    // safety when the fallback clamps edit ranges.
    let text = "hello\n세계";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,2 @@
             hello
            -세계
            +world
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "hello\nworld");
}

#[test]
fn test_apply_diff_to_string_adjusts_line_numbers_after_prior_hunks() {
    let text = "first\nremove first\nfirst\nsame\nremove\nsame\nsame\nremove\nsame\n";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,2 @@
             first
            -remove first
             first
            @@ -4,3 +3,2 @@
             same
            -remove
             same
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(result, "first\nfirst\nsame\nsame\nsame\nremove\nsame\n");
}

#[test]
fn test_apply_diff_to_string_adjusts_line_numbers_after_prior_insertion_hunks() {
    let text = "first\nfirst\nsame\nremove\nsame\nsame\nremove\nsame\n";
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,3 @@
             first
            +inserted
             first
            @@ -6,3 +7,2 @@
             same
            -remove
             same
        "};

    let result = apply_diff_to_string(diff, text).unwrap();
    assert_eq!(
        result,
        "first\ninserted\nfirst\nsame\nremove\nsame\nsame\nsame\n"
    );
}

#[test]
fn test_find_context_candidates_no_false_positive_mid_text() {
    // The stripped fallback must only match at the end of text, not in
    // the middle where a real newline exists.
    let text = "aaa\nbbb\nccc\n";
    let mut hunk = Hunk {
        context: "bbb\n".into(),
        edits: vec![],
        start_line: None,
    };

    let candidates = find_context_candidates(text, &mut hunk);
    // Exact match at offset 4 — the fallback is not used.
    assert_eq!(candidates, vec![4]);
}

#[test]
fn test_find_context_candidates_fallback_at_end() {
    let text = "aaa\nbbb";
    let mut hunk = Hunk {
        context: "bbb\n".into(),
        edits: vec![],
        start_line: None,
    };

    let candidates = find_context_candidates(text, &mut hunk);
    assert_eq!(candidates, vec![4]);
    // Context should be stripped.
    assert_eq!(hunk.context, "bbb");
}

#[test]
fn test_find_context_candidates_no_fallback_mid_text() {
    // "bbb" appears mid-text followed by a newline, so the exact
    // match succeeds. Verify the stripped fallback doesn't produce a
    // second, spurious candidate.
    let text = "aaa\nbbb\nccc";
    let mut hunk = Hunk {
        context: "bbb\nccc\n".into(),
        edits: vec![],
        start_line: None,
    };

    let candidates = find_context_candidates(text, &mut hunk);
    // No exact match (text ends without newline after "ccc"), but the
    // stripped context "bbb\nccc" matches at offset 4, which is the end.
    assert_eq!(candidates, vec![4]);
    assert_eq!(hunk.context, "bbb\nccc");
}

#[test]
fn test_find_context_candidates_clamps_edit_ranges() {
    let text = "aaa\nbbb";
    let mut hunk = Hunk {
        context: "aaa\nbbb\n".into(),
        edits: vec![Edit {
            range: 4..8, // "bbb\n" — end points at the trailing \n
            text: "ccc\n".into(),
        }],
        start_line: None,
    };

    let candidates = find_context_candidates(text, &mut hunk);
    assert_eq!(candidates, vec![0]);
    // Edit range end should be clamped to 7 (new context length).
    assert_eq!(hunk.edits[0].range, 4..7);
}
