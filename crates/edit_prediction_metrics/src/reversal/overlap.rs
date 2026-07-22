use std::ops::Range;

use zeta_prompt::udiff::apply_diff_to_string;

use super::diff::{char_diff, text_diff};
use super::hunks::{HunkLine, filter_diff_hunks_by_excerpt, parse_diff_hunks, reverse_diff};

pub(super) fn compute_excerpt_aware_reversal_overlap(
    edit_history_diffs: &[&str],
    excerpt_content: &str,
    excerpt_start_row: u32,
    predicted_content: &str,
) -> ReversalOverlap {
    let mut current_content = excerpt_content.to_string();
    let mut current_excerpt_start_row = excerpt_start_row;

    for diff in edit_history_diffs.iter().rev() {
        if diff.is_empty() {
            continue;
        }

        let current_row_count = current_content.lines().count() as u32;
        let (filtered_diff, _line_offset) =
            filter_diff_hunks_by_excerpt(diff, current_excerpt_start_row, current_row_count.max(1));

        if filtered_diff.is_empty() {
            let hunks = parse_diff_hunks(diff);
            for hunk in hunks {
                let hunk_end = hunk.new_start.saturating_sub(1) + hunk.new_count;
                if hunk_end <= current_excerpt_start_row {
                    let additions: u32 = hunk
                        .lines
                        .iter()
                        .filter(|l| matches!(l, HunkLine::Addition(_)))
                        .count() as u32;
                    let deletions: u32 = hunk
                        .lines
                        .iter()
                        .filter(|l| matches!(l, HunkLine::Deletion(_)))
                        .count() as u32;
                    if additions >= deletions {
                        current_excerpt_start_row =
                            current_excerpt_start_row.saturating_sub(additions - deletions);
                    } else {
                        current_excerpt_start_row += deletions - additions;
                    }
                }
            }
            continue;
        }

        let reversed = reverse_diff(&format!("--- a/file\n+++ b/file\n{}", filtered_diff));
        match apply_diff_to_string(&reversed, &current_content) {
            Ok(updated) => {
                current_content = updated;
            }
            Err(_) => {
                continue;
            }
        }

        let hunks = parse_diff_hunks(diff);
        for hunk in hunks {
            let hunk_end = hunk.new_start.saturating_sub(1) + hunk.new_count;
            if hunk_end <= current_excerpt_start_row {
                let additions: u32 = hunk
                    .lines
                    .iter()
                    .filter(|l| matches!(l, HunkLine::Addition(_)))
                    .count() as u32;
                let deletions: u32 = hunk
                    .lines
                    .iter()
                    .filter(|l| matches!(l, HunkLine::Deletion(_)))
                    .count() as u32;
                if additions >= deletions {
                    current_excerpt_start_row =
                        current_excerpt_start_row.saturating_sub(additions - deletions);
                } else {
                    current_excerpt_start_row += deletions - additions;
                }
            }
        }
    }

    compute_reversal_overlap(&current_content, excerpt_content, predicted_content)
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct GranularEdit {
    range: Range<usize>,
    old_text: String,
    new_text: String,
}

fn compute_granular_edits(old_text: &str, new_text: &str) -> Vec<GranularEdit> {
    text_diff(old_text, new_text)
        .into_iter()
        .map(|(range, new_text)| GranularEdit {
            old_text: old_text[range.clone()].to_string(),
            range,
            new_text: new_text.to_string(),
        })
        .collect()
}

#[derive(Debug, Clone)]
struct HistoryAdditionRange {
    range_in_current: Range<usize>,
}

#[derive(Debug, Clone)]
struct HistoryDeletionRange {
    deleted_text: String,
    position_in_current: usize,
}

fn compute_history_addition_ranges(history_edits: &[GranularEdit]) -> Vec<HistoryAdditionRange> {
    let mut result = Vec::new();
    let mut offset_delta: isize = 0;

    for edit in history_edits {
        if !edit.new_text.is_empty() {
            let new_start = (edit.range.start as isize + offset_delta) as usize;
            let new_end = new_start + edit.new_text.len();
            result.push(HistoryAdditionRange {
                range_in_current: new_start..new_end,
            });
        }

        offset_delta += edit.new_text.len() as isize - edit.old_text.len() as isize;
    }

    result
}

fn compute_history_deletion_ranges(history_edits: &[GranularEdit]) -> Vec<HistoryDeletionRange> {
    let mut result = Vec::new();
    let mut offset_delta: isize = 0;

    for edit in history_edits {
        if !edit.old_text.is_empty() {
            let position_in_current = (edit.range.start as isize + offset_delta) as usize;
            result.push(HistoryDeletionRange {
                deleted_text: edit.old_text.clone(),
                position_in_current,
            });
        }

        offset_delta += edit.new_text.len() as isize - edit.old_text.len() as isize;
    }

    result
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ReversalOverlap {
    pub(super) chars_reversing_user_edits: usize,
    pub(super) total_chars_in_prediction: usize,
}

impl ReversalOverlap {
    pub(super) fn ratio(&self) -> f32 {
        if self.total_chars_in_prediction == 0 {
            0.0
        } else {
            self.chars_reversing_user_edits as f32 / self.total_chars_in_prediction as f32
        }
    }
}

/// Normalize edits where `old_text` appears as a subsequence within `new_text` (extension),
/// or where `new_text` appears as a subsequence within `old_text` (reduction).
///
/// For extensions: when the user's text is preserved (in order) within the prediction,
/// we only count the newly inserted characters, not the preserved ones.
/// E.g., "epr" → "eprintln!()" becomes 8 inserted chars ("intln!()")
/// E.g., "test_my_function" → "a_test_for_my_special_function_plz" becomes 18 inserted chars
///
/// For reductions: when the prediction's text is preserved (in order) within the original,
/// we only count the deleted characters, not the preserved ones.
/// E.g., "ifrom" → "from" becomes 1 deleted char ("i")
fn normalize_extension_edits(edits: Vec<GranularEdit>) -> Vec<GranularEdit> {
    edits
        .into_iter()
        .flat_map(|edit| {
            if edit.old_text.is_empty() || edit.new_text.is_empty() {
                return vec![edit];
            }

            // Use character-wise diff to find exact byte ranges of changes
            let char_edits = char_diff(&edit.old_text, &edit.new_text);

            let all_deletions = !char_edits.is_empty()
                && char_edits
                    .iter()
                    .all(|(range, replacement)| !range.is_empty() && replacement.is_empty());
            let all_insertions = !char_edits.is_empty()
                && char_edits
                    .iter()
                    .all(|(range, replacement)| range.is_empty() && !replacement.is_empty());
            if all_deletions || all_insertions {
                return char_edits
                    .into_iter()
                    .map(|(range, replacement)| GranularEdit {
                        range: edit.range.start + range.start..edit.range.start + range.end,
                        old_text: edit.old_text[range].to_string(),
                        new_text: replacement.to_string(),
                    })
                    .collect();
            }

            // Otherwise, keep the original edit (mixed changes)
            vec![edit]
        })
        .collect()
}

pub(super) fn compute_reversal_overlap(
    original_content: &str,
    current_content: &str,
    predicted_content: &str,
) -> ReversalOverlap {
    let history_edits =
        normalize_extension_edits(compute_granular_edits(original_content, current_content));
    let prediction_edits =
        normalize_extension_edits(compute_granular_edits(current_content, predicted_content));

    let history_addition_ranges = compute_history_addition_ranges(&history_edits);
    let history_deletion_ranges = compute_history_deletion_ranges(&history_edits);

    let reversed_additions =
        compute_reversed_additions(&history_addition_ranges, &prediction_edits);
    let restored_deletions =
        compute_restored_deletions(&history_deletion_ranges, &prediction_edits);

    let total_chars_in_prediction: usize = prediction_edits
        .iter()
        .map(|e| e.new_text.chars().count() + e.old_text.chars().count())
        .sum();

    ReversalOverlap {
        chars_reversing_user_edits: reversed_additions + restored_deletions,
        total_chars_in_prediction,
    }
}

fn compute_reversed_additions(
    history_addition_ranges: &[HistoryAdditionRange],
    prediction_edits: &[GranularEdit],
) -> usize {
    let mut reversed_chars = 0;

    for pred_edit in prediction_edits {
        for history_addition in history_addition_ranges {
            let overlap_start = pred_edit
                .range
                .start
                .max(history_addition.range_in_current.start);
            let overlap_end = pred_edit
                .range
                .end
                .min(history_addition.range_in_current.end);

            if overlap_start < overlap_end {
                let relative_start = overlap_start - pred_edit.range.start;
                let relative_end = overlap_end - pred_edit.range.start;
                let overlap_text = &pred_edit.old_text[relative_start..relative_end];
                reversed_chars += overlap_text.chars().count();
            }
        }
    }

    reversed_chars
}

fn compute_restored_deletions(
    history_deletion_ranges: &[HistoryDeletionRange],
    prediction_edits: &[GranularEdit],
) -> usize {
    let mut restored = 0;

    for pred_edit in prediction_edits {
        if pred_edit.new_text.is_empty() {
            continue;
        }

        for deletion in history_deletion_ranges {
            if pred_edit.range.contains(&deletion.position_in_current)
                || deletion.position_in_current == pred_edit.range.start
            {
                restored += compute_lcs_length(&deletion.deleted_text, &pred_edit.new_text);
            }
        }
    }

    restored
}

pub(super) fn compute_lcs_length(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 || n == 0 {
        return 0;
    }

    let mut prev = vec![0; n + 1];
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_chars[i - 1] == b_chars[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[n]
}
