use super::*;

#[test]
fn test_v0420_formats_diagnostics_before_related_files() {
    let mut input = make_input(
        "prefix\neditable\nsuffix",
        7..15,
        10,
        vec![],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );
    input.active_buffer_diagnostics = vec![
        ActiveBufferDiagnostic {
            severity: Some(1),
            message: "missing semicolon".to_string(),
            snippet: "let value = 1".to_string(),
            snippet_buffer_row_range: 1..2,
            diagnostic_range_in_snippet: 12..13,
        },
        ActiveBufferDiagnostic {
            severity: Some(2),
            message: "file-level warning".to_string(),
            snippet: String::new(),
            snippet_buffer_row_range: 0..0,
            diagnostic_range_in_snippet: 0..0,
        },
    ];

    let prompt = format_prompt_with_budget_for_format(&input, ZetaFormat::V0420Diagnostics, 10000)
        .expect("v0420 prompt formatting should succeed");

    assert_eq!(
        prompt,
        indoc! {r#"
            <[fim-suffix]>
            suffix
            <[fim-prefix]><filename>diagnostics
            *missing semicolon*:
            ```
            let value = 1
            ```
            *file-level warning*

            <filename>related.rs
            fn helper() {}

            <filename>test.rs
            prefix
            <|marker_1|>edi<|user_cursor|>table<|marker_2|>
            <[fim-middle]>"#}
    );
}

#[test]
fn test_v0317_formats_prompt_with_many_related_files() {
    let related_files = (0..900)
        .map(|index| {
            make_related_file(
                &format!("related_{index}.rs"),
                "fn helper() {\n    let value = 1;\n}\n",
            )
        })
        .collect();

    let input = make_input(
        "code",
        0..4,
        2,
        vec![make_event("a.rs", "-x\n+y\n")],
        related_files,
    );

    let prompt =
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0317SeedMultiRegions, 4096);

    assert!(prompt.is_some());
    let prompt = prompt.expect("v0317 should produce a prompt under high related-file count");
    assert!(prompt.contains("test.rs"));
    assert!(prompt.contains(CURSOR_MARKER));
}

#[test]
fn test_v0327_formats_single_file_prompt_without_related_files() {
    let excerpt = indoc! {"
        line01
        line02
        line03
        line04
        line05
        line06
        line07
        line08
        line09
        line10
        line11
        line12
        line13
        line14
        line15
        line16
        line17
        line18
        line19
        line20
    "};
    let cursor_offset = excerpt.find("line10").expect("cursor line exists");
    let input = make_input(
        excerpt,
        0..excerpt.len(),
        cursor_offset,
        vec![make_event("a.rs", "-x\n+y\n")],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );

    let prompt = format_prompt_with_budget_for_format(&input, ZetaFormat::V0327SingleFile, 4096)
        .expect("v0327 prompt should fit");

    assert!(prompt.contains("line01"));
    assert!(prompt.contains("line20"));
    assert!(prompt.contains("<filename>edit_history"));
    assert!(prompt.contains("<filename>test.rs"));
    assert!(prompt.contains(CURSOR_MARKER));
    assert!(!prompt.contains("related.rs"));
    assert!(!prompt.contains("fn helper() {}"));
}

#[test]
fn test_v0327_resolve_cursor_region_uses_full_excerpt_context() {
    let excerpt = (0..80)
        .map(|index| format!("l{index:02}\n"))
        .collect::<String>();
    let cursor_offset = excerpt.find("l40").expect("cursor line exists");
    let input = make_input(&excerpt, 0..excerpt.len(), cursor_offset, vec![], vec![]);

    let (context, editable_range, context_range, adjusted_cursor) =
        resolve_cursor_region(&input, ZetaFormat::V0327SingleFile);

    assert_eq!(context, excerpt);
    assert_eq!(context_range, 0..excerpt.len());
    assert_eq!(adjusted_cursor, cursor_offset);
    assert!(editable_range.start < adjusted_cursor);
    assert!(editable_range.end > adjusted_cursor);
    assert!(editable_range.end < excerpt.len());
}

#[test]
fn test_v0615_formats_hashed_markers_for_rendered_related_context() {
    let current_text = "fn main() {\n    let value = 1;\n}\n";
    let cursor_offset = current_text.find("let value").unwrap();
    let input = Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: current_text.into(),
        cursor_offset_in_excerpt: cursor_offset,
        excerpt_start_row: Some(0),
        events: Vec::new(),
        related_files: Some(vec![
            RelatedFile {
                path: Path::new("test.rs").into(),
                max_row: 3,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..3,
                    text: current_text.into(),
                    order: 0,
                    context_source: ContextSource::CurrentFile,
                }],
                in_open_source_repo: false,
            },
            RelatedFile {
                path: Path::new("helper.rs").into(),
                max_row: 3,
                excerpts: vec![RelatedExcerpt {
                    row_range: 10..13,
                    text: "fn helper() {\n    one();\n}\n".into(),
                    order: 1,
                    context_source: ContextSource::EditHistory,
                }],
                in_open_source_repo: false,
            },
            RelatedFile {
                path: Path::new("readonly.rs").into(),
                max_row: 1,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..1,
                    text: "pub fn readonly() {}\n".into(),
                    order: 2,
                    context_source: ContextSource::Lsp,
                }],
                in_open_source_repo: false,
            },
        ]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges::default(),
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    let prompt = format_zeta_prompt(&input, ZetaFormat::V0615HashRegions).unwrap();
    let marker_table = hashed_regions::build_marker_table(&input);
    let marker_count: usize = marker_table
        .iter()
        .map(|snippet| snippet.markers.len())
        .sum();

    assert!(prompt.starts_with("<[fim-suffix]>\n<[fim-prefix]><filename>test.rs\n"));
    assert!(prompt.ends_with("<[fim-middle]>"));
    assert!(prompt.contains(CURSOR_MARKER));
    assert!(prompt.contains("<filename>helper.rs\n"));
    assert!(prompt.contains("<filename>readonly.rs\n"));
    assert!(prompt.contains("<filename>readonly.rs\n<|marker_"));
    assert!(prompt.contains("pub fn readonly() {}"));
    assert_eq!(
        prompt.matches(hashed_regions::MARKER_TAG_PREFIX).count(),
        marker_count
    );
    assert!(!prompt.contains(seed_coder::START_MARKER));
}

#[test]
fn test_v0615_parse_related_file_jump_as_patch() {
    let current_text = "fn main() {\n    helper();\n}\n";
    let helper_text = "fn helper() {\n    one();\n}\n";
    let input = Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: current_text.into(),
        cursor_offset_in_excerpt: current_text.find("helper").unwrap(),
        excerpt_start_row: Some(0),
        events: Vec::new(),
        related_files: Some(vec![
            RelatedFile {
                path: Path::new("test.rs").into(),
                max_row: 3,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..3,
                    text: current_text.into(),
                    order: 0,
                    context_source: ContextSource::CurrentFile,
                }],
                in_open_source_repo: false,
            },
            RelatedFile {
                path: Path::new("helper.rs").into(),
                max_row: 13,
                excerpts: vec![RelatedExcerpt {
                    row_range: 10..13,
                    text: helper_text.into(),
                    order: 1,
                    context_source: ContextSource::EditHistory,
                }],
                in_open_source_repo: false,
            },
        ]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges::default(),
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };
    let marker_table = hashed_regions::build_marker_table(&input);
    let helper_markers = &marker_table[1].markers;
    let start_tag = hashed_regions::marker_tag(&helper_markers[0].0);
    let end_tag = hashed_regions::marker_tag(&helper_markers[helper_markers.len() - 1].0);
    let output = format!(
        "{start_tag}\nfn helper() {{\n    two();\n}}\n{end_tag}{}",
        hashed_regions::V0615_END_MARKER
    );

    let patch =
        parse_zeta2_model_output_as_patch(&output, ZetaFormat::V0615HashRegions, &input).unwrap();

    assert!(patch.contains("--- a/helper.rs"), "patch: {patch}");
    assert!(patch.contains("@@ -11,"), "patch: {patch}");
    assert!(patch.contains("-    one();"), "patch: {patch}");
    assert!(patch.contains("+    two();"), "patch: {patch}");
}

#[test]
fn test_v0615_expected_output_round_trips_to_patch() {
    let current_text = "fn main() {\n    helper();\n}\n";
    let helper_text = "fn helper() {\n    one();\n}\n";
    let input = Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: current_text.into(),
        cursor_offset_in_excerpt: current_text.find("helper").unwrap(),
        excerpt_start_row: Some(0),
        events: Vec::new(),
        related_files: Some(vec![
            RelatedFile {
                path: Path::new("test.rs").into(),
                max_row: 3,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..3,
                    text: current_text.into(),
                    order: 0,
                    context_source: ContextSource::CurrentFile,
                }],
                in_open_source_repo: false,
            },
            RelatedFile {
                path: Path::new("helper.rs").into(),
                max_row: 13,
                excerpts: vec![RelatedExcerpt {
                    row_range: 10..13,
                    text: helper_text.into(),
                    order: 1,
                    context_source: ContextSource::EditHistory,
                }],
                in_open_source_repo: false,
            },
        ]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges::default(),
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };
    let patch = indoc! {"
        --- a/helper.rs
        +++ b/helper.rs
        @@ -11,3 +11,3 @@
         fn helper() {
        -    one();
        +    two();
         }
    "};

    let output = format_expected_output(&input, ZetaFormat::V0615HashRegions, patch, None).unwrap();
    let parsed_patch =
        parse_zeta2_model_output_as_patch(&output, ZetaFormat::V0615HashRegions, &input).unwrap();

    assert!(output.contains(hashed_regions::MARKER_TAG_PREFIX));
    assert!(
        parsed_patch.contains("-    one();"),
        "patch: {parsed_patch}"
    );
    assert!(
        parsed_patch.contains("+    two();"),
        "patch: {parsed_patch}"
    );
}

#[test]
fn test_zeta3_prompt_matches_zeta2_seed_multi_region_prompt() {
    let excerpt = "fn main() {\n    helper();\n}\n";
    let cursor_offset = excerpt.find("helper").expect("cursor text exists") + "help".len();
    let cursor_row_start = excerpt.find("    helper").expect("cursor row exists");
    let syntax_ranges = vec![0..excerpt.len()];
    let related_file = make_related_file("related.rs", "fn helper() {}\n");
    let mut zeta2_input = make_input(
        excerpt,
        0..excerpt.len(),
        cursor_offset,
        vec![make_event("test.rs", "-old\n+new\n")],
        vec![related_file.clone()],
    );
    zeta2_input.excerpt_start_row = Some(10);
    zeta2_input.excerpt_ranges =
        compute_legacy_excerpt_ranges(excerpt, cursor_offset, &syntax_ranges);
    zeta2_input.syntax_ranges = Some(syntax_ranges.clone());

    let zeta3_input = Zeta3PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_position: FilePosition {
            row: 11,
            column: (cursor_offset - cursor_row_start) as u32,
        },
        events: vec![Arc::new(make_event("test.rs", "-old\n+new\n"))],
        editable_context: vec![
            RelatedFile {
                path: Path::new("test.rs").into(),
                max_row: 12,
                excerpts: vec![RelatedExcerpt {
                    row_range: 10..12,
                    text: excerpt.into(),
                    order: 0,
                    context_source: ContextSource::CurrentFile,
                }],
                in_open_source_repo: false,
            },
            related_file,
        ],
        syntax_ranges,
        active_buffer_diagnostics: vec![],
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };

    assert_eq!(
        format_zeta3_prompt(&zeta3_input, ZetaFormat::V0318SeedMultiRegions),
        format_zeta_prompt(&zeta2_input, ZetaFormat::V0318SeedMultiRegions),
    );
}

#[test]
fn test_zeta3_parse_seed_multi_region_output_as_patch() {
    let prefix = (0..20)
        .map(|index| format!("prefix {index}\n"))
        .collect::<String>();
    let excerpt = "fn main() {\n    let value = 1;\n    dbg!(value);\n}\n";
    let new_excerpt = "fn main() {\n    let value = 2;\n    dbg!(value);\n}\n";
    let cursor_offset = excerpt.find("value =").expect("cursor text exists");
    let input = Zeta3PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_position: FilePosition { row: 21, column: 8 },
        events: vec![],
        editable_context: vec![RelatedFile {
            path: Path::new("test.rs").into(),
            max_row: 24,
            excerpts: vec![RelatedExcerpt {
                row_range: 20..24,
                text: excerpt.into(),
                order: 0,
                context_source: ContextSource::CurrentFile,
            }],
            in_open_source_repo: false,
        }],
        syntax_ranges: vec![0..excerpt.len()],
        active_buffer_diagnostics: vec![],
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    };
    let output = multi_region::encode_from_old_and_new_v0318(
        excerpt,
        new_excerpt,
        Some(cursor_offset),
        CURSOR_MARKER,
        multi_region::V0318_END_MARKER,
    )
    .unwrap();

    let patch =
        parse_zeta3_model_output_as_patch(&output, ZetaFormat::V0318SeedMultiRegions, &input)
            .unwrap();
    let full_text = format!("{prefix}{excerpt}");
    let expected = format!("{prefix}{new_excerpt}");

    assert!(patch.contains(CURSOR_MARKER));
    assert_eq!(
        udiff::apply_diff_to_string(&patch.replace(CURSOR_MARKER, ""), &full_text).unwrap(),
        expected
    );
}
