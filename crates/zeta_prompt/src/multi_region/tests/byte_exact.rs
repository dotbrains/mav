use super::*;

#[test]
fn test_write_editable_with_markers_v0316_byte_exact() {
    let editable = "aaa\nbbb\nccc\n";
    let mut output = String::new();
    write_editable_with_markers_v0316(&mut output, editable, 4, "<|user_cursor|>");
    assert!(output.starts_with("<|marker_1|>"));
    assert!(output.contains("<|user_cursor|>"));
    let stripped = output.replace("<|user_cursor|>", "");
    let stripped = strip_marker_tags(&stripped);
    assert_eq!(stripped, editable);
}

#[test]
fn test_apply_marker_span_v0316_basic() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_1|>aaa\nBBB\nccc\n<|marker_2|>";
    let result = apply_marker_span_v0316(old, output).unwrap();
    assert_eq!(result, "aaa\nBBB\nccc\n");
}

#[test]
fn test_apply_marker_span_v0316_no_edit() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_1|><|marker_1|>";
    let result = apply_marker_span_v0316(old, output).unwrap();
    assert_eq!(result, old);
}

#[test]
fn test_apply_marker_span_v0316_no_edit_any_marker() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_2|>ignored content<|marker_2|>";
    let result = apply_marker_span_v0316(old, output).unwrap();
    assert_eq!(result, old);
}

#[test]
fn test_apply_marker_span_v0316_multi_block() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\n";
    let marker_offsets = compute_marker_offsets(old);
    assert!(
        marker_offsets.len() >= 3,
        "expected at least 3 offsets, got {:?}",
        marker_offsets
    );

    let new_content = "LINE1\nLINE2\nLINE3\n\nLINE5\nLINE6\nLINE7\nLINE8\n";
    let mut output = String::new();
    output.push_str("<|marker_1|>");
    for i in 0..marker_offsets.len() - 1 {
        if i > 0 {
            output.push_str(&marker_tag(i + 1));
        }
        let start = marker_offsets[i];
        let end = marker_offsets[i + 1];
        let block_len = end - start;
        output.push_str(&new_content[start..start + block_len]);
    }
    let last_marker_num = marker_offsets.len();
    output.push_str(&marker_tag(last_marker_num));
    let result = apply_marker_span_v0316(old, &output).unwrap();
    assert_eq!(result, new_content);
}

#[test]
fn test_apply_marker_span_v0316_byte_exact_no_normalization() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker_1|>aaa\nBBB\nccc<|marker_2|>";
    let result = apply_marker_span_v0316(old, output).unwrap();
    assert_eq!(result, "aaa\nBBB\nccc");
}

#[test]
fn test_encode_v0316_no_edits() {
    let old = "aaa\nbbb\nccc\n";
    let result =
        encode_from_old_and_new_v0316(old, old, Some(5), "<|user_cursor|>", "<|end|>").unwrap();
    assert!(result.ends_with("<|end|>"));
    let stripped = result.strip_suffix("<|end|>").unwrap();
    let result_parsed = apply_marker_span_v0316(old, stripped).unwrap();
    assert_eq!(result_parsed, old);
}

#[test]
fn test_encode_v0316_with_change() {
    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nBBB\nccc\n";
    let result =
        encode_from_old_and_new_v0316(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();
    assert!(result.contains("<|marker_1|>"));
    assert!(result.contains("<|marker_2|>"));
    assert!(result.ends_with("<|end|>"));
}

#[test]
fn test_roundtrip_v0316() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\nline9\nline10\n";
    let new = "line1\nline2\nline3\n\nline5\nLINE6\nline7\nline8\nline9\nline10\n";
    let encoded =
        encode_from_old_and_new_v0316(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();
    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let reconstructed = apply_marker_span_v0316(old, stripped).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_roundtrip_v0316_with_cursor() {
    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nBBB\nccc\n";
    let result =
        encode_from_old_and_new_v0316(old, new, Some(5), "<|user_cursor|>", "<|end|>").unwrap();
    assert!(result.contains("<|user_cursor|>"), "result: {result}");
    assert!(result.contains("B<|user_cursor|>BB"), "result: {result}");
}

#[test]
fn test_roundtrip_v0316_multi_block_change() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\n";
    let new = "line1\nLINE2\nline3\n\nline5\nLINE6\nline7\nline8\n";
    let encoded =
        encode_from_old_and_new_v0316(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();
    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let reconstructed = apply_marker_span_v0316(old, stripped).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_nearest_marker_number() {
    let offsets = vec![0, 10, 20, 30];
    assert_eq!(nearest_marker_number(Some(0), &offsets), 1);
    assert_eq!(nearest_marker_number(Some(9), &offsets), 2);
    assert_eq!(nearest_marker_number(Some(15), &offsets), 2);
    assert_eq!(nearest_marker_number(Some(25), &offsets), 3);
    assert_eq!(nearest_marker_number(Some(30), &offsets), 4);
    assert_eq!(nearest_marker_number(None, &offsets), 1);
}

#[test]
fn test_marker_tag_relative_formats_as_expected() {
    assert_eq!(marker_tag_relative(-2), "<|marker-2|>");
    assert_eq!(marker_tag_relative(-1), "<|marker-1|>");
    assert_eq!(marker_tag_relative(0), "<|marker-0|>");
    assert_eq!(marker_tag_relative(1), "<|marker+1|>");
    assert_eq!(marker_tag_relative(2), "<|marker+2|>");
}

#[test]
fn test_write_editable_with_markers_v0317_includes_relative_markers_and_cursor() {
    let editable = "aaa\nbbb\nccc\n";
    let mut output = String::new();
    write_editable_with_markers_v0317(&mut output, editable, 4, "<|user_cursor|>");

    assert!(output.contains("<|marker-0|>"));
    assert!(output.contains("<|user_cursor|>"));

    let stripped = output.replace("<|user_cursor|>", "");
    let stripped =
        collect_relative_marker_tags(&stripped)
            .iter()
            .fold(stripped.clone(), |acc, marker| {
                let tag = &stripped[marker.tag_start..marker.tag_end];
                acc.replace(tag, "")
            });
    assert_eq!(stripped, editable);
}

#[test]
fn test_apply_marker_span_v0317_basic() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker-0|>aaa\nBBB\nccc\n<|marker+1|>";
    let result = apply_marker_span_v0317(old, output, Some(0)).unwrap();
    assert_eq!(result, "aaa\nBBB\nccc\n");
}

#[test]
fn test_apply_marker_span_v0317_no_edit() {
    let old = "aaa\nbbb\nccc\n";
    let output = "<|marker-0|><|marker-0|>";
    let result = apply_marker_span_v0317(old, output, Some(0)).unwrap();
    assert_eq!(result, old);
}

#[test]
fn test_encode_v0317_no_edits() {
    let old = "aaa\nbbb\nccc\n";
    let result =
        encode_from_old_and_new_v0317(old, old, Some(5), "<|user_cursor|>", "<|end|>").unwrap();
    assert_eq!(result, "<|marker-0|><|marker-0|><|end|>");
}

#[test]
fn test_roundtrip_v0317() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\n";
    let new = "line1\nLINE2\nline3\n\nline5\nLINE6\nline7\nline8\n";
    let cursor = Some(6);

    let encoded =
        encode_from_old_and_new_v0317(old, new, cursor, "<|user_cursor|>", "<|end|>").unwrap();
    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let stripped = stripped.replace("<|user_cursor|>", "");
    let reconstructed = apply_marker_span_v0317(old, &stripped, cursor).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_roundtrip_v0317_with_cursor_marker() {
    let old = "aaa\nbbb\nccc\n";
    let new = "aaa\nBBB\nccc\n";
    let result =
        encode_from_old_and_new_v0317(old, new, Some(5), "<|user_cursor|>", "<|end|>").unwrap();
    assert!(result.contains("<|user_cursor|>"), "result: {result}");
    assert!(result.contains("<|marker-0|>"), "result: {result}");
}

#[test]
fn test_compute_marker_offsets_v0318_uses_larger_block_sizes() {
    let text = "l1\nl2\nl3\n\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\n";
    let v0316_offsets = compute_marker_offsets(text);
    let v0318_offsets = compute_marker_offsets_v0318(text);

    assert!(v0318_offsets.len() < v0316_offsets.len());
    assert_eq!(v0316_offsets.first().copied(), Some(0));
    assert_eq!(v0318_offsets.first().copied(), Some(0));
    assert_eq!(v0316_offsets.last().copied(), Some(text.len()));
    assert_eq!(v0318_offsets.last().copied(), Some(text.len()));
}

#[test]
fn test_roundtrip_v0318() {
    let old = "line1\nline2\nline3\n\nline5\nline6\nline7\nline8\nline9\nline10\n";
    let new = "line1\nline2\nline3\n\nline5\nLINE6\nline7\nline8\nline9\nline10\n";
    let encoded =
        encode_from_old_and_new_v0318(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();
    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let reconstructed = apply_marker_span_v0318(old, stripped).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_roundtrip_v0318_append_at_end_of_editable_region() {
    let old = "line1\nline2\nline3\n";
    let new = "line1\nline2\nline3\nline4\n";
    let encoded =
        encode_from_old_and_new_v0318(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();

    assert_ne!(encoded, "<|marker_2|><|end|>");

    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let reconstructed = apply_marker_span_v0318(old, stripped).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_roundtrip_v0318_insert_at_internal_marker_boundary() {
    let old = "alpha\nbeta\n\ngamma\ndelta\n";
    let new = "alpha\nbeta\n\ninserted\ngamma\ndelta\n";
    let encoded =
        encode_from_old_and_new_v0318(old, new, None, "<|user_cursor|>", "<|end|>").unwrap();

    let stripped = encoded
        .strip_suffix("<|end|>")
        .expect("should have end marker");
    let reconstructed = apply_marker_span_v0318(old, stripped).unwrap();
    assert_eq!(reconstructed, new);
}

#[test]
fn test_encode_v0317_markers_stay_on_line_boundaries() {
    let old = "\
\t\t\t\tcontinue outer;
\t\t\t}
\t\t}
\t}

\tconst intersectionObserver = new IntersectionObserver((entries) => {
\t\tfor (const entry of entries) {
\t\t\tif (entry.isIntersecting) {
\t\t\t\tintersectionObserver.unobserve(entry.target);
\t\t\t\tanchorPreload(/** @type {HTMLAnchorElement} */ (entry.target));
\t\t\t}
\t\t}
\t});

\tconst observer = new MutationObserver(() => {
\t\tconst links = /** @type {NodeListOf<HTMLAnchorElement>} */ (
\t\t\tdocument.querySelectorAll('a[data-preload]')
\t\t);

\t\tfor (const link of links) {
\t\t\tif (linkSet.has(link)) continue;
\t\t\tlinkSet.add(link);

\t\t\tswitch (link.dataset.preload) {
\t\t\t\tcase '':
\t\t\t\tcase 'true':
\t\t\t\tcase 'hover': {
\t\t\t\t\tlink.addEventListener('mouseenter', function callback() {
\t\t\t\t\t\tlink.removeEventListener('mouseenter', callback);
\t\t\t\t\t\tanchorPreload(link);
\t\t\t\t\t});
";
    let new = old.replacen(
        "\t\t\t\tcase 'true':\n",
        "\t\t\t\tcase 'TRUE':<|user_cursor|>\n",
        1,
    );

    let cursor_offset = new.find("<|user_cursor|>").expect("cursor marker in new");
    let new_without_cursor = new.replace("<|user_cursor|>", "");

    let encoded = encode_from_old_and_new_v0317(
        old,
        &new_without_cursor,
        Some(cursor_offset),
        "<|user_cursor|>",
        "<|end|>",
    )
    .unwrap();

    let core = encoded.strip_suffix("<|end|>").unwrap_or(&encoded);
    for marker in collect_relative_marker_tags(core) {
        let tag_start = marker.tag_start;
        assert!(
            tag_start == 0 || core.as_bytes()[tag_start - 1] == b'\n',
            "marker not at line boundary: {} in output:\n{}",
            marker_tag_relative(marker.value),
            core
        );
    }
}
