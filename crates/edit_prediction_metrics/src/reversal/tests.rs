use super::*;
use crate::reversal::history::filter_edit_history_by_path;
use crate::reversal::hunks::{
    apply_diff_to_string_lenient, filter_diff_hunks_by_excerpt, reverse_diff,
};
use crate::reversal::overlap::{
    compute_excerpt_aware_reversal_overlap, compute_lcs_length, compute_reversal_overlap,
};
use indoc::indoc;
use std::path::Path;
use std::sync::Arc;
use zeta_prompt::udiff::{apply_diff_to_string, unified_diff_with_context};
use zeta_prompt::{ExcerptRanges, Zeta2PromptInput};

mod diff_tests;
mod excerpt_tests;
mod history_tests;
mod integration_tests;
mod lenient_tests;
mod overlap_tests;
mod unicode_tests;

fn compute_prediction_reversal_ratio(
    prompt_inputs: &Zeta2PromptInput,
    predicted_content: &str,
    cursor_path: &Path,
) -> f32 {
    compute_prediction_reversal_ratio_from_history(
        prompt_inputs.cursor_excerpt.as_ref(),
        &prompt_inputs.events,
        prompt_inputs.excerpt_start_row,
        predicted_content,
        cursor_path,
    )
}

fn make_test_prompt_inputs(
    content: &str,
    events: Vec<Arc<zeta_prompt::Event>>,
    excerpt_start_row: Option<u32>,
) -> Zeta2PromptInput {
    Zeta2PromptInput {
        cursor_path: Arc::from(Path::new("src/test.rs")),
        cursor_excerpt: content.into(),
        cursor_offset_in_excerpt: 0,
        excerpt_start_row,
        events,
        related_files: Some(Vec::new()),
        active_buffer_diagnostics: Vec::new(),
        excerpt_ranges: ExcerptRanges {
            editable_150: 0..content.len(),
            editable_180: 0..content.len(),
            editable_350: 0..content.len(),
            editable_150_context_350: 0..content.len(),
            editable_180_context_350: 0..content.len(),
            editable_350_context_150: 0..content.len(),
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    }
}
