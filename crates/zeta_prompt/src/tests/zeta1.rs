use super::*;

#[test]
fn test_format_zeta1_from_input_basic() {
    let excerpt = "fn before() {}\nfn foo() {\n    let x = 1;\n}\nfn after() {}\n";
    let input = Zeta2PromptInput {
        cursor_path: Path::new("src/main.rs").into(),
        cursor_excerpt: excerpt.into(),
        cursor_offset_in_excerpt: 30,
        excerpt_start_row: Some(0),
        events: vec![Arc::new(make_event("other.rs", "-old\n+new\n"))],
        related_files: Some(vec![]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges {
            editable_150: 15..41,
            editable_180: 15..41,
            editable_350: 15..41,
            editable_150_context_350: 0..excerpt.len(),
            editable_180_context_350: 0..excerpt.len(),
            editable_350_context_150: 0..excerpt.len(),
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    let prompt = crate::zeta1::format_zeta1_from_input(&input, 15..41, 0..excerpt.len());

    assert_eq!(
        prompt,
        concat!(
            "### Instruction:\n",
            "You are a code completion assistant and your task is to analyze user edits and then rewrite an ",
            "excerpt that the user provides, suggesting the appropriate edits within the excerpt, taking ",
            "into account the cursor location.\n",
            "\n",
            "### User Edits:\n",
            "\n",
            "User edited other.rs:\n",
            "```diff\n",
            "-old\n",
            "+new\n",
            "\n",
            "```\n",
            "\n",
            "### User Excerpt:\n",
            "\n",
            "```src/main.rs\n",
            "<|start_of_file|>\n",
            "fn before() {}\n",
            "<|editable_region_start|>\n",
            "fn foo() {\n",
            "    <|user_cursor_is_here|>let x = 1;\n",
            "\n",
            "<|editable_region_end|>}\n",
            "fn after() {}\n",
            "\n",
            "```\n",
            "\n",
            "### Response:\n",
        ),
    );
}

#[test]
fn test_format_zeta1_from_input_no_start_of_file() {
    let excerpt = "fn foo() {\n    let x = 1;\n}\n";
    let input = Zeta2PromptInput {
        cursor_path: Path::new("src/main.rs").into(),
        cursor_excerpt: excerpt.into(),
        cursor_offset_in_excerpt: 15,
        excerpt_start_row: Some(10),
        events: vec![],
        related_files: Some(vec![]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges {
            editable_150: 0..28,
            editable_180: 0..28,
            editable_350: 0..28,
            editable_150_context_350: 0..28,
            editable_180_context_350: 0..28,
            editable_350_context_150: 0..28,
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    let prompt = crate::zeta1::format_zeta1_from_input(&input, 0..28, 0..28);

    assert_eq!(
        prompt,
        concat!(
            "### Instruction:\n",
            "You are a code completion assistant and your task is to analyze user edits and then rewrite an ",
            "excerpt that the user provides, suggesting the appropriate edits within the excerpt, taking ",
            "into account the cursor location.\n",
            "\n",
            "### User Edits:\n",
            "\n",
            "\n",
            "\n",
            "### User Excerpt:\n",
            "\n",
            "```src/main.rs\n",
            "<|editable_region_start|>\n",
            "fn foo() {\n",
            "    <|user_cursor_is_here|>let x = 1;\n",
            "}\n",
            "\n",
            "<|editable_region_end|>\n",
            "```\n",
            "\n",
            "### Response:\n",
        ),
    );
}

#[test]
fn test_format_zeta1_from_input_with_sub_ranges() {
    let excerpt = "// prefix\nfn foo() {\n    let x = 1;\n}\n// suffix\n";
    let editable_range = 10..37;
    let context_range = 0..excerpt.len();

    let input = Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: excerpt.into(),
        cursor_offset_in_excerpt: 25,
        excerpt_start_row: Some(0),
        events: vec![],
        related_files: Some(vec![]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges {
            editable_150: editable_range.clone(),
            editable_180: editable_range.clone(),
            editable_350: editable_range.clone(),
            editable_150_context_350: context_range.clone(),
            editable_180_context_350: context_range.clone(),
            editable_350_context_150: context_range.clone(),
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    let prompt = crate::zeta1::format_zeta1_from_input(&input, editable_range, context_range);

    assert_eq!(
        prompt,
        concat!(
            "### Instruction:\n",
            "You are a code completion assistant and your task is to analyze user edits and then rewrite an ",
            "excerpt that the user provides, suggesting the appropriate edits within the excerpt, taking ",
            "into account the cursor location.\n",
            "\n",
            "### User Edits:\n",
            "\n",
            "\n",
            "\n",
            "### User Excerpt:\n",
            "\n",
            "```test.rs\n",
            "<|start_of_file|>\n",
            "// prefix\n",
            "<|editable_region_start|>\n",
            "fn foo() {\n",
            "    <|user_cursor_is_here|>let x = 1;\n",
            "}\n",
            "<|editable_region_end|>\n",
            "// suffix\n",
            "\n",
            "```\n",
            "\n",
            "### Response:\n",
        ),
    );
}

#[test]
fn test_max_event_count() {
    fn make_numbered_event(index: usize) -> Event {
        return make_event(
            &format!("event-{index}.rs"),
            &format!("-old-{index}\n+new-{index}\n"),
        );
    }
    let input = make_input(
        "x",
        0..1,
        0,
        (0..3).map(make_numbered_event).collect(),
        vec![],
    );

    let edit_history_section = format_edit_history_within_budget(
        &input.events,
        "<|file_sep|>",
        "edit history",
        usize::MAX,
        5,
    );

    assert_eq!(
        &edit_history_section,
        indoc!(
            "
            <|file_sep|>edit history
            --- a/event-0.rs
            +++ b/event-0.rs
            -old-0
            +new-0
            --- a/event-1.rs
            +++ b/event-1.rs
            -old-1
            +new-1
            --- a/event-2.rs
            +++ b/event-2.rs
            -old-2
            +new-2
        "
        )
    );

    let edit_history_section = format_edit_history_within_budget(
        &input.events,
        "<|file_sep|>",
        "edit history",
        usize::MAX,
        2,
    );

    assert_eq!(
        &edit_history_section,
        indoc!(
            "
            <|file_sep|>edit history
            --- a/event-1.rs
            +++ b/event-1.rs
            -old-1
            +new-1
            --- a/event-2.rs
            +++ b/event-2.rs
            -old-2
            +new-2
        "
        )
    );

    let edit_history_section = format_edit_history_within_budget(
        &input.events,
        "<|file_sep|>",
        "edit history",
        usize::MAX,
        0,
    );

    assert_eq!(&edit_history_section, "");
}

#[test]
fn test_clean_zeta1_model_output_basic() {
    let output = indoc! {"
        <|editable_region_start|>
        fn main() {
            println!(\"hello\");
        }
        <|editable_region_end|>
    "};

    let cleaned = crate::zeta1::clean_zeta1_model_output(output).unwrap();
    assert_eq!(cleaned, "fn main() {\n    println!(\"hello\");\n}");
}

#[test]
fn test_clean_zeta1_model_output_with_cursor() {
    let output = indoc! {"
        <|editable_region_start|>
        fn main() {
            <|user_cursor_is_here|>println!(\"hello\");
        }
        <|editable_region_end|>
    "};

    let cleaned = crate::zeta1::clean_zeta1_model_output(output).unwrap();
    assert_eq!(
        cleaned,
        "fn main() {\n    <|user_cursor|>println!(\"hello\");\n}"
    );
}

#[test]
fn test_clean_zeta1_model_output_no_markers() {
    let output = "fn main() {}\n";
    let cleaned = crate::zeta1::clean_zeta1_model_output(output).unwrap();
    assert_eq!(cleaned, "fn main() {}\n");
}

#[test]
fn test_clean_zeta1_model_output_empty_region() {
    let output = "<|editable_region_start|>\n<|editable_region_end|>\n";
    let cleaned = crate::zeta1::clean_zeta1_model_output(output).unwrap();
    assert_eq!(cleaned, "");
}
