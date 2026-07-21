use super::*;

fn format_seed_coder(input: &Zeta2PromptInput) -> String {
    format_prompt_with_budget_for_format(input, ZetaFormat::V0211SeedCoder, 10000)
        .expect("seed coder prompt formatting should succeed")
}

#[track_caller]
fn format_seed_coder_with_budget(input: &Zeta2PromptInput, max_tokens: usize) -> String {
    format_prompt_with_budget_for_format(input, ZetaFormat::V0211SeedCoder, max_tokens)
        .expect("seed coder prompt formatting should succeed")
}

#[test]
fn test_seed_coder_alias_matches_v0211_seed_coder() {
    let input = make_input(
        "prefix\neditable\nsuffix",
        7..15,
        10,
        vec![make_event("a.rs", "-old\n+new\n")],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );

    assert_eq!(
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0211SeedCoder, 10000),
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0331SeedCoderModelPy, 10000)
    );
    assert_eq!(
        ZetaFormat::parse("V0331SeedCoderModelPy").unwrap(),
        ZetaFormat::V0331SeedCoderModelPy
    );
}

#[test]
fn test_seed_coder_basic_format() {
    let input = make_input(
        "prefix\neditable\nsuffix",
        7..15,
        10,
        vec![make_event("a.rs", "-old\n+new\n")],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );

    assert_eq!(
        format_seed_coder(&input),
        indoc! {r#"
            <[fim-suffix]>
            suffix
            <[fim-prefix]><filename>related.rs
            fn helper() {}

            <filename>edit_history
            --- a/a.rs
            +++ b/a.rs
            -old
            +new

            <filename>test.rs
            prefix
            <<<<<<< CURRENT
            edi<|user_cursor|>table
            =======
            <[fim-middle]>"#}
    );
}

#[test]
fn test_qwen36_multi_region_uses_qwen_psm_fim_format() {
    let input = make_input(
        "prefix\neditable\nsuffix",
        7..15,
        10,
        vec![make_event("a.rs", "-old\n+new\n")],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );

    assert_eq!(
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0608QwenMultiRegions, 10000)
            .expect("qwen prompt formatting should succeed"),
        indoc! {r#"
            <|fim_prefix|><|file_sep|>related.rs
            fn helper() {}

            <|file_sep|>edit_history
            --- a/a.rs
            +++ b/a.rs
            -old
            +new

            <|file_sep|>test.rs
            prefix
            <|marker_1|>edi<|user_cursor|>table<|marker_2|>
            <|fim_suffix|>
            suffix
            <|fim_middle|>"#}
    );
}

#[test]
fn test_seed_coder_no_context() {
    let input = make_input("before\nmiddle\nafter", 7..13, 10, vec![], vec![]);

    assert_eq!(
        format_seed_coder(&input),
        indoc! {r#"
            <[fim-suffix]>
            after
            <[fim-prefix]><filename>test.rs
            before
            <<<<<<< CURRENT
            mid<|user_cursor|>dle
            =======
            <[fim-middle]>"#}
    );
}

#[test]
fn test_seed_coder_truncation_drops_context() {
    let input = make_input(
        "code",
        0..4,
        2,
        vec![make_event("a.rs", "-x\n+y\n")],
        vec![make_related_file("r1.rs", "content\n")],
    );

    // With large budget, everything is included
    assert_eq!(
        format_seed_coder(&input),
        indoc! {r#"
            <[fim-suffix]>
            <[fim-prefix]><filename>r1.rs
            content

            <filename>edit_history
            --- a/a.rs
            +++ b/a.rs
            -x
            +y

            <filename>test.rs
            <<<<<<< CURRENT
            co<|user_cursor|>de
            =======
            <[fim-middle]>"#}
    );

    assert_eq!(
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0211SeedCoder, 24),
        None
    );

    assert_eq!(
        format_seed_coder_with_budget(&input, 40),
        indoc! {r#"
            <[fim-suffix]>
            <[fim-prefix]><filename>test.rs
            <<<<<<< CURRENT
            co<|user_cursor|>de
            =======
            <[fim-middle]>"#
        }
    )
}

#[test]
fn test_seed_coder_truncation_prioritizes_lower_order() {
    let input = make_input(
        "code",
        0..4,
        2,
        vec![],
        vec![
            RelatedFile {
                path: Path::new("low_prio.rs").into(),
                max_row: 5,
                in_open_source_repo: false,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..5,
                    text: "low prio\n".into(),
                    order: 10,
                    context_source: ContextSource::Lsp,
                }],
            },
            RelatedFile {
                path: Path::new("high_prio.rs").into(),
                max_row: 5,
                in_open_source_repo: false,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..5,
                    text: "high prio\n".into(),
                    order: 1,
                    context_source: ContextSource::Lsp,
                }],
            },
        ],
    );

    // With large budget, both included; rendered in stable lexicographic order.
    assert_eq!(
        format_seed_coder(&input),
        indoc! {r#"
            <[fim-suffix]>
            <[fim-prefix]><filename>low_prio.rs
            low prio
            <filename>high_prio.rs
            high prio

            <filename>test.rs
            <<<<<<< CURRENT
            co<|user_cursor|>de
            =======
            <[fim-middle]>"#}
    );

    // With tight budget under the generic heuristic, context is dropped but the
    // minimal cursor section still fits.
    assert_eq!(
        format_prompt_with_budget_for_format(&input, ZetaFormat::V0211SeedCoder, 44),
        Some(
            indoc! {r#"
                <[fim-suffix]>
                <[fim-prefix]><filename>test.rs
                <<<<<<< CURRENT
                co<|user_cursor|>de
                =======
                <[fim-middle]>"#}
            .to_string()
        )
    );
}
