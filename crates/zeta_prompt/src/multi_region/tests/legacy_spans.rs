use super::*;

#[test]
fn test_extract_marker_span() {
    let text = "<|marker_2|>\n    new content\n<|marker_3|>\n";
    let (start, end, content) = extract_marker_span(text).unwrap();
    assert_eq!(start, 2);
    assert_eq!(end, 3);
    assert_eq!(content, "    new content\n");
}

#[test]
fn test_extract_marker_span_multi_line() {
    let text = "<|marker_1|>\nline1\nline2\nline3\n<|marker_4|>";
    let (start, end, content) = extract_marker_span(text).unwrap();
    assert_eq!(start, 1);
    assert_eq!(end, 4);
    assert_eq!(content, "line1\nline2\nline3\n");
}

#[test]
fn test_apply_marker_span_basic() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_1|>\naaa\nBBB\nccc\n<|marker_2|>";
    let result = apply_marker_span(old, output).unwrap();
    assert_eq!(result, "aaa\nBBB\nccc\n");
}

#[test]
fn test_apply_marker_span_preserves_trailing_blank_line() {
    let old = "/\nresult\n\n";
    let output = "<|marker_1|>\n//\nresult\n\n<|marker_2|>";
    let result = apply_marker_span(old, output).unwrap();
    assert_eq!(result, "//\nresult\n\n");
}

#[test]
fn test_encode_no_edits() {
    let old = "aaa\nbbb\nccc\n";
    let result = encode_from_old_and_new(
        old,
        old,
        None,
        "<|user_cursor|>",
        ">>>>>>> UPDATED\n",
        "NO_EDITS\n",
    )
    .unwrap();
    assert_eq!(result, "NO_EDITS\n>>>>>>> UPDATED\n");
}

#[test]
fn test_encode_with_change() {
    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nBBB\nccc\n";
    let result = encode_from_old_and_new(
        old,
        new,
        None,
        "<|user_cursor|>",
        ">>>>>>> UPDATED\n",
        "NO_EDITS\n",
    )
    .unwrap();
    assert!(result.contains("<|marker_1|>"));
    assert!(result.contains("<|marker_2|>"));
    assert!(result.contains("aaa\nBBB\nccc\n"));
    assert!(result.ends_with(">>>>>>> UPDATED\n"));
}

#[test]
fn test_roundtrip_encode_apply() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\nline9\nline10\n";
    let new = "line1\nline2\nline3\n\nline5\nLINE6\nline7\nline8\nline9\nline10\n";
    let encoded = encode_from_old_and_new(
        old,
        new,
        None,
        "<|user_cursor|>",
        ">>>>>>> UPDATED\n",
        "NO_EDITS\n",
    )
    .unwrap();
    let output = encoded
        .strip_suffix(">>>>>>> UPDATED\n")
        .expect("should have end marker");
    let reconstructed = apply_marker_span(old, output).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_extract_editable_region_from_markers_multi() {
    let text = "prefix\n<|marker_1|>\naaa\nbbb\n<|marker_2|>\nccc\nddd\n<|marker_3|>\nsuffix";
    let parsed = extract_editable_region_from_markers(text).unwrap();
    assert_eq!(parsed, "aaa\nbbb\nccc\nddd");
}

#[test]
fn test_extract_editable_region_two_markers() {
    let text = "<|marker_1|>\none\ntwo three\n<|marker_2|>";
    let parsed = extract_editable_region_from_markers(text).unwrap();
    assert_eq!(parsed, "one\ntwo three");
}

#[test]
fn test_encode_with_cursor() {
    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nBBB\nccc\n";
    let result = encode_from_old_and_new(
        old,
        new,
        Some(5),
        "<|user_cursor|>",
        ">>>>>>> UPDATED\n",
        "NO_EDITS\n",
    )
    .unwrap();
    assert!(result.contains("<|user_cursor|>"), "result: {result}");
    assert!(result.contains("B<|user_cursor|>BB"), "result: {result}");
}

#[test]
fn test_extract_marker_span_strips_intermediate_markers() {
    let text = "<|marker_2|>\nline1\n<|marker_3|>\nline2\n<|marker_4|>";
    let (start, end, content) = extract_marker_span(text).unwrap();
    assert_eq!(start, 2);
    assert_eq!(end, 4);
    assert_eq!(content, "line1\nline2\n");
}

#[test]
fn test_extract_marker_span_strips_multiple_intermediate_markers() {
    let text = "<|marker_1|>\naaa\n<|marker_2|>\nbbb\n<|marker_3|>\nccc\n<|marker_4|>";
    let (start, end, content) = extract_marker_span(text).unwrap();
    assert_eq!(start, 1);
    assert_eq!(end, 4);
    assert_eq!(content, "aaa\nbbb\nccc\n");
}

#[test]
fn test_apply_marker_span_with_extra_intermediate_marker() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_1|>\naaa\n<|marker_1|>\nBBB\nccc\n<|marker_2|>";
    let result = apply_marker_span(old, output).unwrap();
    assert_eq!(result, "aaa\nBBB\nccc\n");
}

#[test]
fn test_strip_marker_tags_inline() {
    assert_eq!(strip_marker_tags("no markers here"), "no markers here");
    assert_eq!(strip_marker_tags("before<|marker_5|>after"), "beforeafter");
    assert_eq!(
        strip_marker_tags("line1\n<|marker_3|>\nline2"),
        "line1\nline2"
    );
}
