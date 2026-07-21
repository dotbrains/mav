use super::*;

#[test]
fn test_no_truncation_when_within_budget() {
    let input = make_input(
        "prefix\neditable\nsuffix",
        7..15,
        10,
        vec![make_event("a.rs", "-old\n+new\n")],
        vec![make_related_file("related.rs", "fn helper() {}\n")],
    );

    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>related.rs
            fn helper() {}
            <|file_sep|>edit history
            --- a/a.rs
            +++ b/a.rs
            -old
            +new
            <|file_sep|>test.rs
            <|fim_prefix|>
            prefix
            <|fim_middle|>current
            edi<|user_cursor|>table
            <|fim_suffix|>

            suffix
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_truncation_drops_edit_history_when_budget_tight() {
    let input = make_input(
        "code",
        0..4,
        2,
        vec![make_event("a.rs", "-x\n+y\n")],
        vec![
            make_related_file("r1.rs", "aaaaaaa\n"),
            make_related_file("r2.rs", "bbbbbbb\n"),
        ],
    );

    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>r1.rs
            aaaaaaa
            <|file_sep|>r2.rs
            bbbbbbb
            <|file_sep|>edit history
            --- a/a.rs
            +++ b/a.rs
            -x
            +y
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            co<|user_cursor|>de
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );

    assert_eq!(
        format_with_budget(&input, budget_with_margin(55)),
        Some(
            indoc! {r#"
            <|file_sep|>edit history
            --- a/a.rs
            +++ b/a.rs
            -x
            +y
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            co<|user_cursor|>de
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
            .to_string()
        )
    );
}

#[test]
fn test_truncation_includes_partial_excerpts() {
    let input = make_input(
        "x",
        0..1,
        0,
        vec![],
        vec![RelatedFile {
            path: Path::new("big.rs").into(),
            max_row: 30,
            in_open_source_repo: false,
            excerpts: vec![
                RelatedExcerpt {
                    row_range: 0..10,
                    text: "first excerpt\n".into(),
                    order: 0,
                    context_source: ContextSource::Lsp,
                },
                RelatedExcerpt {
                    row_range: 11..20,
                    text: "second excerpt\n".into(),
                    order: 0,
                    context_source: ContextSource::Lsp,
                },
                RelatedExcerpt {
                    row_range: 21..30,
                    text: "third excerpt\n".into(),
                    order: 0,
                    context_source: ContextSource::Lsp,
                },
            ],
        }],
    );

    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>big.rs
            first excerpt
            ...
            second excerpt
            ...
            third excerpt
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );

    assert_eq!(
        format_with_budget(&input, budget_with_margin(50)).unwrap(),
        indoc! {r#"
            <|file_sep|>big.rs
            first excerpt
            ...
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_contiguous_excerpts_render_without_ellipsis() {
    // Excerpts whose row ranges touch (end == next start) are contiguous
    // segments of the same region and must render seamlessly, without an
    // ellipsis line between them.
    let input = make_input(
        "x",
        0..1,
        0,
        vec![],
        vec![RelatedFile {
            path: Path::new("big.rs").into(),
            max_row: 30,
            in_open_source_repo: false,
            excerpts: vec![
                RelatedExcerpt {
                    row_range: 0..10,
                    text: "first segment\n".into(),
                    order: 1,
                    context_source: ContextSource::GitLog,
                },
                RelatedExcerpt {
                    row_range: 10..20,
                    text: "second segment\n".into(),
                    order: 0,
                    context_source: ContextSource::OracleSnippet,
                },
            ],
        }],
    );

    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>big.rs
            first segment
            second segment
            ...
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_truncation_prioritizes_lower_order_excerpts() {
    // Two files: file_a has a high-order excerpt, file_b has a low-order one.
    // With tight budget, only the lower-order excerpt from file_b should be included.
    let input = make_input(
        "x",
        0..1,
        0,
        vec![],
        vec![
            RelatedFile {
                path: Path::new("file_a.rs").into(),
                max_row: 10,
                in_open_source_repo: false,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..10,
                    text: "low priority content\n".into(),
                    order: 5,
                    context_source: ContextSource::Lsp,
                }],
            },
            RelatedFile {
                path: Path::new("file_b.rs").into(),
                max_row: 10,
                in_open_source_repo: false,
                excerpts: vec![RelatedExcerpt {
                    row_range: 0..10,
                    text: "high priority content\n".into(),
                    order: 1,
                    context_source: ContextSource::Lsp,
                }],
            },
        ],
    );

    // With large budget, both files included; rendered in stable lexicographic order.
    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>file_a.rs
            low priority content
            <|file_sep|>file_b.rs
            high priority content
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );

    // With tight budget, only file_b (lower order) fits.
    // Cursor section is ~37 tokens, so budget 52 leaves ~15 for related files.
    // file_b header (7) + excerpt (7) = 14 tokens, which fits.
    // file_a would need another 14 tokens, which doesn't fit.
    assert_eq!(
        format_with_budget(&input, budget_with_margin(52)).unwrap(),
        indoc! {r#"
            <|file_sep|>file_b.rs
            high priority content
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_truncation_drops_high_order_excerpts_within_file() {
    // A single file has excerpts at order 1 and order 3. With a tight budget,
    // only the order-1 excerpts are included while the order-3 excerpt is
    // dropped — even though they belong to the same file. This also preserves
    // the parent invariant: parent outline items have order ≤ their best
    // child, so they're always included when any child is.
    let input = make_input(
        "x",
        0..1,
        0,
        vec![],
        vec![RelatedFile {
            path: Path::new("mod.rs").into(),
            max_row: 30,
            in_open_source_repo: false,
            excerpts: vec![
                RelatedExcerpt {
                    row_range: 0..5,
                    text: "mod header\n".into(),
                    order: 1,
                    context_source: ContextSource::Lsp,
                },
                RelatedExcerpt {
                    row_range: 6..15,
                    text: "important fn\n".into(),
                    order: 1,
                    context_source: ContextSource::Lsp,
                },
                RelatedExcerpt {
                    row_range: 16..30,
                    text: "less important fn\n".into(),
                    order: 3,
                    context_source: ContextSource::Lsp,
                },
            ],
        }],
    );

    // With large budget, all three excerpts included.
    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>mod.rs
            mod header
            ...
            important fn
            ...
            less important fn
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );

    // With tight budget, only order<=1 excerpts included (header + important fn).
    assert_eq!(
        format_with_budget(&input, budget_with_margin(55)).unwrap(),
        indoc! {r#"
            <|file_sep|>mod.rs
            mod header
            ...
            important fn
            ...
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_truncation_drops_older_events_first() {
    let input = make_input(
        "x",
        0..1,
        0,
        vec![make_event("old.rs", "-1\n"), make_event("new.rs", "-2\n")],
        vec![],
    );

    assert_eq!(
        format_with_budget(&input, 10000).unwrap(),
        indoc! {r#"
            <|file_sep|>edit history
            --- a/old.rs
            +++ b/old.rs
            -1
            --- a/new.rs
            +++ b/new.rs
            -2
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );

    assert_eq!(
        format_with_budget(&input, 60).unwrap(),
        indoc! {r#"
            <|file_sep|>edit history
            --- a/new.rs
            +++ b/new.rs
            -2
            <|file_sep|>test.rs
            <|fim_prefix|>
            <|fim_middle|>current
            <|user_cursor|>x
            <|fim_suffix|>
            <|fim_middle|>updated
        "#}
        .to_string()
    );
}

#[test]
fn test_cursor_excerpt_always_included_with_minimal_budget() {
    let input = make_input(
        "fn main() {}",
        0..12,
        3,
        vec![make_event("a.rs", "-old\n+new\n")],
        vec![make_related_file("related.rs", "helper\n")],
    );

    assert!(format_with_budget(&input, 30).is_none())
}
