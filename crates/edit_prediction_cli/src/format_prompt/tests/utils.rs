use super::*;

#[test]
fn test_extract_all_codeblocks_multiple() {
    let text = indoc::indoc! {"
        First edit:

        `````
        block one
        `````

        Second edit:

        `````
        block two
        with ``` nested
        `````
        "};
    let blocks = extract_all_codeblocks(text);
    assert_eq!(
        blocks,
        vec![
            "block one\n".to_string(),
            "block two\nwith ``` nested\n".to_string()
        ]
    );
}

#[test]
fn test_extract_last_code_block() {
    let text = indoc::indoc! {"
        Some thinking

        ```
        first block
        ```

        `````path='something' lines=1:2
        last block
        `````
        "};
    let last_block = extract_last_codeblock(text).unwrap();
    assert_eq!(last_block, "last block\n");
}

#[test]
fn test_extract_codeblock_with_nested_fences() {
    let text = indoc::indoc! {"
        `````
        content with ``` inline
        and ```python nested
        more content
        `````
        "};
    let last_block = extract_last_codeblock(text).unwrap();
    assert_eq!(
        last_block,
        "content with ``` inline\nand ```python nested\nmore content\n"
    );
}

#[test]
fn test_extract_codeblock_ignores_inline_backticks() {
    let text = indoc::indoc! {"
        `````
        here is some `code` with inline backticks
        and here```more```stuff
        `````
        "};
    let last_block = extract_last_codeblock(text).unwrap();
    assert_eq!(
        last_block,
        "here is some `code` with inline backticks\nand here```more```stuff\n"
    );
}

#[test]
fn test_extract_editable_region_old_format() {
    let text = indoc::indoc! {"
        some lines
        are
        here
        <|editable_region_start|>
        one
        two three

        <|editable_region_end|>
        more
        lines here
        "};
    let parsed = TeacherPrompt::extract_editable_region(text).unwrap();
    assert_eq!(
        parsed,
        indoc::indoc! {"
        one
        two three"}
    );
}

#[test]
fn test_extract_editable_region_marker_format() {
    let text = indoc::indoc! {"
        some context
        <|marker_1|>
        one
        two three
        <|marker_2|>
        more context
        "};
    let parsed = multi_region::extract_editable_region_from_markers(text).unwrap();
    assert_eq!(parsed, "one\ntwo three");
}

#[test]
fn test_extract_editable_region_multi_markers() {
    let text = indoc::indoc! {"
        prefix
        <|marker_1|>
        aaa
        bbb
        <|marker_2|>
        ccc
        ddd
        <|marker_3|>
        suffix
        "};
    let parsed = multi_region::extract_editable_region_from_markers(text).unwrap();
    // Intermediate marker and its trailing \n are stripped
    assert_eq!(parsed, "aaa\nbbb\nccc\nddd");
}

#[test]
fn test_extract_last_codeblock_nested_bibtex() {
    let text = indoc::indoc! {r#"
        Looking at the edit history, I can see that a Citation section was just added.

        `````
        ## Collaborations
        Our mission is to create a 4D generative model.

        ## Citation

        If you found Unique3D helpful, please cite our report:
        ```bibtex
        @misc{wu2024unique3d,
              title={Unique3D},
        }
        ```
        `````
        "#};
    let last_block = extract_last_codeblock(text).unwrap();
    assert_eq!(
        last_block,
        indoc::indoc! {r#"
        ## Collaborations
        Our mission is to create a 4D generative model.

        ## Citation

        If you found Unique3D helpful, please cite our report:
        ```bibtex
        @misc{wu2024unique3d,
              title={Unique3D},
        }
        ```
        "#}
    );
}

#[test]
fn test_extract_editable_region_no_markers() {
    let text = indoc::indoc! {"
        one
        two three"};
    let parsed = TeacherPrompt::extract_editable_region(text).unwrap();
    assert_eq!(
        parsed,
        indoc::indoc! {"
        one
        two three"}
    );
}

#[test]
fn test_parse_no_edits_response() {
    let response = indoc::indoc! {"
        The code is already complete. There is no clear next edit to make.

        `````
        NO_EDITS
        `````
    "};
    let codeblock = extract_last_codeblock(response).unwrap();
    assert_eq!(codeblock.trim(), TeacherPrompt::NO_EDITS);
}

#[test]
fn test_extract_codeblock_no_valid_block() {
    // Text with no code blocks should return None
    let text = "Just some plain text without any code blocks";
    assert!(extract_last_codeblock(text).is_none());

    // Unclosed code block should return None
    let text = indoc::indoc! {"
        ```
        unclosed block
    "};
    assert!(extract_last_codeblock(text).is_none());

    // Analysis text with nested markdown but no proper outer block
    let text = indoc::indoc! {"
        # Analysis
        Looking at this:
        ```
        some code
        ```
        But then more analysis without wrapping block
    "};
    // This should find the inner block
    let result = extract_last_codeblock(text).unwrap();
    assert_eq!(result, "some code\n");
}

#[test]
fn test_extract_codeblock_no_trailing_newline() {
    // Text ending without trailing newline after closing fence
    let text = "`````\ncontent here\n`````";
    let result = extract_last_codeblock(text).unwrap();
    assert_eq!(result, "content here\n");
}

#[test]
fn test_parse_no_edits_response_with_trailing_backticks() {
    let response = "NO_EDITS```";

    let parsed = TeacherPrompt::parse(
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
            prompt_inputs: None,
            prompt: None,
            predictions: Vec::new(),
            score: Vec::new(),
            qa: Vec::new(),
            mav_version: None,
            state: None,
        },
        response,
    )
    .unwrap();

    assert!(parsed.0.is_empty());
    assert!(parsed.1.is_none());
}
