use super::*;

#[test]
fn test_tokenize() {
    let text = "";
    assert_eq!(tokenize(text, None).collect::<Vec<_>>(), Vec::<&str>::new());

    let text = " ";
    assert_eq!(tokenize(text, None).collect::<Vec<_>>(), vec![" "]);

    let text = "one";
    assert_eq!(tokenize(text, None).collect::<Vec<_>>(), vec!["one"]);

    let text = "one\n";
    assert_eq!(tokenize(text, None).collect::<Vec<_>>(), vec!["one", "\n"]);

    let text = "one.two(three)";
    assert_eq!(
        tokenize(text, None).collect::<Vec<_>>(),
        vec!["one", ".", "two", "(", "three", ")"]
    );

    let text = "one two three()";
    assert_eq!(
        tokenize(text, None).collect::<Vec<_>>(),
        vec!["one", " ", "two", " ", "three", "(", ")"]
    );

    let text = "   one\n two three";
    assert_eq!(
        tokenize(text, None).collect::<Vec<_>>(),
        vec!["   ", "one", "\n ", "two", " ", "three"]
    );
}

#[test]
fn test_text_diff() {
    let old_text = "one two three";
    let new_text = "one TWO three";
    assert_eq!(text_diff(old_text, new_text), [(4..7, "TWO".into()),]);

    let old_text = "one\ntwo\nthree\n";
    let new_text = "one\ntwo\nAND\nTHEN\nthree\n";
    assert_eq!(
        text_diff(old_text, new_text),
        [(8..8, "AND\nTHEN\n".into()),]
    );

    let old_text = "one two\nthree four five\nsix seven eight nine\nten\n";
    let new_text = "one two\nthree FOUR five\nsix SEVEN eight nine\nten\nELEVEN\n";
    assert_eq!(
        text_diff(old_text, new_text),
        [
            (14..18, "FOUR".into()),
            (28..33, "SEVEN".into()),
            (49..49, "ELEVEN\n".into())
        ]
    );
}

#[test]
fn test_apply_diff_patch() {
    let old_text = "one two\nthree four five\nsix seven eight nine\nten\n";
    let new_text = "one two\nthree FOUR five\nsix SEVEN eight nine\nten\nELEVEN\n";
    let patch = unified_diff(old_text, new_text);
    assert_eq!(apply_diff_patch(old_text, &patch).unwrap(), new_text);
}

#[test]
fn test_apply_reversed_diff_patch() {
    let old_text = "one two\nthree four five\nsix seven eight nine\nten\n";
    let new_text = "one two\nthree FOUR five\nsix SEVEN eight nine\nten\nELEVEN\n";
    let patch = unified_diff(old_text, new_text);
    assert_eq!(
        apply_reversed_diff_patch(new_text, &patch).unwrap(),
        old_text
    );
}

#[test]
fn test_char_diff() {
    assert_eq!(char_diff("", ""), vec![]);
    assert_eq!(char_diff("", "abc"), vec![(0..0, "abc")]);
    assert_eq!(char_diff("abc", ""), vec![(0..3, "")]);
    assert_eq!(char_diff("ac", "abc"), vec![(1..1, "b")]);
    assert_eq!(char_diff("abc", "ac"), vec![(1..2, "")]);
    assert_eq!(char_diff("abc", "adc"), vec![(1..2, "d")]);
    assert_eq!(char_diff("日", "日本語"), vec![(3..3, "本語")]);
    assert_eq!(char_diff("日本語", "日"), vec![(3..9, "")]);
    assert_eq!(char_diff("🎉", "🎉🎊🎈"), vec![(4..4, "🎊🎈")]);
    assert_eq!(
        char_diff("test日本", "test日本語です"),
        vec![(10..10, "語です")]
    );
}

#[test]
fn test_unified_diff_with_offsets() {
    let old_text = "foo\nbar\nbaz\n";
    let new_text = "foo\nBAR\nbaz\n";

    let expected_diff_body = " foo\n-bar\n+BAR\n baz\n";

    let diff_no_offset = unified_diff(old_text, new_text);
    assert_eq!(
        diff_no_offset,
        format!("@@ -1,3 +1,3 @@\n{}", expected_diff_body)
    );

    let diff_with_offset = unified_diff_with_offsets(old_text, new_text, 9, 11);
    assert_eq!(
        diff_with_offset,
        format!("@@ -10,3 +12,3 @@\n{}", expected_diff_body)
    );

    let diff_with_offset = unified_diff_with_offsets(old_text, new_text, 99, 104);
    assert_eq!(
        diff_with_offset,
        format!("@@ -100,3 +105,3 @@\n{}", expected_diff_body)
    );
}

#[test]
fn test_unified_diff_with_context() {
    let old_text = "line1\nline2\nline3\nline4\nline5\nCHANGE_ME\nline7\nline8\n";
    let new_text = "line1\nline2\nline3\nline4\nline5\nCHANGED\nline7\nline8\n";

    let diff_default = unified_diff_with_offsets(old_text, new_text, 0, 0);
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
