use super::*;

pub(crate) fn build_example_from_snowflake(
    request_id: String,
    device_id: String,
    time: String,
    input: Zeta2PromptInput,
    tags: Vec<String>,
    rejection: Option<RejectionInfo>,
    mav_version: Option<String>,
) -> Example {
    let cursor_excerpt = input.cursor_excerpt.as_ref();
    let cursor_offset = input.cursor_offset_in_excerpt;

    let mut edit_history = String::new();
    for event in &input.events {
        zeta_prompt::write_event(&mut edit_history, event);
        edit_history.push('\n');
    }

    let (rejection_reason, was_shown) = match &rejection {
        Some(r) => (r.reason.clone(), r.was_shown),
        None => (String::new(), false),
    };

    let spec = ExampleSpec {
        name: request_id.clone(),
        repository_url: String::new(),
        revision: String::new(),
        tags,
        reasoning: None,
        uncommitted_diff: String::new(),
        recently_opened_files: Vec::new(),
        recently_viewed_files: Vec::new(),
        uncommitted_diff_contains_edit_history: false,
        cursor_path: input.cursor_path.clone(),
        cursor_position: build_cursor_position(cursor_excerpt, cursor_offset),
        edit_history,
        expected_patches: Vec::new(),
        rejected_patch: None,
        telemetry: Some(TelemetrySource {
            request_id,
            device_id,
            time,
            rejection_reason,
            was_shown,
        }),
        human_feedback: Vec::new(),
        rating: None,
    };

    Example {
        spec,
        mav_version,
        prompt_inputs: Some(input),
        prompt: None,
        predictions: Vec::new(),
        score: Vec::new(),
        qa: Vec::new(),
        state: None,
    }
}

fn build_cursor_position(excerpt: &str, cursor_offset: usize) -> String {
    let before = &excerpt[..cursor_offset.min(excerpt.len())];
    let after = &excerpt[cursor_offset.min(excerpt.len())..];
    format!("{}[CURSOR_POSITION]{}", before, after)
}

pub(crate) fn build_output_patch(
    cursor_path: &std::path::Path,
    cursor_excerpt: &str,
    editable_range: &std::ops::Range<usize>,
    model_output: &str,
) -> String {
    let old_text = &cursor_excerpt[editable_range.clone()];

    let editable_start_row = cursor_excerpt[..editable_range.start]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32;

    let diff_body = language::unified_diff_with_offsets(
        old_text,
        model_output,
        editable_start_row,
        editable_start_row,
    );

    let mut patch = String::new();
    writeln!(&mut patch, "--- a/{}", cursor_path.display()).ok();
    writeln!(&mut patch, "+++ b/{}", cursor_path.display()).ok();
    patch.push_str(&diff_body);
    patch
}
