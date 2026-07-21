//! Hashed Regions (V0609HashedRegions): a variant of the Smart Regions
//! multi-region format where marker tags are identified by a short
//! content-derived hash (e.g. `<|marker_b1f8|>`) instead of a sequence
//! number.
//!
//! Hashed identifiers are self-describing: a tag can be mapped back to its
//! location without reproducing the exact rendering order of the prompt, so
//! markers can be placed across *all* prompt context, and budget-based
//! truncation of related files doesn't shift the addressing of the remaining
//! markers. All context, including the current file, lives in related files:
//! context retrieval includes the current file via `ContextSource::CurrentFile`,
//! so the cursor file is expected to be one of the related files. Inputs that
//! weren't run through current-file retrieval can be normalized with
//! [`ensure_cursor_file_excerpt`] before rendering or parsing.

pub const MARKER_TAG_PREFIX: &str = "<|marker_";
pub const MARKER_TAG_SUFFIX: &str = "|>";
pub const V0615_END_MARKER: &str = "<[end▁of▁sentence]>";
pub const NO_EDITS: &str = "NO_EDITS";
/// Number of base64 characters in a marker tag identifier.
pub const TAG_ID_LEN: usize = 4;

const BASE64_URL_SAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

mod encoding;
pub use encoding::{encode_from_old_and_new, encode_patch_as_output};

mod location;
pub use location::{
    RelatedFileCursor, ensure_cursor_file_excerpt, locate_cursor_in_related_files,
    marker_table_for_excerpt, related_file_patch_path,
};

mod markers;
#[cfg(test)]
use markers::unique_tag_id;
pub use markers::{
    SnippetMarkers, build_editable_marker_table, build_marker_table, extract_marker_span,
    extract_marker_span_allow_same, is_hash_region_editable_context_source, marker_tag,
    markers_for_text, write_snippet_with_markers,
};

mod parsing;
pub use parsing::{HashRegionCursor, build_patch_from_spans, parse_output_as_patch};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContextSource, RelatedExcerpt, RelatedFile, Zeta2PromptInput};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn make_input(cursor_excerpt: &str, related: &[(&str, &[&str])]) -> Zeta2PromptInput {
        Zeta2PromptInput {
            cursor_path: PathBuf::from("src/main.rs").into(),
            cursor_excerpt: cursor_excerpt.into(),
            cursor_offset_in_excerpt: 0,
            excerpt_start_row: Some(0),
            events: Vec::new(),
            related_files: Some(
                related
                    .iter()
                    .map(|(path, excerpts)| {
                        let mut row = 0;
                        RelatedFile {
                            path: Arc::from(PathBuf::from(path).as_path()),
                            max_row: 1000,
                            excerpts: excerpts
                                .iter()
                                .map(|text| {
                                    let row_count = text.matches('\n').count() as u32;
                                    let excerpt = RelatedExcerpt {
                                        row_range: row..row + row_count,
                                        text: Arc::from(*text),
                                        order: 0,
                                        context_source: ContextSource::CurrentFile,
                                    };
                                    row += row_count + 10;
                                    excerpt
                                })
                                .collect(),
                            in_open_source_repo: false,
                        }
                    })
                    .collect(),
            ),
            active_buffer_diagnostics: Vec::new(),
            excerpt_ranges: crate::ExcerptRanges::default(),
            syntax_ranges: None,
            in_open_source_repo: false,
            can_collect_data: false,
            repo_url: None,
        }
    }

    #[test]
    fn test_ensure_cursor_file_excerpt_synthesizes_when_uncovered() {
        // The cursor file's only related excerpt is a fragment elsewhere in the
        // file (rows 40..42), not covering the cursor at row 1.
        let mut input = make_input(
            "fn main() {\n    let x = 1;\n}\n",
            &[("src/main.rs", &["// unrelated\n// fragment\n"])],
        );
        input.cursor_offset_in_excerpt = 16; // inside "    let x = 1;"
        input.related_files.as_mut().unwrap()[0].excerpts[0].row_range = 40..42;

        assert!(locate_cursor_in_related_files(&input).is_none());
        assert!(ensure_cursor_file_excerpt(&mut input));

        let cursor =
            locate_cursor_in_related_files(&input).expect("cursor covered after synthesis");
        let file = &input.related_files.as_ref().unwrap()[cursor.file_ix];
        // The fragment was replaced by the synthesized full window, so the file
        // content isn't duplicated with overlapping markers.
        assert_eq!(file.excerpts.len(), 1);
        assert_eq!(file.excerpts[0].context_source, ContextSource::CurrentFile);
        assert_eq!(file.excerpts[0].row_range, 0..3);
        assert_eq!(
            file.excerpts[0].text.as_ref(),
            "fn main() {\n    let x = 1;\n}\n"
        );
    }

    #[test]
    fn test_ensure_cursor_file_excerpt_noop_when_covered() {
        // make_input places the cursor file's excerpt at rows 0..3, covering the
        // cursor at row 1.
        let mut input = make_input(
            "fn main() {\n    let x = 1;\n}\n",
            &[("src/main.rs", &["fn main() {\n    let x = 1;\n}\n"])],
        );
        input.cursor_offset_in_excerpt = 16;
        let before = input.clone();
        assert!(ensure_cursor_file_excerpt(&mut input));
        assert_eq!(input, before);
    }

    #[test]
    fn test_tag_ids_are_unique_even_for_identical_blocks() {
        let mut used = HashSet::new();
        let id_a = unique_tag_id("same content", &mut used);
        let id_b = unique_tag_id("same content", &mut used);
        assert_ne!(id_a, id_b);
        assert_eq!(id_a.len(), TAG_ID_LEN);
        assert_eq!(id_b.len(), TAG_ID_LEN);
    }

    #[test]
    fn test_tag_ids_are_deterministic() {
        let mut used_a = HashSet::new();
        let mut used_b = HashSet::new();
        assert_eq!(
            unique_tag_id("hello\nworld\n", &mut used_a),
            unique_tag_id("hello\nworld\n", &mut used_b)
        );
    }

    #[test]
    fn test_build_marker_table_covers_all_context() {
        let input = make_input(
            "fn main() {\n    println!();\n}\n",
            &[
                ("src/a.rs", &["struct A;\n", "impl A {}\n"]),
                ("src/b.rs", &["struct B;\n"]),
            ],
        );
        let table = build_marker_table(&input);
        assert_eq!(table.len(), 3);
        assert_eq!((table[0].file_ix, table[0].excerpt_ix), (0, 0));
        assert_eq!((table[1].file_ix, table[1].excerpt_ix), (0, 1));
        assert_eq!((table[2].file_ix, table[2].excerpt_ix), (1, 0));

        let mut all_ids = HashSet::new();
        for snippet in &table {
            assert!(snippet.markers.len() >= 2);
            assert_eq!(snippet.markers.first().map(|(_, offset)| *offset), Some(0));
            for (id, _) in &snippet.markers {
                assert!(all_ids.insert(id.clone()), "duplicate tag id {id}");
            }
        }
    }

    #[test]
    fn test_write_snippet_with_markers_and_cursor() {
        let text = "fn main() {\n    let x = 1;\n}\n";
        let markers = vec![("aaaa".to_string(), 0), ("bbbb".to_string(), text.len())];
        let mut output = String::new();
        write_snippet_with_markers(&mut output, text, &markers, Some((16, "<|user_cursor|>")));
        assert_eq!(
            output,
            "<|marker_aaaa|>\nfn main() {\n    <|user_cursor|>let x = 1;\n}\n<|marker_bbbb|>"
        );
    }

    #[test]
    fn test_extract_marker_span_round_trip() {
        let codeblock = "<|marker_aaaa|>\nnew content\n<|marker_bbbb|>";
        let (start, end, content) = extract_marker_span(codeblock).unwrap();
        assert_eq!(start, "aaaa");
        assert_eq!(end, "bbbb");
        assert_eq!(content, "new content\n");
    }

    #[test]
    fn test_extract_marker_span_strips_intermediate_tags() {
        let codeblock = "<|marker_aaaa|>\nline one\n<|marker_cccc|>\nline two\n<|marker_bbbb|>";
        let (start, end, content) = extract_marker_span(codeblock).unwrap();
        assert_eq!(start, "aaaa");
        assert_eq!(end, "bbbb");
        assert_eq!(content, "line one\nline two\n");
    }

    #[test]
    fn test_extract_marker_span_rejects_single_marker() {
        assert!(extract_marker_span("<|marker_aaaa|>\ncontent\n").is_err());
    }

    #[test]
    fn test_extract_marker_span_rejects_same_marker() {
        assert!(extract_marker_span("<|marker_aaaa|>\ncontent\n<|marker_aaaa|>").is_err());
    }

    const MULTI_FN_EXCERPT: &str = "fn alpha() {\n    one();\n}\n\nfn beta() {\n    two();\n}\n\nfn gamma() {\n    three();\n}\n";

    const TWO_HUNK_PATCH: &str = concat!(
        "--- a/src/main.rs\n",
        "+++ b/src/main.rs\n",
        "@@ -1,3 +1,3 @@\n",
        " fn alpha() {\n",
        "-    one();\n",
        "+    uno();\n",
        " }\n",
        "@@ -9,3 +9,3 @@\n",
        " fn gamma() {\n",
        "-    three();\n",
        "+    tres();\n",
        " }\n",
    );

    #[test]
    fn test_encode_multi_hunk_emits_multiple_blocks() {
        let input = make_input(MULTI_FN_EXCERPT, &[("src/main.rs", &[MULTI_FN_EXCERPT])]);
        let output =
            encode_patch_as_output(&input, TWO_HUNK_PATCH, None, "<|user_cursor|>").unwrap();

        assert!(output.ends_with(V0615_END_MARKER), "output: {output}");
        // Two blocks => four marker tags, exactly one end marker.
        assert_eq!(
            output.matches(MARKER_TAG_PREFIX).count(),
            4,
            "output: {output}"
        );
        assert_eq!(
            output.matches(V0615_END_MARKER).count(),
            1,
            "output: {output}"
        );
        assert!(output.contains("uno();"), "output: {output}");
        assert!(output.contains("tres();"), "output: {output}");
    }

    #[test]
    fn test_round_trip_multi_hunk() {
        let input = make_input(MULTI_FN_EXCERPT, &[("src/main.rs", &[MULTI_FN_EXCERPT])]);
        let output =
            encode_patch_as_output(&input, TWO_HUNK_PATCH, None, "<|user_cursor|>").unwrap();
        let patch = parse_output_as_patch(&input, &output, "<|user_cursor|>").unwrap();

        assert!(patch.contains("-    one();"), "patch: {patch}");
        assert!(patch.contains("+    uno();"), "patch: {patch}");
        assert!(patch.contains("-    three();"), "patch: {patch}");
        assert!(patch.contains("+    tres();"), "patch: {patch}");
    }

    #[test]
    fn test_encode_partial_skips_unreachable_hunk() {
        // Second hunk targets a file that is not in the prompt context, so it
        // is unreachable. The first (reachable) hunk is still encoded.
        let patch = format!(
            "{TWO_HUNK_PATCH}--- a/other.rs\n+++ b/other.rs\n@@ -1,1 +1,1 @@\n-gone();\n+kept();\n"
        );
        let input = make_input(MULTI_FN_EXCERPT, &[("src/main.rs", &[MULTI_FN_EXCERPT])]);
        let output = encode_patch_as_output(&input, &patch, None, "<|user_cursor|>").unwrap();

        assert_ne!(output.trim_end_matches(V0615_END_MARKER), NO_EDITS);
        assert!(output.contains("uno();"), "output: {output}");
        assert!(output.contains("tres();"), "output: {output}");
        assert!(!output.contains("kept();"), "output: {output}");
    }

    #[test]
    fn test_encode_no_edits_when_all_hunks_unreachable() {
        let patch = "--- a/other.rs\n+++ b/other.rs\n@@ -1,3 +1,3 @@\n fn x() {\n-    gone();\n+    kept();\n }\n";
        let input = make_input(MULTI_FN_EXCERPT, &[("src/main.rs", &[MULTI_FN_EXCERPT])]);
        let output = encode_patch_as_output(&input, patch, None, "<|user_cursor|>").unwrap();

        assert_eq!(output, format!("{NO_EDITS}{V0615_END_MARKER}"));
    }

    #[test]
    fn test_parse_multiple_direct_marker_blocks() {
        // The student emits raw marker spans with no code fences; blocks are
        // delimited by pairing tags two at a time.
        let input = make_input(MULTI_FN_EXCERPT, &[("src/main.rs", &[MULTI_FN_EXCERPT])]);
        let markers = build_marker_table(&input)[0].markers.clone();
        assert!(markers.len() >= 3, "expected internal markers: {markers:?}");

        let tag = |ix: usize| marker_tag(&markers[ix].0);
        let old_first = &MULTI_FN_EXCERPT[markers[0].1..markers[1].1];
        let old_second = &MULTI_FN_EXCERPT[markers[1].1..markers[markers.len() - 1].1];
        let new_first = old_first.replace("one()", "uno()");
        let new_second = old_second.replace("three()", "tres()");

        let output = format!(
            "{}\n{}{}\n{}\n{}{}{}",
            tag(0),
            new_first,
            tag(1),
            tag(1),
            new_second,
            tag(markers.len() - 1),
            V0615_END_MARKER,
        );

        let patch = parse_output_as_patch(&input, &output, "<|user_cursor|>").unwrap();
        assert!(patch.contains("+    uno();"), "patch: {patch}");
        assert!(patch.contains("+    tres();"), "patch: {patch}");
        assert_eq!(
            patch.matches("--- a/src/main.rs").count(),
            1,
            "patch: {patch}"
        );
    }
}
