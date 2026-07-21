pub(crate) const V0316_MIN_BLOCK_LINES: usize = 3;
const V0316_MAX_BLOCK_LINES: usize = 8;
const V0318_MIN_BLOCK_LINES: usize = 6;
const V0318_MAX_BLOCK_LINES: usize = 16;
const V0618_MIN_BLOCK_LINES: usize = 3;
const V0618_MAX_BLOCK_LINES: usize = 32;
const MAX_NUDGE_LINES: usize = 5;

struct LineInfo {
    start: usize,
    is_blank: bool,
    is_good_start: bool,
}

fn collect_line_info(text: &str) -> Vec<LineInfo> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let trimmed = line.trim();
        let is_blank = trimmed.is_empty();
        let is_good_start = !is_blank && !is_structural_tail(trimmed);
        lines.push(LineInfo {
            start: offset,
            is_blank,
            is_good_start,
        });
        offset += line.len() + 1;
    }
    // split('\n') on "abc\n" yields ["abc", ""] — drop the phantom trailing
    // empty element when the text ends with '\n'.
    if text.ends_with('\n') && lines.len() > 1 {
        lines.pop();
    }
    lines
}

/// Whether a trimmed line is a model-friendly place to start a block: it has
/// content and isn't a structural tail. Exposed for reuse by context
/// retrieval when snapping excerpt boundaries to block boundaries.
pub fn is_good_block_start(trimmed_line: &str) -> bool {
    !trimmed_line.is_empty() && !is_structural_tail(trimmed_line)
}

fn is_structural_tail(trimmed_line: &str) -> bool {
    if trimmed_line.starts_with(&['}', ']', ')']) {
        return true;
    }
    matches!(
        trimmed_line.trim_end_matches(';'),
        "break" | "continue" | "return" | "throw" | "end"
    )
}

/// Starting from line `from`, scan up to `MAX_NUDGE_LINES` forward to find a
/// line with `is_good_start`. Returns `None` if no suitable line is found.
fn skip_to_good_start(lines: &[LineInfo], from: usize) -> Option<usize> {
    (from..lines.len().min(from + MAX_NUDGE_LINES)).find(|&i| lines[i].is_good_start)
}

/// Compute byte offsets within `editable_text` where marker boundaries should
/// be placed.
///
/// Returns a sorted `Vec<usize>` that always starts with `0` and ends with
/// `editable_text.len()`. Interior offsets are placed at line boundaries
/// (right after a `\n`), preferring blank-line boundaries when available and
/// respecting `min_block_lines` / `max_block_lines` constraints.
fn compute_marker_offsets_with_limits(
    editable_text: &str,
    min_block_lines: usize,
    max_block_lines: usize,
) -> Vec<usize> {
    if editable_text.is_empty() {
        return vec![0, 0];
    }

    let lines = collect_line_info(editable_text);
    let mut offsets = vec![0usize];
    let mut last_boundary_line = 0;
    let mut i = 0;

    while i < lines.len() {
        let gap = i - last_boundary_line;

        // Blank-line split: non-blank line following blank line(s) with enough
        // accumulated lines.
        if gap >= min_block_lines && !lines[i].is_blank && i > 0 && lines[i - 1].is_blank {
            let target = if lines[i].is_good_start {
                i
            } else {
                skip_to_good_start(&lines, i).unwrap_or(i)
            };
            if lines.len() - target >= min_block_lines
                && lines[target].start > *offsets.last().unwrap_or(&0)
            {
                offsets.push(lines[target].start);
                last_boundary_line = target;
                i = target + 1;
                continue;
            }
        }

        // Hard cap: too many lines without a split.
        if gap >= max_block_lines {
            let target = skip_to_good_start(&lines, i).unwrap_or(i);
            if lines[target].start > *offsets.last().unwrap_or(&0) {
                offsets.push(lines[target].start);
                last_boundary_line = target;
                i = target + 1;
                continue;
            }
        }

        i += 1;
    }

    let end = editable_text.len();
    if *offsets.last().unwrap_or(&0) != end {
        offsets.push(end);
    }

    offsets
}

/// Compute byte offsets within `editable_text` for the V0316/V0317 block sizing rules.
pub fn compute_marker_offsets(editable_text: &str) -> Vec<usize> {
    compute_marker_offsets_with_limits(editable_text, V0316_MIN_BLOCK_LINES, V0316_MAX_BLOCK_LINES)
}

pub fn compute_marker_offsets_v0318(editable_text: &str) -> Vec<usize> {
    compute_marker_offsets_with_limits(editable_text, V0318_MIN_BLOCK_LINES, V0318_MAX_BLOCK_LINES)
}

pub fn compute_marker_offsets_v0618(editable_text: &str) -> Vec<usize> {
    compute_marker_offsets_with_limits(editable_text, V0618_MIN_BLOCK_LINES, V0618_MAX_BLOCK_LINES)
}

fn line_start_at_or_before(text: &str, offset: usize) -> usize {
    let bounded_offset = text.floor_char_boundary(offset.min(text.len()));
    text[..bounded_offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end_at_or_after(text: &str, offset: usize) -> usize {
    let bounded_offset = text.floor_char_boundary(offset.min(text.len()));
    if bounded_offset >= text.len() {
        return text.len();
    }

    text[bounded_offset..]
        .find('\n')
        .map(|index| bounded_offset + index + 1)
        .unwrap_or(text.len())
}

pub(crate) fn grow_v0327_candidate_range(
    text: &str,
    cursor_offset: usize,
    editable_token_limit: usize,
) -> std::ops::Range<usize> {
    if text.is_empty() {
        return 0..0;
    }

    let byte_budget = editable_token_limit.saturating_mul(3).max(1);
    let half_budget = byte_budget / 2;

    let mut start = cursor_offset.saturating_sub(half_budget);
    let mut end = start.saturating_add(byte_budget).min(text.len());

    if end.saturating_sub(start) < byte_budget {
        start = end.saturating_sub(byte_budget);
    }

    start = line_start_at_or_before(text, start);
    end = line_end_at_or_after(text, end);

    if start < end {
        start..end
    } else {
        let line_start = line_start_at_or_before(text, cursor_offset);
        let line_end = line_end_at_or_after(text, cursor_offset);
        line_start..line_end.max(line_start)
    }
}

fn trim_v0327_candidate_range_to_markers(
    text: &str,
    candidate_range: std::ops::Range<usize>,
    cursor_offset: usize,
) -> std::ops::Range<usize> {
    let candidate_text = &text[candidate_range.clone()];
    let marker_offsets = compute_marker_offsets_v0318(candidate_text);

    if marker_offsets.len() <= 2 {
        return candidate_range;
    }

    let candidate_cursor_offset = cursor_offset
        .saturating_sub(candidate_range.start)
        .min(candidate_text.len());
    let first_internal_marker_index = if candidate_cursor_offset >= marker_offsets[1] {
        1
    } else {
        0
    };
    let last_internal_marker_index = marker_offsets.len() - 2;
    let last_marker_index = marker_offsets.len() - 1;
    let end_marker_index = if candidate_cursor_offset <= marker_offsets[last_internal_marker_index]
    {
        last_internal_marker_index
    } else {
        last_marker_index
    };

    let trimmed_start = candidate_range.start + marker_offsets[first_internal_marker_index];
    let trimmed_end = candidate_range.start + marker_offsets[end_marker_index];

    if trimmed_start < trimmed_end {
        trimmed_start..trimmed_end
    } else {
        let block_index = cursor_block_index(Some(candidate_cursor_offset), &marker_offsets);
        let start = candidate_range.start + marker_offsets[block_index];
        let end = candidate_range.start + marker_offsets[block_index + 1];
        if start < end {
            start..end
        } else {
            candidate_range
        }
    }
}

pub fn compute_v0327_editable_range(
    text: &str,
    cursor_offset: usize,
    editable_token_limit: usize,
) -> std::ops::Range<usize> {
    let candidate_range = grow_v0327_candidate_range(text, cursor_offset, editable_token_limit);
    trim_v0327_candidate_range_to_markers(text, candidate_range, cursor_offset)
}

pub(crate) fn cursor_block_index(cursor_offset: Option<usize>, marker_offsets: &[usize]) -> usize {
    let cursor = cursor_offset.unwrap_or(0);
    marker_offsets
        .windows(2)
        .position(|window| cursor >= window[0] && cursor < window[1])
        .unwrap_or_else(|| marker_offsets.len().saturating_sub(2))
}
