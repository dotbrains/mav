mod diff;
mod history;
mod hunks;
mod overlap;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use history::{extract_diff_from_event, filter_edit_history_by_path, is_predicted_event};
use hunks::{apply_diff_to_string_lenient, reverse_diff};
use overlap::{compute_excerpt_aware_reversal_overlap, compute_reversal_overlap};
use zeta_prompt::udiff::apply_diff_to_string;

pub fn compute_prediction_reversal_ratio_from_history(
    current_content: &str,
    edit_history: &[Arc<zeta_prompt::Event>],
    excerpt_start_row: Option<u32>,
    predicted_content: &str,
    cursor_path: &Path,
) -> f32 {
    let relevant_events = filter_edit_history_by_path(edit_history, cursor_path);

    let most_recent = match relevant_events.last() {
        Some(event) if !is_predicted_event(event) => *event,
        _ => return 0.0,
    };

    let diff = extract_diff_from_event(most_recent);
    if diff.is_empty() {
        return 0.0;
    }

    if let Some(excerpt_start_row) = excerpt_start_row {
        let diffs = vec![diff];
        let overlap = compute_excerpt_aware_reversal_overlap(
            &diffs,
            current_content,
            excerpt_start_row,
            predicted_content,
        );
        return overlap.ratio();
    }

    let reversed = reverse_diff(diff);
    let with_headers = format!(
        "--- a/file
+++ b/file
{}",
        reversed
    );
    let original_content = match apply_diff_to_string(&with_headers, current_content) {
        Ok(updated_content) => updated_content,
        Err(_) => apply_diff_to_string_lenient(&reversed, current_content),
    };

    let overlap = compute_reversal_overlap(&original_content, current_content, predicted_content);
    overlap.ratio()
}
