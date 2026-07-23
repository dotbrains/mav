use super::support::make_example;
use super::*;

#[test]
fn test_teacher_jumps_format_prompt_markers_everywhere() {
    let example = make_example(
        "fn main() {\n    let x = 1;\n}\n",
        16,
        &[("src/lib.rs", &[("pub fn helper() {}\n", 5)])],
    );
    let prompt = TeacherJumpsPrompt::format_prompt(&example, 8192).unwrap();

    assert!(prompt.contains(TeacherJumpsPrompt::USER_CURSOR_MARKER));
    assert!(prompt.contains("`````src/main.rs\n"));
    assert!(prompt.contains("`````src/lib.rs\n"));
    // Markers in both the current file and the related excerpt.
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    for snippet in &marker_table {
        for (id, _) in &snippet.markers {
            assert!(
                prompt.contains(&hashed_regions::marker_tag(id)),
                "prompt is missing marker {id}"
            );
        }
    }
    // The current file appears exactly once, in its own section, with the
    // user cursor injected.
    assert_eq!(prompt.matches("let x = 1;").count(), 1);
    assert!(prompt.contains("<|user_cursor|>let x = 1;"));
}

#[test]
fn test_teacher_jumps_cursor_file_with_coinciding_worktree_root_name() {
    // Worktree root `jaq` contains a `jaq/` subdirectory: the cursor path
    // is `jaq/src/main.rs` while the related-file entry is prefixed with
    // the root name (`jaq/jaq/src/main.rs`).
    let mut example = make_example("fn main() {\n    let x = 1;\n}\n", 16, &[]);
    example.spec.cursor_path = std::sync::Arc::from(std::path::Path::new("jaq/src/main.rs"));
    {
        let prompt_inputs = example.prompt_inputs.as_mut().unwrap();
        prompt_inputs.cursor_path = std::path::Path::new("jaq/src/main.rs").into();
        prompt_inputs.related_files.as_mut().unwrap()[0].path =
            std::sync::Arc::from(std::path::Path::new("jaq/jaq/src/main.rs"));
    }

    let prompt = TeacherJumpsPrompt::format_prompt(&example, 8192).unwrap();
    assert!(prompt.contains("<|user_cursor|>let x = 1;"));

    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let cursor_markers = &marker_table[0].markers;
    let start_tag = hashed_regions::marker_tag(&cursor_markers[0].0);
    let end_tag = hashed_regions::marker_tag(&cursor_markers[cursor_markers.len() - 1].0);
    let response =
        format!("`````\n{start_tag}\nfn main() {{\n    let x = 2;\n}}\n{end_tag}\n`````\n");
    let (patch, _) = TeacherJumpsPrompt::parse(&example, &response).unwrap();
    assert!(patch.contains("--- a/jaq/src/main.rs"), "patch: {patch}");
}

#[test]
fn test_teacher_jumps_format_prompt_requires_current_file_context() {
    let mut example = make_example("fn main() {}\n", 0, &[]);
    example.prompt_inputs.as_mut().unwrap().related_files = Some(Vec::new());
    assert!(TeacherJumpsPrompt::format_prompt(&example, 8192).is_err());
}

#[test]
fn test_teacher_jumps_synthesizes_missing_cursor_file_excerpt() {
    // Simulate a settled-data sample: `cursor_excerpt` is present, but the
    // related-file excerpts of the cursor file don't cover the cursor (only
    // an unrelated fragment elsewhere in the file).
    let mut example = make_example("fn main() {\n    let x = 1;\n}\n", 16, &[]);
    {
        let prompt_inputs = example.prompt_inputs.as_mut().unwrap();
        prompt_inputs.related_files.as_mut().unwrap()[0].excerpts =
            vec![zeta_prompt::RelatedExcerpt {
                row_range: 40..42,
                text: std::sync::Arc::from("// unrelated\n// fragment\n"),
                order: 0,
                context_source: zeta_prompt::ContextSource::Bm25,
            }];
    }

    // Without a covering cursor-file excerpt, formatting hard-errors.
    assert!(TeacherJumpsPrompt::format_prompt(&example, 8192).is_err());

    // `ensure_cursor_file_excerpt` (in zeta_prompt) synthesizes one from
    // `cursor_excerpt`, so the prompt formats with the cursor in the
    // current-file window and the unrelated fragment is replaced (no
    // duplicated content with overlapping markers).
    hashed_regions::ensure_cursor_file_excerpt(example.prompt_inputs.as_mut().unwrap());
    let prompt = TeacherJumpsPrompt::format_prompt(&example, 8192).unwrap();
    assert!(prompt.contains("<|user_cursor|>let x = 1;"));
    assert!(!prompt.contains("// unrelated"));
    assert_eq!(prompt.matches("let x = 1;").count(), 1);
}

#[test]
fn test_teacher_jumps_cursor_file_hunks_are_file_absolute() {
    let mut example = make_example("fn main() {\n    let x = 1;\n}\n", 16, &[]);
    {
        let prompt_inputs = example.prompt_inputs.as_mut().unwrap();
        prompt_inputs.excerpt_start_row = Some(10);
        prompt_inputs.related_files.as_mut().unwrap()[0].excerpts[0].row_range = 10..13;
    }
    let marker_table = hashed_regions::build_marker_table(example.prompt_inputs.as_ref().unwrap());
    let cursor_markers = &marker_table[0].markers;
    let start_tag = hashed_regions::marker_tag(&cursor_markers[0].0);
    let end_tag = hashed_regions::marker_tag(&cursor_markers[cursor_markers.len() - 1].0);

    let response = format!(
        "The user is changing x.\n\n`````\n{start_tag}\nfn main() {{\n    let x = 2;<|user_cursor|>\n}}\n{end_tag}\n`````\n"
    );
    let (patch, cursor) = TeacherJumpsPrompt::parse(&example, &response).unwrap();

    // Hunk rows are file-absolute (1-based in the hunk header, excerpt
    // starts at 0-based row 10).
    assert!(patch.contains("@@ -11,"), "patch: {patch}");
    let cursor = cursor.unwrap();
    assert_eq!(cursor.row, 11);
}
