use super::*;

#[gpui::test]
fn test_table_column_selection(cx: &mut TestAppContext) {
    let rendered = render_markdown("| a | b |\n|---|---|\n| c | d |", cx);

    assert!(rendered.lines.len() >= 2);
    let first_bounds = rendered.lines[0].layout.bounds();
    let second_bounds = rendered.lines[1].layout.bounds();

    let first_index = match rendered.source_index_for_position(first_bounds.center()) {
        Ok(index) | Err(index) => index,
    };
    let second_index = match rendered.source_index_for_position(second_bounds.center()) {
        Ok(index) | Err(index) => index,
    };

    let first_word = rendered.text_for_range(rendered.surrounding_word_range(first_index));
    let second_word = rendered.text_for_range(rendered.surrounding_word_range(second_index));

    assert_eq!(first_word, "a");
    assert_eq!(second_word, "b");
}

#[test]
fn test_table_state_current_cell_alignment_centers_headers() {
    let mut table = TableState::default();
    table.start(vec![Alignment::Left, Alignment::Right, Alignment::None]);

    table.start_head();
    for _ in 0..3 {
        assert_eq!(table.current_cell_alignment(), Some(Alignment::Center));
        table.end_cell();
    }

    table.end_head();
    table.start_row();
    assert_eq!(table.current_cell_alignment(), Some(Alignment::Left));
    table.end_cell();
    assert_eq!(table.current_cell_alignment(), Some(Alignment::Right));
    table.end_cell();
    assert_eq!(table.current_cell_alignment(), Some(Alignment::None));
    table.end_cell();
    table.end_row();

    table.end();
    assert_eq!(table.current_cell_alignment(), None);
}

#[test]
fn test_table_state_current_cell_alignment_outside_table() {
    let table = TableState::default();
    assert_eq!(table.current_cell_alignment(), None);
}

#[test]
fn test_table_checkbox_detection() {
    let md = "| Done |\n|------|\n| [x] |\n| [ ] |";
    let events = crate::parser::parse_markdown_with_options(md, false, false, false).events;

    let mut in_table = false;
    let mut cell_texts: Vec<String> = Vec::new();
    let mut current_cell = String::new();

    for (range, event) in &events {
        match event {
            MarkdownEvent::Start(MarkdownTag::Table(_)) => in_table = true,
            MarkdownEvent::End(MarkdownTagEnd::Table) => in_table = false,
            MarkdownEvent::Start(MarkdownTag::TableCell) => current_cell.clear(),
            MarkdownEvent::End(MarkdownTagEnd::TableCell) => {
                if in_table {
                    cell_texts.push(current_cell.clone());
                }
            }
            MarkdownEvent::Text if in_table => {
                current_cell.push_str(&md[range.clone()]);
            }
            _ => {}
        }
    }

    let checkbox_cells: Vec<&String> = cell_texts
        .iter()
        .filter(|t| {
            let trimmed = t.trim();
            trimmed == "[x]" || trimmed == "[X]" || trimmed == "[ ]"
        })
        .collect();
    assert_eq!(
        checkbox_cells.len(),
        2,
        "Expected 2 checkbox cells, got: {cell_texts:?}"
    );
    assert_eq!(checkbox_cells[0].trim(), "[x]");
    assert_eq!(checkbox_cells[1].trim(), "[ ]");
}

#[test]
fn test_table_checkbox_marker_source_range() {
    let md = "| Done |\n|------|\n|  [x]  |\n| [ ] |";
    let events = crate::parser::parse_markdown_with_options(md, false, false, false).events;

    let mut in_cell = false;
    let mut pending_text = String::new();
    let mut mappings: Vec<SourceMapping> = Vec::new();
    let mut cell_ranges: Vec<Range<usize>> = Vec::new();

    for (range, event) in &events {
        match event {
            MarkdownEvent::Start(MarkdownTag::TableCell) => {
                in_cell = true;
                pending_text.clear();
                mappings.clear();
            }
            MarkdownEvent::End(MarkdownTagEnd::TableCell) => {
                if in_cell {
                    let trimmed = pending_text.trim();
                    if trimmed == "[x]" || trimmed == "[X]" || trimmed == "[ ]" {
                        let leading = pending_text.len() - pending_text.trim_start().len();
                        let rendered = leading..leading + trimmed.len();
                        let marker_source = source_range_for_rendered(&mappings, &rendered)
                            .expect("marker source range");
                        cell_ranges.push(marker_source);
                    }
                }
                in_cell = false;
            }
            MarkdownEvent::Text if in_cell => {
                mappings.push(SourceMapping {
                    rendered_index: pending_text.len(),
                    source_index: range.start,
                });
                pending_text.push_str(&md[range.clone()]);
            }
            _ => {}
        }
    }

    assert_eq!(cell_ranges.len(), 2);
    for marker_range in &cell_ranges {
        let slice = &md[marker_range.clone()];
        assert!(
            slice == "[x]" || slice == "[X]" || slice == "[ ]",
            "expected `[x]`/`[X]`/`[ ]`, got {slice:?} at {marker_range:?}"
        );
    }
}

#[gpui::test]
fn test_escaped_pipes_in_inline_code_inside_tables(cx: &mut TestAppContext) {
    let markdown = "\
| Pattern | What it does |
| --- | --- |
| `^echo(\\s\\|$)` | command pattern |
| `a\\|b` | alternation |
| `(a\\|b)` | grouped alternation |
| `a\\|\\|b` | empty middle alternative |";
    let rendered = render_markdown(markdown, cx);
    let text = rendered.text_for_range(0..markdown.len());

    assert_eq!(
        text,
        "Pattern\n\
             What it does\n\
             ^echo(\\s|$)\n\
             command pattern\n\
             a|b\n\
             alternation\n\
             (a|b)\n\
             grouped alternation\n\
             a||b\n\
             empty middle alternative"
    );
}

#[test]
fn test_escape_plain_text() {
    assert_eq!(Markdown::escape("hello world"), "hello world");
    assert_eq!(Markdown::escape(""), "");
    assert_eq!(Markdown::escape("café ☕ naïve"), "café ☕ naïve");
}

#[test]
fn test_escape_punctuation() {
    assert_eq!(Markdown::escape("hello `world`"), r"hello \`world\`");
    assert_eq!(Markdown::escape("a|b"), r"a\|b");
}

#[test]
fn test_escape_leading_spaces() {
    assert_eq!(Markdown::escape("    hello"), [&nbsp(4), "hello"].concat());
    assert_eq!(
        Markdown::escape("    | { a: string }"),
        [&nbsp(4), r"\| \{ a\: string \}"].concat()
    );
    assert_eq!(
        Markdown::escape("  first\n  second"),
        [&nbsp(2), "first\n\n", &nbsp(2), "second"].concat()
    );
    assert_eq!(Markdown::escape("hello   world"), "hello   world");
}

#[test]
fn test_escape_leading_tabs() {
    assert_eq!(Markdown::escape("\thello"), [&nbsp(4), "hello"].concat());
    assert_eq!(
        Markdown::escape("hello\n\t\tindented"),
        ["hello\n\n", &nbsp(8), "indented"].concat()
    );
    assert_eq!(
        Markdown::escape(" \t hello"),
        [&nbsp(1 + 4 + 1), "hello"].concat()
    );
    assert_eq!(Markdown::escape("hello\tworld"), "hello\tworld");
}

#[test]
fn test_escape_newlines() {
    assert_eq!(Markdown::escape("a\nb"), "a\n\nb");
    assert_eq!(Markdown::escape("a\n\nb"), "a\n\n\n\nb");
    assert_eq!(Markdown::escape("\nhello"), "\n\nhello");
}

#[test]
fn test_escape_multiline_diagnostic() {
    assert_eq!(
        Markdown::escape("    | { a: string }\n    | { b: number }"),
        [
            &nbsp(4),
            r"\| \{ a\: string \}",
            "\n\n",
            &nbsp(4),
            r"\| \{ b\: number \}",
        ]
        .concat()
    );
}

#[test]
fn test_escape_non_ascii() {
    // Cyrillic characters should not have backslashes added before them,
    // but ASCII punctuation should still be escaped.
    assert_eq!(Markdown::escape("Привет, мир"), r"Привет\, мир");
    // Test with markdown special characters mixed in
    assert_eq!(Markdown::escape("Привет, *мир*"), r"Привет\, \*мир\*");
    // Test with the exact example from the issue (single quotes are also ASCII punctuation)
    assert_eq!(
        Markdown::escape("Отсутствует пробел справа от ','"),
        r"Отсутствует пробел справа от \'\,\'"
    );
    // Test more non-ASCII scripts
    assert_eq!(
        Markdown::escape("こんにちは *world*"),
        r"こんにちは \*world\*"
    );
    assert_eq!(Markdown::escape("العربيّة [link]"), r"العربيّة \[link\]");
    assert_eq!(Markdown::escape("Ελληνικά _text_"), r"Ελληνικά \_text\_");
    assert_eq!(Markdown::escape("עברית `code`"), r"עברית \`code\`");
    // Non-ASCII followed by ASCII punctuation
    assert_eq!(Markdown::escape("Test: тест"), r"Test\: тест");
}

#[test]
fn test_escape_output_len_matches_precomputed() {
    let cases = [
        "",
        "hello world",
        "hello `world`",
        "    hello",
        "    | { a: string }",
        "\thello",
        "hello\n\t\tindented",
        " \t hello",
        "hello\tworld",
        "a\nb",
        "a\n\nb",
        "\nhello",
        "    | { a: string }\n    | { b: number }",
        "café ☕ naïve",
    ];
    for input in cases {
        let mut escaper = MarkdownEscaper::new();
        let precomputed: usize = input.chars().map(|c| escaper.next(c).output_len(c)).sum();

        let mut escaper = MarkdownEscaper::new();
        let mut output = String::new();
        for c in input.chars() {
            escaper.next(c).write_to(c, &mut output);
        }

        assert_eq!(precomputed, output.len(), "length mismatch for {:?}", input);
    }
}

#[test]
fn test_escape_prevents_code_block() {
    let diagnostic = "    | { a: string }";
    assert!(has_code_block(diagnostic));
    assert!(!has_code_block(&Markdown::escape(diagnostic)));
}
