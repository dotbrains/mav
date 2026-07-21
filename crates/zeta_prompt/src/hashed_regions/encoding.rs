use crate::{Zeta2PromptInput, udiff};
use anyhow::{Context as _, Result};
use std::path::Path;

use super::location::{merge_contiguous_snippets, snippet_path_and_start_row};
use super::markers::build_marker_table;
use super::{NO_EDITS, V0615_END_MARKER, marker_tag};

fn common_prefix_suffix(a: &[u8], b: &[u8]) -> (usize, usize) {
    let prefix = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let remaining_a = a.len() - prefix;
    let remaining_b = b.len() - prefix;
    let max_suffix = remaining_a.min(remaining_b);
    let suffix = a[a.len() - max_suffix..]
        .iter()
        .rev()
        .zip(b[b.len() - max_suffix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (prefix, suffix)
}

fn nearest_marker_id(markers: &[(String, usize)], cursor_offset: Option<usize>) -> &str {
    let cursor = cursor_offset.unwrap_or(0);
    markers
        .iter()
        .min_by_key(|(_, offset)| (*offset as isize - cursor as isize).unsigned_abs())
        .map(|(id, _)| id.as_str())
        .unwrap_or("unknown")
}

/// Encode a single marker-bounded edit block for one snippet, given its old and
/// new text. The returned block starts and ends with a marker tag and does
/// **not** include the output end marker; callers concatenate blocks and append
/// [`V0615_END_MARKER`] once after the last block.
pub fn encode_from_old_and_new(
    old_text: &str,
    new_text: &str,
    markers: &[(String, usize)],
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
) -> Result<String> {
    let no_edit_id = nearest_marker_id(markers, cursor_offset_in_new);
    if old_text == new_text {
        let tag = marker_tag(no_edit_id);
        return Ok(format!("{tag}{tag}"));
    }

    let (common_prefix, common_suffix) =
        common_prefix_suffix(old_text.as_bytes(), new_text.as_bytes());
    let change_end_in_old = old_text.len() - common_suffix;
    let mut start_marker_ix = markers
        .iter()
        .rposition(|(_, offset)| *offset <= common_prefix)
        .unwrap_or(0);
    let mut end_marker_ix = markers
        .iter()
        .position(|(_, offset)| *offset >= change_end_in_old)
        .unwrap_or_else(|| markers.len().saturating_sub(1));

    if start_marker_ix == end_marker_ix {
        if end_marker_ix < markers.len().saturating_sub(1) {
            end_marker_ix += 1;
        } else if start_marker_ix > 0 {
            start_marker_ix -= 1;
        }
    }

    let old_start = markers
        .get(start_marker_ix)
        .map(|(_, offset)| *offset)
        .context("start marker out of range")?;
    let old_end = markers
        .get(end_marker_ix)
        .map(|(_, offset)| *offset)
        .context("end marker out of range")?;
    let new_start = old_start;
    let new_end = new_text
        .len()
        .saturating_sub(old_text.len().saturating_sub(old_end));
    let new_span = &new_text[new_start..new_end];

    let mut result = String::new();
    result.push_str(&marker_tag(&markers[start_marker_ix].0));
    result.push('\n');
    if let Some(cursor_offset) = cursor_offset_in_new {
        if cursor_offset >= new_start && cursor_offset <= new_end {
            let cursor_in_span = cursor_offset - new_start;
            result.push_str(&new_span[..cursor_in_span]);
            result.push_str(cursor_marker);
            result.push_str(&new_span[cursor_in_span..]);
        } else {
            result.push_str(new_span);
        }
    } else {
        result.push_str(new_span);
    }
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&marker_tag(&markers[end_marker_ix].0));
    Ok(result)
}

/// Parse student model output (raw marker spans, no markdown code fences) into
/// a unified patch.
///
/// The output is a run of marker tags with no fences, so blocks are delimited by
/// pairing tags two at a time: `(1, 2), (3, 4), ...`. This matches the encoder,

pub fn encode_patch_as_output(
    input: &Zeta2PromptInput,
    patch: &str,
    cursor_offset: Option<usize>,
    cursor_marker: &str,
) -> Result<String> {
    if patch.lines().count() <= 3 {
        return Ok(format!("{NO_EDITS}{V0615_END_MARKER}"));
    }

    let marker_table = build_marker_table(input);
    let snippets = merge_contiguous_snippets(input, marker_table)?;
    let mut parser = udiff::DiffParser::new(patch);
    let mut blocks: Vec<String> = Vec::new();

    while let Some(event) = parser.next().context("failed to parse expected patch")? {
        let udiff::DiffEvent::Hunk {
            path,
            mut hunk,
            status: _,
        } = event
        else {
            continue;
        };

        // A hunk whose file isn't in the prompt context is unreachable; skip it
        // and keep any other reachable hunks (partial edit).
        let Some((snippet_ix, start_row)) =
            snippets
                .iter()
                .enumerate()
                .find_map(|(snippet_ix, snippet)| {
                    let (snippet_path, start_row) =
                        snippet_path_and_start_row(input, snippet).ok()?;
                    (snippet_path == Path::new(path.as_ref())).then_some((snippet_ix, start_row))
                })
        else {
            continue;
        };
        let snippet = &snippets[snippet_ix];
        let old_text = snippet.text.as_ref();
        let candidates = udiff::find_context_candidates(old_text, &mut hunk);
        // A hunk whose location can't be pinned down within the snippet is
        // unreachable; skip it.
        let Some(hunk_offset) =
            udiff::disambiguate_by_line_number(&candidates, hunk.start_line, &|offset| {
                start_row + old_text[..offset].matches('\n').count() as u32
            })
        else {
            continue;
        };

        let mut new_text = old_text.to_string();
        for edit in hunk.edits.iter().rev() {
            let range = (hunk_offset + edit.range.start)..(hunk_offset + edit.range.end);
            new_text.replace_range(range, &edit.text);
        }
        // The cursor marker is placed in every region whose span contains it.
        // The extracted `cursor_offset` is hunk-relative, so map it through each
        // hunk's offset; `encode_from_old_and_new` inserts it only when it lands
        // within that block's span.
        let cursor_in_new = cursor_offset.map(|cursor| (hunk_offset + cursor).min(new_text.len()));
        blocks.push(encode_from_old_and_new(
            old_text,
            &new_text,
            &snippet.markers,
            cursor_in_new,
            cursor_marker,
        )?);
    }

    if blocks.is_empty() {
        return Ok(format!("{NO_EDITS}{V0615_END_MARKER}"));
    }

    let mut output = blocks.join("\n");
    output.push_str(V0615_END_MARKER);
    Ok(output)
}
