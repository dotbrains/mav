use super::support::make_example;
use super::*;
use zeta_prompt::multi_region;

#[test]
fn test_v0327_teacher_prompt_uses_resolved_ranges() {
    let excerpt = (0..80)
        .map(|index| format!("line{index:02}\n"))
        .collect::<String>();
    let cursor_offset = excerpt.find("line40").expect("cursor line exists");
    let prompt_inputs = zeta_prompt::Zeta2PromptInput {
        cursor_path: std::path::Path::new("src/main.rs").into(),
        cursor_excerpt: excerpt.clone().into(),
        cursor_offset_in_excerpt: cursor_offset,
        excerpt_start_row: None,
        events: Vec::new(),
        related_files: Some(Vec::new()),
        active_buffer_diagnostics: Vec::new(),
        excerpt_ranges: zeta_prompt::ExcerptRanges {
            editable_150: 0..32,
            editable_180: 0..32,
            editable_350: 0..32,
            editable_512: None,
            editable_150_context_350: 0..48,
            editable_180_context_350: 0..48,
            editable_350_context_150: 20..50,
            editable_350_context_512: None,
            editable_350_context_1024: None,
            context_4096: None,
            context_8192: Some(30..excerpt.len()),
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    let (stored_editable_range, stored_context_range) = zeta_prompt::excerpt_range_for_format(
        ZetaFormat::V0327SingleFile,
        &prompt_inputs.excerpt_ranges,
    );
    assert!(stored_context_range.start > stored_editable_range.start);

    let (editable_range, context_range) =
        resolved_excerpt_ranges_for_format(&prompt_inputs, ZetaFormat::V0327SingleFile);
    assert_eq!(context_range, 0..excerpt.len());
    assert!(editable_range.start < cursor_offset);
    assert!(editable_range.end > cursor_offset);

    let prompt = TeacherPrompt::format_prompt(
        &Example {
            spec: edit_prediction::example_spec::ExampleSpec {
                name: "test".to_string(),
                repository_url: "https://github.com/mav-industries/mav.git".to_string(),
                revision: "HEAD".to_string(),
                tags: Vec::new(),
                reasoning: None,
                uncommitted_diff: String::new(),
                recently_opened_files: Vec::new(),
                recently_viewed_files: Vec::new(),
                uncommitted_diff_contains_edit_history: false,
                cursor_path: std::sync::Arc::from(std::path::Path::new("src/main.rs")),
                cursor_position: "0:0".to_string(),
                edit_history: String::new(),
                expected_patches: Vec::new(),
                rejected_patch: None,
                telemetry: None,
                human_feedback: Vec::new(),
                rating: None,
            },
            prompt_inputs: Some(prompt_inputs),
            prompt: None,
            predictions: Vec::new(),
            score: Vec::new(),
            qa: Vec::new(),
            mav_version: None,
            state: None,
        },
        editable_range,
        context_range,
        false,
    );

    assert!(prompt.contains(TeacherPrompt::EDITABLE_REGION_START));
    assert!(prompt.contains(TeacherPrompt::USER_CURSOR_MARKER));
    assert!(prompt.contains("line40"));
}
