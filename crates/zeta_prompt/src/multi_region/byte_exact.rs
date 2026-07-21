use anyhow::{Result, anyhow};

use super::*;

pub(crate) struct ParsedTag {
    pub(crate) value: isize,
    pub(crate) tag_start: usize,
    pub(crate) tag_end: usize,
}

fn collect_tags(text: &str, prefix: &str, parse: fn(&str) -> Option<isize>) -> Vec<ParsedTag> {
    let mut tags = Vec::new();
    let mut search_from = 0;
    while let Some(rel_pos) = text[search_from..].find(prefix) {
        let tag_start = search_from + rel_pos;
        let payload_start = tag_start + prefix.len();
        if let Some(suffix_rel) = text[payload_start..].find(MARKER_TAG_SUFFIX) {
            let payload_end = payload_start + suffix_rel;
            if let Some(value) = parse(&text[payload_start..payload_end]) {
                let tag_end = payload_end + MARKER_TAG_SUFFIX.len();
                tags.push(ParsedTag {
                    value,
                    tag_start,
                    tag_end,
                });
                search_from = tag_end;
                continue;
            }
        }
        search_from = tag_start + prefix.len();
    }
    tags
}

pub(crate) fn collect_marker_tags(text: &str) -> Vec<ParsedTag> {
    collect_tags(text, MARKER_TAG_PREFIX, |s| {
        s.parse::<usize>().ok().map(|n| n as isize)
    })
}

pub(crate) fn collect_relative_marker_tags(text: &str) -> Vec<ParsedTag> {
    collect_tags(text, RELATIVE_MARKER_TAG_PREFIX, |s| {
        s.parse::<isize>().ok()
    })
}

pub fn nearest_marker_number(cursor_offset: Option<usize>, marker_offsets: &[usize]) -> usize {
    let cursor = cursor_offset.unwrap_or(0);
    marker_offsets
        .iter()
        .enumerate()
        .min_by_key(|(_, offset)| (**offset as isize - cursor as isize).unsigned_abs())
        .map(|(idx, _)| idx + 1)
        .unwrap_or(1)
}
fn map_boundary_offset(
    old_rel: usize,
    old_span_len: usize,
    new_span_len: usize,
    span_common_prefix: usize,
    span_common_suffix: usize,
) -> usize {
    if old_rel <= span_common_prefix {
        old_rel
    } else if old_rel >= old_span_len - span_common_suffix {
        new_span_len - (old_span_len - old_rel)
    } else {
        let old_changed_start = span_common_prefix;
        let old_changed_len = old_span_len
            .saturating_sub(span_common_prefix)
            .saturating_sub(span_common_suffix);
        let new_changed_start = span_common_prefix;
        let new_changed_len = new_span_len
            .saturating_sub(span_common_prefix)
            .saturating_sub(span_common_suffix);

        new_changed_start
            + ((old_rel - old_changed_start) * new_changed_len)
                .checked_div(old_changed_len)
                .unwrap_or(new_changed_len)
    }
}

fn snap_to_line_start(text: &str, offset: usize) -> usize {
    let bounded = offset.min(text.len());
    let bounded = text.floor_char_boundary(bounded);

    if bounded >= text.len() {
        return text.len();
    }

    if bounded == 0 || text.as_bytes().get(bounded - 1) == Some(&b'\n') {
        return bounded;
    }

    if let Some(next_nl_rel) = text[bounded..].find('\n') {
        let next = bounded + next_nl_rel + 1;
        return text.floor_char_boundary(next.min(text.len()));
    }

    let prev_start = text[..bounded].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    text.floor_char_boundary(prev_start)
}

/// Write the editable region content with byte-exact marker tags, inserting the
/// cursor marker at the given offset within the editable text.
///
/// The `tag_for_index` closure maps a boundary index to the marker tag string.
fn write_editable_with_markers_impl(
    output: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
    cursor_marker: &str,
    marker_offsets: &[usize],
    tag_for_index: impl Fn(usize) -> String,
) {
    let mut cursor_placed = false;
    for (i, &offset) in marker_offsets.iter().enumerate() {
        output.push_str(&tag_for_index(i));

        if let Some(&next_offset) = marker_offsets.get(i + 1) {
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

pub fn write_editable_with_markers_v0316(
    output: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
    cursor_marker: &str,
) {
    let marker_offsets = compute_marker_offsets(editable_text);
    write_editable_with_markers_impl(
        output,
        editable_text,
        cursor_offset_in_editable,
        cursor_marker,
        &marker_offsets,
        |i| marker_tag(i + 1),
    );
}

pub fn write_editable_with_markers_v0317(
    output: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
    cursor_marker: &str,
) {
    let marker_offsets = compute_marker_offsets(editable_text);
    let anchor_idx = cursor_block_index(Some(cursor_offset_in_editable), &marker_offsets);
    write_editable_with_markers_impl(
        output,
        editable_text,
        cursor_offset_in_editable,
        cursor_marker,
        &marker_offsets,
        |i| marker_tag_relative(i as isize - anchor_idx as isize),
    );
}

pub fn write_editable_with_markers_v0318(
    output: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
    cursor_marker: &str,
) {
    let marker_offsets = compute_marker_offsets_v0318(editable_text);
    write_editable_with_markers_impl(
        output,
        editable_text,
        cursor_offset_in_editable,
        cursor_marker,
        &marker_offsets,
        |i| marker_tag(i + 1),
    );
}

fn apply_marker_span_impl(
    old_editable: &str,
    tags: &[ParsedTag],
    output: &str,
    resolve_boundaries: impl Fn(isize, isize) -> Result<(usize, usize)>,
) -> Result<String> {
    if tags.is_empty() {
        return Err(anyhow!("no marker tags found in output"));
    }
    if tags.len() == 1 {
        return Err(anyhow!(
            "only one marker tag found in output, expected at least two"
        ));
    }

    let start_value = tags[0].value;
    let end_value = tags[tags.len() - 1].value;

    if start_value == end_value {
        return Ok(old_editable.to_string());
    }

    let (start_byte, end_byte) = resolve_boundaries(start_value, end_value)?;

    if start_byte > end_byte {
        return Err(anyhow!("start marker must come before end marker"));
    }

    let mut new_content = String::new();
    for i in 0..tags.len() - 1 {
        let content_start = tags[i].tag_end;
        let content_end = tags[i + 1].tag_start;
        if content_start <= content_end {
            new_content.push_str(&output[content_start..content_end]);
        }
    }

    let mut result = String::new();
    result.push_str(&old_editable[..start_byte]);
    result.push_str(&new_content);
    result.push_str(&old_editable[end_byte..]);

    Ok(result)
}

pub fn apply_marker_span_v0316(old_editable: &str, output: &str) -> Result<String> {
    let tags = collect_marker_tags(output);

    if tags.len() >= 2 {
        let start_num = tags[0].value;
        let end_num = tags[tags.len() - 1].value;
        if start_num != end_num {
            let expected: Vec<isize> = (start_num..=end_num).collect();
            let actual: Vec<isize> = tags.iter().map(|t| t.value).collect();
            if actual != expected {
                eprintln!(
                    "V0316 marker sequence validation failed: expected {:?}, got {:?}. Attempting best-effort parse.",
                    expected, actual
                );
            }
        }
    }

    let marker_offsets = compute_marker_offsets(old_editable);
    apply_marker_span_impl(old_editable, &tags, output, |start_val, end_val| {
        let start_idx = (start_val as usize)
            .checked_sub(1)
            .context("marker numbers are 1-indexed")?;
        let end_idx = (end_val as usize)
            .checked_sub(1)
            .context("marker numbers are 1-indexed")?;
        let start_byte = *marker_offsets
            .get(start_idx)
            .context("start marker number out of range")?;
        let end_byte = *marker_offsets
            .get(end_idx)
            .context("end marker number out of range")?;
        Ok((start_byte, end_byte))
    })
}

pub fn apply_marker_span_v0317(
    old_editable: &str,
    output: &str,
    cursor_offset_in_old: Option<usize>,
) -> Result<String> {
    let tags = collect_relative_marker_tags(output);
    let marker_offsets = compute_marker_offsets(old_editable);
    let anchor_idx = cursor_block_index(cursor_offset_in_old, &marker_offsets);

    apply_marker_span_impl(old_editable, &tags, output, |start_delta, end_delta| {
        let start_idx_signed = anchor_idx as isize + start_delta;
        let end_idx_signed = anchor_idx as isize + end_delta;
        if start_idx_signed < 0 || end_idx_signed < 0 {
            return Err(anyhow!("relative marker maps before first marker"));
        }
        let start_idx = usize::try_from(start_idx_signed).context("invalid start marker index")?;
        let end_idx = usize::try_from(end_idx_signed).context("invalid end marker index")?;
        let start_byte = *marker_offsets
            .get(start_idx)
            .context("start marker number out of range")?;
        let end_byte = *marker_offsets
            .get(end_idx)
            .context("end marker number out of range")?;
        Ok((start_byte, end_byte))
    })
}

pub fn apply_marker_span_v0318(old_editable: &str, output: &str) -> Result<String> {
    let tags = collect_marker_tags(output);

    if tags.len() >= 2 {
        let start_num = tags[0].value;
        let end_num = tags[tags.len() - 1].value;
        if start_num != end_num {
            let expected: Vec<isize> = (start_num..=end_num).collect();
            let actual: Vec<isize> = tags.iter().map(|t| t.value).collect();
            if actual != expected {
                eprintln!(
                    "V0318 marker sequence validation failed: expected {:?}, got {:?}. Attempting best-effort parse.",
                    expected, actual
                );
            }
        }
    }

    let marker_offsets = compute_marker_offsets_v0318(old_editable);
    apply_marker_span_impl(old_editable, &tags, output, |start_val, end_val| {
        let start_idx = (start_val as usize)
            .checked_sub(1)
            .context("marker numbers are 1-indexed")?;
        let end_idx = (end_val as usize)
            .checked_sub(1)
            .context("marker numbers are 1-indexed")?;
        let start_byte = *marker_offsets
            .get(start_idx)
            .context("start marker number out of range")?;
        let end_byte = *marker_offsets
            .get(end_idx)
            .context("end marker number out of range")?;
        Ok((start_byte, end_byte))
    })
}

fn encode_from_old_and_new_impl(
    old_editable: &str,
    new_editable: &str,
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
    end_marker: &str,
    no_edit_tag: &str,
    marker_offsets: &[usize],
    tag_for_block_idx: impl Fn(usize) -> String,
) -> Result<String> {
    if old_editable == new_editable {
        return Ok(format!("{no_edit_tag}{no_edit_tag}{end_marker}"));
    }

    let (common_prefix, common_suffix) =
        common_prefix_suffix(old_editable.as_bytes(), new_editable.as_bytes());
    let change_end_in_old = old_editable.len() - common_suffix;

    let mut start_marker_idx = marker_offsets
        .iter()
        .rposition(|&offset| offset <= common_prefix)
        .unwrap_or(0);
    let mut end_marker_idx = marker_offsets
        .iter()
        .position(|&offset| offset >= change_end_in_old)
        .unwrap_or(marker_offsets.len() - 1);

    if start_marker_idx == end_marker_idx {
        if end_marker_idx < marker_offsets.len().saturating_sub(1) {
            end_marker_idx += 1;
        } else if start_marker_idx > 0 {
            start_marker_idx -= 1;
        }
    }

    let old_start = marker_offsets[start_marker_idx];
    let old_end = marker_offsets[end_marker_idx];

    let new_start = old_start;
    let new_end = new_editable
        .len()
        .saturating_sub(old_editable.len().saturating_sub(old_end));

    let new_span = &new_editable[new_start..new_end];
    let old_span = &old_editable[old_start..old_end];

    let (span_common_prefix, span_common_suffix) =
        common_prefix_suffix(old_span.as_bytes(), new_span.as_bytes());

    let mut result = String::new();
    let mut prev_new_rel = 0usize;
    let mut cursor_placed = false;

    for block_idx in start_marker_idx..end_marker_idx {
        result.push_str(&tag_for_block_idx(block_idx));

        let new_rel_end = if block_idx + 1 == end_marker_idx {
            new_span.len()
        } else {
            let old_rel = marker_offsets[block_idx + 1] - old_start;
            let mapped = map_boundary_offset(
                old_rel,
                old_span.len(),
                new_span.len(),
                span_common_prefix,
                span_common_suffix,
            );
            snap_to_line_start(new_span, mapped)
        };

        let new_rel_end = new_rel_end.max(prev_new_rel);
        let block_content = &new_span[prev_new_rel..new_rel_end];

        if !cursor_placed {
            if let Some(cursor_offset) = cursor_offset_in_new {
                let abs_start = new_start + prev_new_rel;
                let abs_end = new_start + new_rel_end;
                if cursor_offset >= abs_start && cursor_offset <= abs_end {
                    cursor_placed = true;
                    let cursor_in_block = cursor_offset - abs_start;
                    let bounded = cursor_in_block.min(block_content.len());
                    result.push_str(&block_content[..bounded]);
                    result.push_str(cursor_marker);
                    result.push_str(&block_content[bounded..]);
                    prev_new_rel = new_rel_end;
                    continue;
                }
            }
        }

        result.push_str(block_content);
        prev_new_rel = new_rel_end;
    }

    result.push_str(&tag_for_block_idx(end_marker_idx));
    result.push_str(end_marker);

    Ok(result)
}

pub fn encode_from_old_and_new_v0316(
    old_editable: &str,
    new_editable: &str,
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
    end_marker: &str,
) -> Result<String> {
    let marker_offsets = compute_marker_offsets(old_editable);
    let no_edit_tag = marker_tag(nearest_marker_number(cursor_offset_in_new, &marker_offsets));
    encode_from_old_and_new_impl(
        old_editable,
        new_editable,
        cursor_offset_in_new,
        cursor_marker,
        end_marker,
        &no_edit_tag,
        &marker_offsets,
        |block_idx| marker_tag(block_idx + 1),
    )
}

pub fn encode_from_old_and_new_v0317(
    old_editable: &str,
    new_editable: &str,
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
    end_marker: &str,
) -> Result<String> {
    let marker_offsets = compute_marker_offsets(old_editable);
    let anchor_idx = cursor_block_index(cursor_offset_in_new, &marker_offsets);
    let no_edit_tag = marker_tag_relative(0);
    encode_from_old_and_new_impl(
        old_editable,
        new_editable,
        cursor_offset_in_new,
        cursor_marker,
        end_marker,
        &no_edit_tag,
        &marker_offsets,
        |block_idx| marker_tag_relative(block_idx as isize - anchor_idx as isize),
    )
}

pub fn encode_from_old_and_new_v0318(
    old_editable: &str,
    new_editable: &str,
    cursor_offset_in_new: Option<usize>,
    cursor_marker: &str,
    end_marker: &str,
) -> Result<String> {
    let marker_offsets = compute_marker_offsets_v0318(old_editable);
    let no_edit_tag = marker_tag(nearest_marker_number(cursor_offset_in_new, &marker_offsets));
    encode_from_old_and_new_impl(
        old_editable,
        new_editable,
        cursor_offset_in_new,
        cursor_marker,
        end_marker,
        &no_edit_tag,
        &marker_offsets,
        |block_idx| marker_tag(block_idx + 1),
    )
}
