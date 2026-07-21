use anyhow::{Context as _, Result, anyhow};

pub const MARKER_TAG_PREFIX: &str = "<|marker_";
pub const MARKER_TAG_SUFFIX: &str = "|>";
pub const RELATIVE_MARKER_TAG_PREFIX: &str = "<|marker";
pub const V0316_END_MARKER: &str = "<[end▁of▁sentence]>";
pub const V0317_END_MARKER: &str = "<[end▁of▁sentence]>";
pub const V0318_END_MARKER: &str = "<[end▁of▁sentence]>";
pub const V0327_END_MARKER: &str = "<[end▁of▁sentence]>";

pub fn marker_tag(number: usize) -> String {
    format!("{MARKER_TAG_PREFIX}{number}{MARKER_TAG_SUFFIX}")
}

pub fn marker_tag_relative(delta: isize) -> String {
    if delta > 0 {
        format!("<|marker+{delta}|>")
    } else if delta == 0 {
        String::from("<|marker-0|>")
    } else {
        format!("<|marker{delta}|>")
    }
}

mod marker_offsets;
use marker_offsets::cursor_block_index;
#[cfg(test)]
use marker_offsets::{V0316_MIN_BLOCK_LINES, grow_v0327_candidate_range};
pub use marker_offsets::{
    compute_marker_offsets, compute_marker_offsets_v0318, compute_marker_offsets_v0618,
    compute_v0327_editable_range, is_good_block_start,
};

/// Write the editable region content with marker tags, inserting the cursor
/// marker at the given offset within the editable text.
pub fn write_editable_with_markers(
    output: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
    cursor_marker: &str,
) {
    let marker_offsets = compute_marker_offsets(editable_text);
    let mut cursor_placed = false;
    for (i, &offset) in marker_offsets.iter().enumerate() {
        let marker_num = i + 1;
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&marker_tag(marker_num));

        if let Some(&next_offset) = marker_offsets.get(i + 1) {
            output.push('\n');
            let block = &editable_text[offset..next_offset];
            if !cursor_placed
                && cursor_offset_in_editable >= offset
                && cursor_offset_in_editable <= next_offset
            {
                cursor_placed = true;
                let cursor_in_block = cursor_offset_in_editable - offset;
                output.push_str(&block[..cursor_in_block]);
                output.push_str(cursor_marker);
                output.push_str(&block[cursor_in_block..]);
            } else {
                output.push_str(block);
            }
        }
    }
}

/// Strip any `<|marker_N|>` tags from `text`.
///
/// When a marker tag sits on its own line (followed by `\n`), the trailing
/// newline is also removed so the surrounding lines stay joined naturally.
pub(crate) fn strip_marker_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pos = 0;
    let bytes = text.as_bytes();
    while let Some(rel) = text[pos..].find(MARKER_TAG_PREFIX) {
        result.push_str(&text[pos..pos + rel]);
        let num_start = pos + rel + MARKER_TAG_PREFIX.len();
        if let Some(suffix_rel) = text[num_start..].find(MARKER_TAG_SUFFIX) {
            let mut tag_end = num_start + suffix_rel + MARKER_TAG_SUFFIX.len();
            if bytes.get(tag_end) == Some(&b'\n') {
                tag_end += 1;
            }
            pos = tag_end;
        } else {
            result.push_str(MARKER_TAG_PREFIX);
            pos = num_start;
        }
    }
    result.push_str(&text[pos..]);
    result
}

/// Parse model output that uses the marker format.
///
/// Returns `(start_marker_num, end_marker_num, content_between_markers)`.
/// The leading format-level newline after the start marker is stripped.
/// Trailing newlines are preserved so blank-line endings in the editable
/// region are not lost.
///
/// Any extra intermediate marker tags that the model may have inserted
/// between the first and last markers are stripped from the returned content.
pub fn extract_marker_span(text: &str) -> Result<(usize, usize, String)> {
    let first_tag_start = text
        .find(MARKER_TAG_PREFIX)
        .context("no start marker found in output")?;
    let first_num_start = first_tag_start + MARKER_TAG_PREFIX.len();
    let first_num_end = text[first_num_start..]
        .find(MARKER_TAG_SUFFIX)
        .map(|i| i + first_num_start)
        .context("malformed start marker tag")?;
    let start_num: usize = text[first_num_start..first_num_end]
        .parse()
        .context("start marker number is not a valid integer")?;
    let first_tag_end = first_num_end + MARKER_TAG_SUFFIX.len();

    let last_tag_start = text
        .rfind(MARKER_TAG_PREFIX)
        .context("no end marker found in output")?;
    let last_num_start = last_tag_start + MARKER_TAG_PREFIX.len();
    let last_num_end = text[last_num_start..]
        .find(MARKER_TAG_SUFFIX)
        .map(|i| i + last_num_start)
        .context("malformed end marker tag")?;
    let end_num: usize = text[last_num_start..last_num_end]
        .parse()
        .context("end marker number is not a valid integer")?;

    if start_num == end_num {
        return Err(anyhow!(
            "start and end markers are the same (marker {})",
            start_num
        ));
    }

    let mut content_start = first_tag_end;
    if text.as_bytes().get(content_start) == Some(&b'\n') {
        content_start += 1;
    }
    let content_end = last_tag_start;

    let content = &text[content_start..content_end.max(content_start)];
    let content = strip_marker_tags(content);
    Ok((start_num, end_num, content))
}

/// Given old editable text and model output with marker span, reconstruct the
/// full new editable region.
pub fn apply_marker_span(old_editable: &str, output: &str) -> Result<String> {
    let (start_num, end_num, raw_new_span) = extract_marker_span(output)?;
    let marker_offsets = compute_marker_offsets(old_editable);

    let start_idx = start_num
        .checked_sub(1)
        .context("marker numbers are 1-indexed")?;
    let end_idx = end_num
        .checked_sub(1)
        .context("marker numbers are 1-indexed")?;
    let start_byte = *marker_offsets
        .get(start_idx)
        .context("start marker number out of range")?;
    let end_byte = *marker_offsets
        .get(end_idx)
        .context("end marker number out of range")?;

    if start_byte > end_byte {
        return Err(anyhow!("start marker must come before end marker"));
    }

    let old_span = &old_editable[start_byte..end_byte];
    let mut new_span = raw_new_span;
    if old_span.ends_with('\n') && !new_span.ends_with('\n') && !new_span.is_empty() {
        new_span.push('\n');
    }
    if !old_span.ends_with('\n') && new_span.ends_with('\n') {
        new_span.pop();
    }

    let mut result = String::new();
    result.push_str(&old_editable[..start_byte]);
    result.push_str(&new_span);
    result.push_str(&old_editable[end_byte..]);

    Ok(result)
}

/// Compare old and new editable text, find the minimal marker span that covers
/// all changes, and encode the result with marker tags.
pub fn encode_from_old_and_new(
    old_editable: &str,
    new_editable: &str,
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
    end_marker: &str,
    no_edits_marker: &str,
) -> Result<String> {
    if old_editable == new_editable {
        return Ok(format!("{no_edits_marker}{end_marker}"));
    }

    let marker_offsets = compute_marker_offsets(old_editable);
    let (common_prefix, common_suffix) =
        common_prefix_suffix(old_editable.as_bytes(), new_editable.as_bytes());
    let change_end_in_old = old_editable.len() - common_suffix;

    let start_marker_idx = marker_offsets
        .iter()
        .rposition(|&offset| offset <= common_prefix)
        .unwrap_or(0);
    let end_marker_idx = marker_offsets
        .iter()
        .position(|&offset| offset >= change_end_in_old)
        .unwrap_or(marker_offsets.len() - 1);

    let old_start = marker_offsets[start_marker_idx];
    let old_end = marker_offsets[end_marker_idx];

    let new_start = old_start;
    let new_end = new_editable
        .len()
        .saturating_sub(old_editable.len().saturating_sub(old_end));

    let new_span = &new_editable[new_start..new_end];

    let start_marker_num = start_marker_idx + 1;
    let end_marker_num = end_marker_idx + 1;

    let mut result = String::new();
    result.push_str(&marker_tag(start_marker_num));
    result.push('\n');

    if let Some(cursor_offset) = cursor_offset_in_new {
        if cursor_offset >= new_start && cursor_offset <= new_end {
            let cursor_in_span = cursor_offset - new_start;
            let bounded = cursor_in_span.min(new_span.len());
            result.push_str(&new_span[..bounded]);
            result.push_str(cursor_marker);
            result.push_str(&new_span[bounded..]);
        } else {
            result.push_str(new_span);
        }
    } else {
        result.push_str(new_span);
    }

    if !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&marker_tag(end_marker_num));
    result.push('\n');
    result.push_str(end_marker);

    Ok(result)
}

/// Extract the full editable region from text that uses marker tags.
///
/// Returns the concatenation of all block contents between the first and last
/// markers, with intermediate marker tags stripped.
pub fn extract_editable_region_from_markers(text: &str) -> Option<String> {
    let first_marker_start = text.find(MARKER_TAG_PREFIX)?;

    let mut markers: Vec<(usize, usize)> = Vec::new();
    let mut search_start = first_marker_start;
    while let Some(rel_pos) = text[search_start..].find(MARKER_TAG_PREFIX) {
        let tag_start = search_start + rel_pos;
        let num_start = tag_start + MARKER_TAG_PREFIX.len();
        let num_end = text[num_start..].find(MARKER_TAG_SUFFIX)?;
        let tag_end = num_start + num_end + MARKER_TAG_SUFFIX.len();
        markers.push((tag_start, tag_end));
        search_start = tag_end;
    }

    if markers.len() < 2 {
        return None;
    }

    let (_, first_tag_end) = markers[0];
    let (last_tag_start, _) = markers[markers.len() - 1];

    let mut content_start = first_tag_end;
    if text.as_bytes().get(content_start) == Some(&b'\n') {
        content_start += 1;
    }
    let mut content_end = last_tag_start;
    if content_end > content_start && text.as_bytes().get(content_end - 1) == Some(&b'\n') {
        content_end -= 1;
    }

    let raw = &text[content_start..content_end];
    let result = strip_marker_tags(raw);
    let result = result.strip_suffix('\n').unwrap_or(&result).to_string();
    Some(result)
}

mod byte_exact;
#[cfg(test)]
use byte_exact::collect_relative_marker_tags;
pub use byte_exact::{
    apply_marker_span_v0316, apply_marker_span_v0317, apply_marker_span_v0318,
    encode_from_old_and_new_v0316, encode_from_old_and_new_v0317, encode_from_old_and_new_v0318,
    nearest_marker_number, write_editable_with_markers_v0316, write_editable_with_markers_v0317,
    write_editable_with_markers_v0318,
};

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

/// Map a byte offset from old span coordinates to new span coordinates,
/// using common prefix/suffix within the span for accuracy.
#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    mod byte_exact;
    mod legacy_spans;
    mod marker_offsets;
}
