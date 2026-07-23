use super::MarkdownEvent::*;
use super::MarkdownTag::*;
use super::*;

#[test]
fn test_code_block_metadata() {
    assert_eq!(
        parse_markdown_with_options(
            "```rust\nfn main() {\n let a = 1;\n}\n```",
            false,
            false,
            false
        ),
        ParsedMarkdownData {
            events: vec![
                (0..37, RootStart),
                (
                    0..37,
                    Start(CodeBlock {
                        kind: CodeBlockKind::FencedLang("rust".into()),
                        metadata: CodeBlockMetadata {
                            content_range: 8..34,
                            line_count: 3,
                            is_fenced_closed: true,
                        }
                    })
                ),
                (8..34, Text),
                (0..37, End(MarkdownTagEnd::CodeBlock)),
                (0..37, RootEnd(0)),
            ],
            language_names: {
                let mut h = HashSet::default();
                h.insert("rust".into());
                h
            },
            root_block_starts: vec![0],
            ..Default::default()
        }
    );
    assert_eq!(
        parse_markdown_with_options("    fn main() {}", false, false, false),
        ParsedMarkdownData {
            events: vec![
                (4..16, RootStart),
                (
                    4..16,
                    Start(CodeBlock {
                        kind: CodeBlockKind::Indented,
                        metadata: CodeBlockMetadata {
                            content_range: 4..16,
                            line_count: 1,
                            is_fenced_closed: false,
                        }
                    })
                ),
                (4..16, Text),
                (4..16, End(MarkdownTagEnd::CodeBlock)),
                (4..16, RootEnd(0)),
            ],
            root_block_starts: vec![4],
            ..Default::default()
        }
    );
}

fn assert_code_block_does_not_emit_links(markdown: &str) {
    let parsed = parse_markdown_with_options(markdown, false, false, false);
    let mut code_block_depth = 0;
    let mut code_block_count = 0;
    let mut saw_text_inside_code_block = false;

    for (_, event) in &parsed.events {
        match event {
            Start(CodeBlock { .. }) => {
                code_block_depth += 1;
                code_block_count += 1;
            }
            End(MarkdownTagEnd::CodeBlock) => {
                assert!(
                    code_block_depth > 0,
                    "encountered a code block end without a matching start"
                );
                code_block_depth -= 1;
            }
            Start(Link { .. }) | End(MarkdownTagEnd::Link) => {
                assert_eq!(
                    code_block_depth, 0,
                    "code blocks should not emit link events"
                );
            }
            Text | SubstitutedText(_) if code_block_depth > 0 => {
                saw_text_inside_code_block = true;
            }
            _ => {}
        }
    }

    assert_eq!(code_block_count, 1, "expected exactly one code block");
    assert_eq!(code_block_depth, 0, "unterminated code block");
    assert!(
        saw_text_inside_code_block,
        "expected text inside the code block"
    );
}

#[test]
fn test_code_blocks_do_not_autolink_urls() {
    assert_code_block_does_not_emit_links("```txt\nhttps://example.com\n```");
    assert_code_block_does_not_emit_links("    https://example.com");
    assert_code_block_does_not_emit_links(
        "```txt\r\nhttps:/\\/example.com\r\nhttps://example&#46;com\r\n```",
    );
    assert_code_block_does_not_emit_links(
        "    https:/\\/example.com\r\n    https://example&#46;com",
    );
}

#[test]
fn test_metadata_blocks_are_root_blocks() {
    assert_eq!(
        parse_markdown_with_options(
            "+++\ntitle = \"Example\"\n+++\n\nParagraph",
            false,
            false,
            true
        ),
        ParsedMarkdownData {
            events: vec![
                (0..25, RootStart),
                (0..25, Start(MetadataBlock(MetadataBlockKind::PlusesStyle))),
                (4..22, Text),
                (
                    0..25,
                    End(MarkdownTagEnd::MetadataBlock(
                        MetadataBlockKind::PlusesStyle
                    ))
                ),
                (0..25, RootEnd(0)),
                (27..36, RootStart),
                (27..36, Start(Paragraph)),
                (27..36, Text),
                (27..36, End(MarkdownTagEnd::Paragraph)),
                (27..36, RootEnd(1)),
            ],
            root_block_starts: vec![0, 27],
            metadata_blocks: BTreeMap::from_iter([(
                0,
                ParsedMetadataBlock {
                    content_range: 4..22,
                    rows: None,
                },
            )]),
            ..Default::default()
        }
    );
}

#[test]
fn test_metadata_blocks_are_omitted_by_default() {
    assert_eq!(
        parse_markdown_with_options(
            "+++\ntitle = \"Example\"\n+++\n\nParagraph",
            false,
            false,
            false
        ),
        ParsedMarkdownData {
            events: vec![
                (27..36, RootStart),
                (27..36, Start(Paragraph)),
                (27..36, Text),
                (27..36, End(MarkdownTagEnd::Paragraph)),
                (27..36, RootEnd(0)),
            ],
            root_block_starts: vec![27],
            ..Default::default()
        }
    );
}

#[test]
fn test_table_checkboxes_remain_text_in_cells() {
    let markdown = "\
| Done | Task    |
|------|---------|
| [x]  | Fix bug |
| [ ]  | Add feature |";
    let parsed = parse_markdown_with_options(markdown, false, false, false);

    let mut in_table = false;
    let mut saw_task_list_marker = false;
    let mut cell_texts = Vec::new();
    let mut current_cell = String::new();

    for (range, event) in &parsed.events {
        match event {
            Start(Table(_)) => in_table = true,
            End(MarkdownTagEnd::Table) => in_table = false,
            Start(TableCell) => current_cell.clear(),
            End(MarkdownTagEnd::TableCell) => {
                if in_table {
                    cell_texts.push(current_cell.clone());
                }
            }
            Text if in_table => current_cell.push_str(&markdown[range.clone()]),
            TaskListMarker(_) if in_table => saw_task_list_marker = true,
            _ => {}
        }
    }

    let checkbox_cells: Vec<&str> = cell_texts
        .iter()
        .map(|cell| cell.trim())
        .filter(|cell| *cell == "[x]" || *cell == "[X]" || *cell == "[ ]")
        .collect();

    assert!(
        !saw_task_list_marker,
        "Table checkboxes should remain text, not task-list markers"
    );
    assert_eq!(checkbox_cells, vec!["[x]", "[ ]"]);
}

#[test]
fn test_extract_code_content_range() {
    let input = "```let x = 5;```";
    assert_eq!(extract_code_content_range(input), 3..13);

    let input = "``let x = 5;``";
    assert_eq!(extract_code_content_range(input), 2..12);

    let input = "`let x = 5;`";
    assert_eq!(extract_code_content_range(input), 1..11);

    let input = "plain text";
    assert_eq!(extract_code_content_range(input), 0..10);

    let input = "``let x = 5;`";
    assert_eq!(extract_code_content_range(input), 0..13);
}

#[test]
fn test_inline_code_substitutes_escaped_pipes() {
    let markdown = r"| Pattern |
| --- |
| `a\|b` |";
    let parsed = parse_markdown_with_options(markdown, false, false, false);
    let code_range = {
        let start = markdown.find(r"a\|b").expect("inline code source");
        start..start + r"a\|b".len()
    };

    assert!(
        parsed
            .events
            .iter()
            .any(|(range, event)| range == &code_range && event == &SubstitutedCode("a|b".into())),
        "expected escaped pipe in table inline code to render as decoded inline code: {:?}",
        parsed.events
    );
}

#[test]
fn test_inline_code_keeps_escaped_pipes_outside_tables() {
    let markdown = r"`a\|b`";
    let parsed = parse_markdown_with_options(markdown, false, false, false);

    assert!(
        parsed
            .events
            .iter()
            .any(|(range, event)| range == &(1..5) && event == &Code),
        "expected escaped pipe outside a table to remain normal inline code: {:?}",
        parsed.events
    );
}

#[test]
fn test_extract_code_block_content_range() {
    let input = "```rust\nlet x = 5;\n```";
    assert_eq!(extract_code_block_content_range(input), 8..19);

    let input = "plain text";
    assert_eq!(extract_code_block_content_range(input), 0..10);

    let input = "```python\nprint('hello')\nprint('world')\n```";
    assert_eq!(extract_code_block_content_range(input), 10..40);

    // Malformed input
    let input = "`````";
    assert_eq!(extract_code_block_content_range(input), 3..3);
}
