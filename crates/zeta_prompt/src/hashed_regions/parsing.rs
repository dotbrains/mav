use crate::{Zeta2PromptInput, multi_region, udiff};
use anyhow::{Context as _, Result, anyhow};
use std::{collections::HashMap, ops::Range, path::PathBuf};

use super::location::{ParseSnippet, merge_contiguous_snippets, snippet_path_and_start_row};
use super::markers::build_marker_table;
use super::{MARKER_TAG_PREFIX, MARKER_TAG_SUFFIX, NO_EDITS, V0615_END_MARKER};

pub fn parse_output_as_patch(
    input: &Zeta2PromptInput,
    output: &str,
    cursor_marker: &str,
) -> Result<String> {
    let output = output.strip_suffix(V0615_END_MARKER).unwrap_or(output);
    if output.trim() == NO_EDITS {
        return Ok(String::new());
    }

    let spans = pair_marker_spans(output)?;
    let (patch, _cursor) = build_patch_from_spans(input, &spans, cursor_marker)?;
    Ok(patch)
}

/// A cursor position resolved while turning marker-span edits into a patch.
pub struct HashRegionCursor {
    pub path: PathBuf,
    /// Byte offset of the cursor within `new_text`.
    pub cursor_offset_in_new_text: usize,
    /// Full new text of the edited snippet, after applying all of its edits.
    pub new_text: String,
    /// Original text of the edited snippet.
    pub old_text: String,
    /// 0-based row where the snippet starts in its file.
    pub start_row: u32,
}

/// One marker-bounded edit resolved against a parse snippet.
struct ParsedSpanEdit {
    snippet_ix: usize,
    range: Range<usize>,
    new_text: String,
    cursor_offset_in_new_text: Option<usize>,
}

/// Split raw model output into marker-bounded spans by pairing marker tags two
/// at a time. Returns `(start_id, end_id, raw_new_span)` per pair, where
/// `raw_new_span` may still contain the cursor marker.
fn pair_marker_spans(output: &str) -> Result<Vec<(String, String, String)>> {
    let tags = find_all_marker_tags(output);
    if tags.len() < 2 {
        return Err(anyhow!("output does not contain a marker-bounded span"));
    }
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 1 < tags.len() {
        let (start_id, _, start_tag_end) = &tags[i];
        let (end_id, end_tag_start, _) = &tags[i + 1];
        let content = &output[*start_tag_end..*end_tag_start];
        let content = content.strip_prefix('\n').unwrap_or(content);
        let content = multi_region::strip_marker_tags(content);
        spans.push((start_id.clone(), end_id.clone(), content));
        i += 2;
    }
    Ok(spans)
}

/// Find every marker tag in `text`, in order, as `(id, tag_start, tag_end)`.
fn find_all_marker_tags(text: &str) -> Vec<(String, usize, usize)> {
    let mut tags = Vec::new();
    let mut search = 0;
    while let Some(rel) = text[search..].find(MARKER_TAG_PREFIX) {
        let tag_start = search + rel;
        let id_start = tag_start + MARKER_TAG_PREFIX.len();
        let Some(suffix_rel) = text[id_start..].find(MARKER_TAG_SUFFIX) else {
            break;
        };
        let id_end = id_start + suffix_rel;
        let tag_end = id_end + MARKER_TAG_SUFFIX.len();
        tags.push((text[id_start..id_end].to_string(), tag_start, tag_end));
        search = tag_end;
    }
    tags
}

/// Resolve a list of marker spans into per-snippet edits and assemble a unified
/// patch.
///
/// `spans` is a list of `(start_id, end_id, raw_new_span)` where `raw_new_span`
/// may still contain `cursor_marker`. This is shared by the student parser
/// (which pairs raw marker tags) and the teacher parser (which extracts spans
/// from markdown code fences). Edits that overlap an already-accepted edit in
/// the same snippet are skipped (lenient). The cursor marker is honored in
/// every region that contains it; the returned [`HashRegionCursor`] reports the
/// first such position.
pub fn build_patch_from_spans(
    input: &Zeta2PromptInput,
    spans: &[(String, String, String)],
    cursor_marker: &str,
) -> Result<(String, Option<HashRegionCursor>)> {
    let marker_table = build_marker_table(input);
    let snippets = merge_contiguous_snippets(input, marker_table)?;
    let mut marker_index: HashMap<&str, (usize, usize)> = HashMap::new();
    for (snippet_ix, snippet) in snippets.iter().enumerate() {
        for (id, offset) in &snippet.markers {
            marker_index.insert(id.as_str(), (snippet_ix, *offset));
        }
    }

    let mut edits: Vec<ParsedSpanEdit> = Vec::new();
    for (start_id, end_id, raw_new_span) in spans {
        let &(start_snippet, start_byte) = marker_index
            .get(start_id.as_str())
            .with_context(|| format!("unknown start marker `{start_id}`"))?;
        let &(end_snippet, end_byte) = marker_index
            .get(end_id.as_str())
            .with_context(|| format!("unknown end marker `{end_id}`"))?;

        if start_snippet != end_snippet {
            return Err(anyhow!(
                "markers `{start_id}` and `{end_id}` belong to different context snippets \
                 that are not contiguous excerpts of the same file"
            ));
        }
        if start_byte > end_byte {
            return Err(anyhow!(
                "start marker `{start_id}` must come before end marker `{end_id}`"
            ));
        }

        let old_text = snippets[start_snippet].text.as_ref();
        let old_span = &old_text[start_byte..end_byte];

        let cursor_in_span = raw_new_span.find(cursor_marker);
        let mut new_span = raw_new_span.replace(cursor_marker, "");
        if old_span.is_empty() {
            if !new_span.is_empty() && !new_span.ends_with('\n') {
                new_span.push('\n');
            }
        } else {
            if old_span.ends_with('\n') && !new_span.ends_with('\n') && !new_span.is_empty() {
                new_span.push('\n');
            }
            if !old_span.ends_with('\n') && new_span.ends_with('\n') {
                new_span.pop();
            }
        }

        if !new_span.is_empty()
            && let Some(dropped) = detect_trailing_deletion(old_span, &new_span)
        {
            return Err(anyhow!(
                "edit span `{start_id}`..`{end_id}` looks truncated: the replacement \
                 stops before the end marker, which would silently delete:\n{dropped}"
            ));
        }

        // `cursor_in_span` was located in `raw_new_span` before the trailing
        // newline normalization above, which can drop a byte. Clamp it to the
        // finalized replacement so the offset never points past `new_span`
        // (downstream cursor mapping byte-slices `new_text` by this offset).
        let cursor_offset_in_new_text = cursor_in_span.map(|offset| offset.min(new_span.len()));
        edits.push(ParsedSpanEdit {
            snippet_ix: start_snippet,
            range: start_byte..end_byte,
            new_text: new_span,
            cursor_offset_in_new_text,
        });
    }

    assemble_patch_from_edits(input, &snippets, edits)
}

/// Apply resolved edits to their snippets and emit one diff section per edited
/// snippet, in the order snippets first appear in the edit sequence.
fn assemble_patch_from_edits(
    input: &Zeta2PromptInput,
    snippets: &[ParseSnippet<'_>],
    edits: Vec<ParsedSpanEdit>,
) -> Result<(String, Option<HashRegionCursor>)> {
    let mut snippet_order: Vec<usize> = Vec::new();
    for edit in &edits {
        if !snippet_order.contains(&edit.snippet_ix) {
            snippet_order.push(edit.snippet_ix);
        }
    }

    let mut diff_output = String::new();
    let mut cursor = None;

    for &snippet_ix in &snippet_order {
        let snippet = &snippets[snippet_ix];
        let mut snippet_edits: Vec<&ParsedSpanEdit> = edits
            .iter()
            .filter(|edit| edit.snippet_ix == snippet_ix)
            .collect();
        snippet_edits.sort_by_key(|edit| edit.range.start);

        // Lenient overlap handling: keep edits in line order, dropping any whose
        // range starts before the previous accepted edit ended.
        let mut accepted: Vec<&ParsedSpanEdit> = Vec::new();
        let mut last_end = 0usize;
        for edit in snippet_edits {
            if !accepted.is_empty() && edit.range.start < last_end {
                continue;
            }
            last_end = edit.range.end;
            accepted.push(edit);
        }

        let old_text = snippet.text.as_ref();
        let (path, start_row) = snippet_path_and_start_row(input, snippet)?;

        let mut new_text = String::new();
        let mut position = 0;
        let mut cursor_in_new_text = None;
        for edit in &accepted {
            new_text.push_str(&old_text[position..edit.range.start]);
            if let Some(cursor_offset) = edit.cursor_offset_in_new_text {
                cursor_in_new_text = Some(new_text.len() + cursor_offset);
            }
            new_text.push_str(&edit.new_text);
            position = edit.range.end;
        }
        new_text.push_str(&old_text[position..]);

        let diff = udiff::unified_diff_with_context(old_text, &new_text, start_row, start_row, 3);
        if !diff.is_empty() {
            let path_str = path
                .iter()
                .map(|component| component.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            diff_output.push_str(&format!("--- a/{path_str}\n+++ b/{path_str}\n"));
            diff_output.push_str(&diff);
            if !diff_output.ends_with('\n') {
                diff_output.push('\n');
            }
        }

        if cursor.is_none()
            && let Some(cursor_offset) = cursor_in_new_text
        {
            cursor = Some(HashRegionCursor {
                path: path.clone(),
                cursor_offset_in_new_text: cursor_offset,
                new_text: new_text.clone(),
                old_text: old_text.to_string(),
                start_row,
            });
        }
    }

    Ok((diff_output, cursor))
}

/// Detects a span replacement that ends in a pure deletion of the span's tail,
/// the signature of a model that stopped writing before reaching its end
/// marker.
///
/// Returns the deleted tail if the line diff between `old_span` and `new_span`
/// ends with a deletion-only group that reaches the last line of `old_span` and
/// drops more than `MAX_TRAILING_DELETED_LINES` non-blank lines.
fn detect_trailing_deletion(old_span: &str, new_span: &str) -> Option<String> {
    const MAX_TRAILING_DELETED_LINES: usize = 3;

    fn flag_if_large(deleted_tail: &str) -> Option<String> {
        let non_blank_deleted = deleted_tail
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        (non_blank_deleted > MAX_TRAILING_DELETED_LINES)
            .then(|| deleted_tail.trim_end().to_string())
    }

    // A verbatim prefix is checked at the byte level so that a replacement
    // stopping mid-line is caught too; the line diff below would see that as a
    // trailing replace group rather than a pure deletion.
    if let Some(deleted_tail) = old_span.strip_prefix(new_span) {
        return flag_if_large(deleted_tail);
    }

    // With zero context lines, hunks contain only `-` and `+` lines, and within
    // a hunk deletions precede insertions, so a diff whose final line is a
    // deletion ends with a deletion-only group.
    let diff = udiff::unified_diff_with_context(old_span, new_span, 0, 0, 0);
    let lines: Vec<&str> = diff.lines().collect();
    let mut deletion_start = lines.len();
    while deletion_start > 0 && lines[deletion_start - 1].starts_with('-') {
        deletion_start -= 1;
    }
    let deleted: Vec<&str> = lines[deletion_start..]
        .iter()
        .map(|line| line.strip_prefix('-').unwrap_or(line))
        .collect();
    if deleted.is_empty() {
        return None;
    }

    // The trailing `-` run is preceded by its hunk header exactly when the hunk
    // is deletion-only (a replacement group would interpose `+` lines).
    let header = lines.get(deletion_start.checked_sub(1)?)?;
    let old_range_start: usize = header
        .strip_prefix("@@ -")?
        .split(',')
        .next()?
        .parse()
        .ok()?;

    // Only flag deletions that reach the end of the span; a deletion in the
    // middle is followed by reproduced context, so the model demonstrably kept
    // writing past it.
    if old_range_start + deleted.len() - 1 != old_span.lines().count() {
        return None;
    }

    flag_if_large(&deleted.join("\n"))
}
