use super::*;
use crate::{PredictionProvider, TeacherBackend};
use edit_prediction::example_spec::ExampleSpec;
use std::{path::Path, sync::Arc};
use zeta_prompt::ZetaFormat;

fn example_with_previous_prediction() -> Example {
    Example {
        spec: ExampleSpec {
            name: "example".to_string(),
            repository_url: "https://github.com/mav-industries/mav.git".to_string(),
            revision: "HEAD".to_string(),
            tags: Vec::new(),
            reasoning: None,
            uncommitted_diff: String::new(),
            recently_opened_files: Vec::new(),
            recently_viewed_files: Vec::new(),
            uncommitted_diff_contains_edit_history: false,
            cursor_path: Arc::from(Path::new("src/main.rs")),
            cursor_position: "0:0".to_string(),
            edit_history: String::new(),
            expected_patches: Vec::new(),
            rejected_patch: None,
            telemetry: None,
            human_feedback: Vec::new(),
            rating: None,
        },
        prompt_inputs: None,
        prompt: None,
        predictions: vec![ExamplePrediction {
            actual_patch: Some("previous patch".to_string()),
            actual_output: String::new(),
            actual_cursor: Some(ActualCursor {
                path: "src/main.rs".to_string(),
                row: 1,
                column: 2,
                offset: 3,
                editable_region_offset: Some(4),
            }),
            error: None,
            provider: PredictionProvider::Teacher(TeacherBackend::Sonnet45, ZetaFormat::default()),
            cumulative_logprob: None,
            avg_logprob: None,
        }],
        score: Vec::new(),
        qa: Vec::new(),
        mav_version: None,
        state: None,
    }
}

#[test]
fn test_parse_keeps_previous_when_sentinel_appears_outside_last_codeblock() {
    let example = example_with_previous_prediction();
    let actual_output = indoc::indoc! {"
        After reviewing the feedback, the previous prediction is still correct.
        Use `KEEP_PREVIOUS`.

        ```
        unrelated trailing code block
        ```
    "};

    let (patch, cursor) = parse(&example, actual_output).unwrap();

    assert_eq!(patch, "previous patch");
    assert_eq!(cursor.unwrap().offset, 3);
}
