use anyhow::{Context as _, Result, anyhow};

use super::*;

pub fn find_context_candidates(text: &str, hunk: &mut Hunk) -> Vec<usize> {
    let candidates: Vec<usize> = text
        .match_indices(&hunk.context)
        .map(|(offset, _)| offset)
        .collect();

    if !candidates.is_empty() {
        return candidates;
    }

    if hunk.context.ends_with('\n') && !hunk.context.is_empty() {
        let old_len = hunk.context.len();
        hunk.context.pop();
        let new_len = hunk.context.len();

        if !hunk.context.is_empty() {
            let candidates: Vec<usize> = text
                .match_indices(&hunk.context)
                .filter(|(offset, _)| offset + new_len == text.len())
                .map(|(offset, _)| offset)
                .collect();

            if !candidates.is_empty() {
                for edit in &mut hunk.edits {
                    let touched_phantom = edit.range.end > new_len;
                    edit.range.start = edit.range.start.min(new_len);
                    edit.range.end = edit.range.end.min(new_len);
                    if touched_phantom {
                        // The replacement text was also written with a
                        // trailing '\n' that corresponds to the phantom
                        // newline we just removed from the context.
                        if edit.text.ends_with('\n') {
                            edit.text.pop();
                        }
                    }
                }
                return candidates;
            }

            // Restore if fallback didn't help either.
            hunk.context.push('\n');
            debug_assert_eq!(hunk.context.len(), old_len);
        } else {
            hunk.context.push('\n');
        }
    }

    Vec::new()
}

/// Given multiple candidate offsets where context matches, use line numbers to disambiguate.
/// Returns the offset that matches the expected line, or None if no match or no line number available.
pub fn disambiguate_by_line_number(
    candidates: &[usize],
    expected_line: Option<u32>,
    offset_to_line: &dyn Fn(usize) -> u32,
) -> Option<usize> {
    match candidates.len() {
        0 => None,
        1 => Some(candidates[0]),
        _ => {
            let expected = expected_line?;
            candidates
                .iter()
                .copied()
                .find(|&offset| offset_to_line(offset) == expected)
        }
    }
}

pub fn apply_diff_to_string(diff_str: &str, text: &str) -> Result<String> {
    apply_diff_to_string_with_hunk_offset(diff_str, text).map(|(text, _)| text)
}

/// Applies a diff to a string and returns the result along with the offset where
/// the first hunk's context matched in the original text. This offset can be used
/// to adjust cursor positions that are relative to the hunk's content.
pub fn apply_diff_to_string_with_hunk_offset(
    diff_str: &str,
    text: &str,
) -> Result<(String, Option<usize>)> {
    let mut diff = DiffParser::new(diff_str);

    let mut text = text.to_string();
    let mut first_hunk_offset = None;
    let mut line_delta = 0i64;

    while let Some(event) = diff.next().context("Failed to parse diff")? {
        match event {
            DiffEvent::Hunk {
                mut hunk,
                path: _,
                status: _,
            } => {
                let candidates = find_context_candidates(&text, &mut hunk);
                let adjusted_start_line = hunk
                    .start_line
                    .and_then(|start_line| u32::try_from(start_line as i64 + line_delta).ok());

                let hunk_offset =
                    disambiguate_by_line_number(&candidates, adjusted_start_line, &|offset| {
                        text[..offset].matches('\n').count() as u32
                    })
                    .ok_or_else(|| anyhow!("couldn't resolve hunk"))?;

                if first_hunk_offset.is_none() {
                    first_hunk_offset = Some(hunk_offset);
                }

                let mut hunk_line_delta = 0i64;
                for edit in hunk.edits.iter().rev() {
                    let range = (hunk_offset + edit.range.start)..(hunk_offset + edit.range.end);
                    let deleted_lines = text[range.clone()].matches('\n').count() as i64;
                    let inserted_lines = edit.text.matches('\n').count() as i64;
                    text.replace_range(range, &edit.text);
                    hunk_line_delta += inserted_lines - deleted_lines;
                }
                line_delta += hunk_line_delta;
            }
            DiffEvent::FileEnd { .. } => {
                line_delta = 0;
            }
        }
    }

    Ok((text, first_hunk_offset))
}
