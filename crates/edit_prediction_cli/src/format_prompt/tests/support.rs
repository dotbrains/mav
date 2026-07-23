use super::*;

pub(super) fn make_example(
    cursor_excerpt: &str,
    cursor_offset: usize,
    related: &[(&str, &[(&str, u32)])],
) -> Example {
    // The cursor file is included as the first related file, mirroring
    // `ContextSource::CurrentFile` context retrieval.
    let cursor_file = zeta_prompt::RelatedFile {
        path: std::sync::Arc::from(std::path::Path::new("src/main.rs")),
        max_row: 1000,
        excerpts: vec![zeta_prompt::RelatedExcerpt {
            row_range: 0..cursor_excerpt.matches('\n').count() as u32,
            text: std::sync::Arc::from(cursor_excerpt),
            order: 0,
            context_source: zeta_prompt::ContextSource::CurrentFile,
        }],
        in_open_source_repo: false,
    };
    let related_files = std::iter::once(cursor_file)
        .chain(related.iter().map(|(path, excerpts)| {
            zeta_prompt::RelatedFile {
                path: std::sync::Arc::from(std::path::Path::new(path)),
                max_row: 1000,
                excerpts: excerpts
                    .iter()
                    .map(|(text, start_row)| zeta_prompt::RelatedExcerpt {
                        row_range: *start_row..*start_row + text.matches('\n').count() as u32,
                        text: std::sync::Arc::from(*text),
                        order: 0,
                        context_source: zeta_prompt::ContextSource::CurrentFile,
                    })
                    .collect(),
                in_open_source_repo: false,
            }
        }))
        .collect();

    Example {
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
        prompt_inputs: Some(zeta_prompt::Zeta2PromptInput {
            cursor_path: std::path::Path::new("src/main.rs").into(),
            cursor_excerpt: cursor_excerpt.into(),
            cursor_offset_in_excerpt: cursor_offset,
            excerpt_start_row: Some(0),
            events: Vec::new(),
            related_files: Some(related_files),
            active_buffer_diagnostics: Vec::new(),
            excerpt_ranges: zeta_prompt::ExcerptRanges::default(),
            syntax_ranges: None,
            in_open_source_repo: false,
            can_collect_data: false,
            repo_url: None,
        }),
        prompt: None,
        predictions: Vec::new(),
        score: Vec::new(),
        qa: Vec::new(),
        mav_version: None,
        state: None,
    }
}
