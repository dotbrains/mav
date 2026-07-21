use crate::{ContextSource, Zeta2PromptInput, multi_region};
use anyhow::{Context as _, Result, anyhow};
use std::collections::HashSet;

use super::{BASE64_URL_SAFE_ALPHABET, MARKER_TAG_PREFIX, MARKER_TAG_SUFFIX, TAG_ID_LEN};

pub fn marker_tag(id: &str) -> String {
    format!("{MARKER_TAG_PREFIX}{id}{MARKER_TAG_SUFFIX}")
}

/// Marker tags assigned to one contiguous snippet of context.
#[derive(Debug, Clone)]
pub struct SnippetMarkers {
    pub file_ix: usize,
    pub excerpt_ix: usize,
    /// `(tag id, byte offset within the snippet text)`, sorted by offset.
    /// The first marker is at offset 0 and the last at `text.len()`.
    pub markers: Vec<(String, usize)>,
}

/// Assign hashed marker tags to every related-file excerpt of `input`.
///
/// The assignment is deterministic and independent of any later budget-based
/// truncation, so the same table can be rebuilt when parsing model output.
pub fn build_marker_table(input: &Zeta2PromptInput) -> Vec<SnippetMarkers> {
    build_marker_table_with_filter(input, |_| true)
}

pub fn build_editable_marker_table(input: &Zeta2PromptInput) -> Vec<SnippetMarkers> {
    build_marker_table_with_filter(input, is_hash_region_editable_context_source)
}

pub fn is_hash_region_editable_context_source(context_source: ContextSource) -> bool {
    matches!(
        context_source,
        ContextSource::CurrentFile | ContextSource::EditHistory
    )
}

fn build_marker_table_with_filter(
    input: &Zeta2PromptInput,
    include_context_source: impl Fn(ContextSource) -> bool,
) -> Vec<SnippetMarkers> {
    let mut used_ids = HashSet::new();
    let mut snippets = Vec::new();
    if let Some(related_files) = input.related_files.as_deref() {
        for (file_ix, file) in related_files.iter().enumerate() {
            for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
                if include_context_source(excerpt.context_source) {
                    snippets.push(SnippetMarkers {
                        file_ix,
                        excerpt_ix,
                        markers: assign_tags(&excerpt.text, &mut used_ids),
                    });
                }
            }
        }
    }
    snippets
}

pub fn markers_for_text(text: &str) -> Vec<(String, usize)> {
    let mut used_ids = HashSet::new();
    assign_tags(text, &mut used_ids)
}

fn assign_tags(text: &str, used_ids: &mut HashSet<String>) -> Vec<(String, usize)> {
    let offsets = multi_region::compute_marker_offsets_v0618(text);
    offsets
        .iter()
        .enumerate()
        .map(|(i, &offset)| {
            let block = match offsets.get(i + 1) {
                Some(&next_offset) => &text[offset..next_offset],
                // The final marker has no following block; hash the preceding
                // one. This collides with the previous marker's tag by
                // construction, which `unique_tag_id` resolves by reseeding.
                None => {
                    let previous_offset = if i == 0 { 0 } else { offsets[i - 1] };
                    &text[previous_offset..offset]
                }
            };
            (unique_tag_id(block, used_ids), offset)
        })
        .collect()
}

pub(crate) fn unique_tag_id(content: &str, used_ids: &mut HashSet<String>) -> String {
    let mut seed = 0u64;
    loop {
        let id = encode_tag_id(hash_with_seed(content, seed));
        if used_ids.insert(id.clone()) {
            return id;
        }
        seed += 1;
    }
}

/// FNV-1a, with the seed folded in ahead of the content.
fn hash_with_seed(content: &str, seed: u64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in seed.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn encode_tag_id(hash: u64) -> String {
    (0..TAG_ID_LEN)
        .map(|i| BASE64_URL_SAFE_ALPHABET[((hash >> (6 * i)) & 0x3f) as usize] as char)
        .collect()
}

/// Write `text` into `output`, inserting marker tags at the given offsets.
/// When `cursor` is provided, its marker string is inserted at the given byte
/// offset within `text`.
pub fn write_snippet_with_markers(
    output: &mut String,
    text: &str,
    markers: &[(String, usize)],
    cursor: Option<(usize, &str)>,
) {
    let mut cursor_placed = false;
    for (i, (id, offset)) in markers.iter().enumerate() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&marker_tag(id));

        if let Some((_, next_offset)) = markers.get(i + 1) {
            output.push('\n');
            let block = &text[*offset..*next_offset];
            match cursor {
                Some((cursor_offset, cursor_marker))
                    if !cursor_placed
                        && cursor_offset >= *offset
                        && cursor_offset <= *next_offset =>
                {
                    cursor_placed = true;
                    let cursor_in_block = cursor_offset - offset;
                    output.push_str(&block[..cursor_in_block]);
                    output.push_str(cursor_marker);
                    output.push_str(&block[cursor_in_block..]);
                }
                _ => output.push_str(block),
            }
        }
    }
}

/// Extract a marker-bounded span from a model-output codeblock.
///
/// Returns `(start tag id, end tag id, content)` where `content` is the text
/// between the first and last marker tags, with any intermediate marker tags
/// stripped.
pub fn extract_marker_span(text: &str) -> Result<(String, String, String)> {
    let (start_id, end_id, content) = extract_marker_span_allow_same(text)?;
    if start_id == end_id {
        return Err(anyhow!(
            "start and end markers are the same (marker {start_id})"
        ));
    }
    Ok((start_id, end_id, content))
}

pub fn extract_marker_span_allow_same(text: &str) -> Result<(String, String, String)> {
    let first_tag_start = text
        .find(MARKER_TAG_PREFIX)
        .context("no start marker found in output")?;
    let first_id_start = first_tag_start + MARKER_TAG_PREFIX.len();
    let first_id_end = text[first_id_start..]
        .find(MARKER_TAG_SUFFIX)
        .map(|i| i + first_id_start)
        .context("malformed start marker tag")?;
    let start_id = &text[first_id_start..first_id_end];
    let first_tag_end = first_id_end + MARKER_TAG_SUFFIX.len();

    let last_tag_start = text
        .rfind(MARKER_TAG_PREFIX)
        .context("no end marker found in output")?;
    if last_tag_start == first_tag_start {
        return Err(anyhow!("output span must be bounded by two marker tags"));
    }
    let last_id_start = last_tag_start + MARKER_TAG_PREFIX.len();
    let last_id_end = text[last_id_start..]
        .find(MARKER_TAG_SUFFIX)
        .map(|i| i + last_id_start)
        .context("malformed end marker tag")?;
    let end_id = &text[last_id_start..last_id_end];

    let mut content_start = first_tag_end;
    if text.as_bytes().get(content_start) == Some(&b'\n') {
        content_start += 1;
    }
    let content_end = last_tag_start;
    let content = &text[content_start..content_end.max(content_start)];
    let content = multi_region::strip_marker_tags(content);
    Ok((start_id.to_string(), end_id.to_string(), content))
}
