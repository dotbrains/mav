use language::{BufferSnapshot, Point, ToPoint as _};
use std::ops::Range;
use text::OffsetRangeExt as _;

const CURSOR_EXCERPT_TOKEN_BUDGET: usize = 8192;

/// Computes a cursor excerpt as the largest linewise symmetric region around
/// the cursor that fits within an 8192-token budget. Returns the point range,
/// byte offset range, and the cursor offset relative to the excerpt start.
pub fn compute_cursor_excerpt(
    snapshot: &BufferSnapshot,
    cursor_offset: usize,
) -> (Range<Point>, Range<usize>, usize) {
    let cursor_point = cursor_offset.to_point(snapshot);
    let cursor_row = cursor_point.row;
    let (start_row, end_row, _) =
        expand_symmetric_from_cursor(snapshot, cursor_row, CURSOR_EXCERPT_TOKEN_BUDGET);

    let excerpt_range = Point::new(start_row, 0)..Point::new(end_row, snapshot.line_len(end_row));
    let excerpt_offset_range = excerpt_range.to_offset(snapshot);
    let cursor_offset_in_excerpt = cursor_offset - excerpt_offset_range.start;

    (
        excerpt_range,
        excerpt_offset_range,
        cursor_offset_in_excerpt,
    )
}

/// Expands symmetrically from cursor, one line at a time, alternating down then up.
/// Returns (start_row, end_row, remaining_tokens).
fn expand_symmetric_from_cursor(
    snapshot: &BufferSnapshot,
    cursor_row: u32,
    mut token_budget: usize,
) -> (u32, u32, usize) {
    let mut start_row = cursor_row;
    let mut end_row = cursor_row;

    let cursor_line_tokens = line_token_count(snapshot, cursor_row);
    token_budget = token_budget.saturating_sub(cursor_line_tokens);

    loop {
        let can_expand_up = start_row > 0;
        let can_expand_down = end_row < snapshot.max_point().row;

        if token_budget == 0 || (!can_expand_up && !can_expand_down) {
            break;
        }

        if can_expand_down {
            let next_row = end_row + 1;
            let line_tokens = line_token_count(snapshot, next_row);
            if line_tokens <= token_budget {
                end_row = next_row;
                token_budget = token_budget.saturating_sub(line_tokens);
            } else {
                break;
            }
        }

        if can_expand_up && token_budget > 0 {
            let next_row = start_row - 1;
            let line_tokens = line_token_count(snapshot, next_row);
            if line_tokens <= token_budget {
                start_row = next_row;
                token_budget = token_budget.saturating_sub(line_tokens);
            } else {
                break;
            }
        }
    }

    (start_row, end_row, token_budget)
}

/// Typical number of string bytes per token for the purposes of limiting model input. This is
/// intentionally low to err on the side of underestimating limits.
pub(crate) const BYTES_PER_TOKEN_GUESS: usize = 3;

pub fn guess_token_count(bytes: usize) -> usize {
    bytes / BYTES_PER_TOKEN_GUESS
}

fn line_token_count(snapshot: &BufferSnapshot, row: u32) -> usize {
    guess_token_count(snapshot.line_len(row) as usize).max(1)
}

/// Computes the byte offset ranges of all syntax nodes containing the cursor,
/// ordered from innermost to outermost. The offsets are relative to
/// `excerpt_offset_range.start`.
pub fn compute_syntax_ranges(
    snapshot: &BufferSnapshot,
    cursor_offset: usize,
    excerpt_offset_range: &Range<usize>,
) -> Vec<Range<usize>> {
    let cursor_point = cursor_offset.to_point(snapshot);
    let range = cursor_point..cursor_point;
    let mut current = snapshot.syntax_ancestor(range);
    let mut ranges = Vec::new();
    let mut last_range: Option<(usize, usize)> = None;

    while let Some(node) = current.take() {
        let node_start = node.start_byte();
        let node_end = node.end_byte();
        let key = (node_start, node_end);

        current = node.parent();

        if last_range == Some(key) {
            continue;
        }
        last_range = Some(key);

        let start = node_start.saturating_sub(excerpt_offset_range.start);
        let end = node_end
            .min(excerpt_offset_range.end)
            .saturating_sub(excerpt_offset_range.start);
        ranges.push(start..end);
    }

    ranges
}

/// Expands context by first trying to reach syntax boundaries,
/// then expanding line-wise only if no syntax expansion occurred.
pub fn expand_context_syntactically_then_linewise(
    snapshot: &BufferSnapshot,
    editable_range: Range<Point>,
    context_token_limit: usize,
) -> Range<Point> {
    let mut start_row = editable_range.start.row;
    let mut end_row = editable_range.end.row;
    let mut remaining_tokens = context_token_limit;
    let mut did_syntax_expand = false;

    // Phase 1: Try to expand to containing syntax boundaries, picking the largest that fits.
    for (boundary_start, boundary_end) in containing_syntax_boundaries(snapshot, start_row, end_row)
    {
        let tokens_for_start = if boundary_start < start_row {
            estimate_tokens_for_rows(snapshot, boundary_start, start_row)
        } else {
            0
        };
        let tokens_for_end = if boundary_end > end_row {
            estimate_tokens_for_rows(snapshot, end_row + 1, boundary_end + 1)
        } else {
            0
        };

        let total_needed = tokens_for_start + tokens_for_end;

        if total_needed <= remaining_tokens {
            if boundary_start < start_row {
                start_row = boundary_start;
            }
            if boundary_end > end_row {
                end_row = boundary_end;
            }
            remaining_tokens = remaining_tokens.saturating_sub(total_needed);
            did_syntax_expand = true;
        } else {
            break;
        }
    }

    // Phase 2: Only expand line-wise if no syntax expansion occurred.
    if !did_syntax_expand {
        (start_row, end_row, _) =
            expand_linewise_biased(snapshot, start_row, end_row, remaining_tokens, true);
    }

    let start = Point::new(start_row, 0);
    let end = Point::new(end_row, snapshot.line_len(end_row));
    start..end
}

/// Returns an iterator of (start_row, end_row) for successively larger syntax nodes
/// containing the given row range. Smallest containing node first.
fn containing_syntax_boundaries(
    snapshot: &BufferSnapshot,
    start_row: u32,
    end_row: u32,
) -> impl Iterator<Item = (u32, u32)> {
    let range = Point::new(start_row, 0)..Point::new(end_row, snapshot.line_len(end_row));
    let mut current = snapshot.syntax_ancestor(range);
    let mut last_rows: Option<(u32, u32)> = None;

    std::iter::from_fn(move || {
        while let Some(node) = current.take() {
            let node_start_row = node.start_position().row as u32;
            let node_end_row = node.end_position().row as u32;
            let rows = (node_start_row, node_end_row);

            current = node.parent();

            // Skip nodes that don't extend beyond our range.
            if node_start_row >= start_row && node_end_row <= end_row {
                continue;
            }

            // Skip if same as last returned (some nodes have same span).
            if last_rows == Some(rows) {
                continue;
            }

            last_rows = Some(rows);
            return Some(rows);
        }
        None
    })
}

/// Expands line-wise with a bias toward one direction.
/// Returns (start_row, end_row, remaining_tokens).
fn expand_linewise_biased(
    snapshot: &BufferSnapshot,
    mut start_row: u32,
    mut end_row: u32,
    mut remaining_tokens: usize,
    prefer_up: bool,
) -> (u32, u32, usize) {
    loop {
        let can_expand_up = start_row > 0;
        let can_expand_down = end_row < snapshot.max_point().row;

        if remaining_tokens == 0 || (!can_expand_up && !can_expand_down) {
            break;
        }

        let mut expanded = false;

        // Try preferred direction first.
        if prefer_up {
            if can_expand_up {
                let next_row = start_row - 1;
                let line_tokens = line_token_count(snapshot, next_row);
                if line_tokens <= remaining_tokens {
                    start_row = next_row;
                    remaining_tokens = remaining_tokens.saturating_sub(line_tokens);
                    expanded = true;
                }
            }
            if can_expand_down && remaining_tokens > 0 {
                let next_row = end_row + 1;
                let line_tokens = line_token_count(snapshot, next_row);
                if line_tokens <= remaining_tokens {
                    end_row = next_row;
                    remaining_tokens = remaining_tokens.saturating_sub(line_tokens);
                    expanded = true;
                }
            }
        } else {
            if can_expand_down {
                let next_row = end_row + 1;
                let line_tokens = line_token_count(snapshot, next_row);
                if line_tokens <= remaining_tokens {
                    end_row = next_row;
                    remaining_tokens = remaining_tokens.saturating_sub(line_tokens);
                    expanded = true;
                }
            }
            if can_expand_up && remaining_tokens > 0 {
                let next_row = start_row - 1;
                let line_tokens = line_token_count(snapshot, next_row);
                if line_tokens <= remaining_tokens {
                    start_row = next_row;
                    remaining_tokens = remaining_tokens.saturating_sub(line_tokens);
                    expanded = true;
                }
            }
        }

        if !expanded {
            break;
        }
    }

    (start_row, end_row, remaining_tokens)
}

/// Estimates token count for rows in range [start_row, end_row).
fn estimate_tokens_for_rows(snapshot: &BufferSnapshot, start_row: u32, end_row: u32) -> usize {
    let mut tokens = 0;
    for row in start_row..end_row {
        tokens += line_token_count(snapshot, row);
    }
    tokens
}

#[cfg(test)]
mod tests;
