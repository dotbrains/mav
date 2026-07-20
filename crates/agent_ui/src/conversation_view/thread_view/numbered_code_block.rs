use super::*;

/// `cat -n`-style numbered code block, already stripped of its line-number
/// prefixes and ready to render. Line numbers are guaranteed to be contiguous
/// starting at `first_number`, so we only store the first number and the line
/// count rather than allocating a per-line `Vec`.
pub(super) struct ParsedCatNumberedCode {
    code: String,
    first_number: u32,
    line_count: usize,
}

pub(super) fn parse_cat_numbered_markdown_code_block(
    markdown: &str,
) -> Option<ParsedCatNumberedCode> {
    let (_tag, code) = parse_single_fenced_code_block(markdown)?;
    parse_cat_numbered_code(code)
}

fn parse_single_fenced_code_block(markdown: &str) -> Option<(&str, &str)> {
    let first_non_backtick = markdown.find(|character| character != '`')?;
    if first_non_backtick < 3 {
        return None;
    }

    let fence = &markdown[..first_non_backtick];
    let after_opening_fence = &markdown[first_non_backtick..];
    let tag_end = after_opening_fence.find('\n')?;
    let tag = &after_opening_fence[..tag_end];
    let after_tag = &after_opening_fence[tag_end + 1..];
    let closing_fence = format!("\n{fence}\n");
    let code = after_tag.strip_suffix(&closing_fence)?;
    Some((tag, code))
}

fn parse_cat_numbered_code(code: &str) -> Option<ParsedCatNumberedCode> {
    if code.is_empty() {
        return None;
    }

    let mut output = String::with_capacity(code.len());
    let mut first_number = None;
    let mut expected_number = None;
    let mut line_count: usize = 0;
    for raw_line in code.split_inclusive('\n') {
        let line = strip_line_ending(raw_line);
        let (number, text) = parse_cat_numbered_line(line)?;
        if let Some(expected) = expected_number {
            if number != expected {
                return None;
            }
        } else {
            first_number = Some(number);
        }
        expected_number = number.checked_add(1);
        if line_count > 0 {
            output.push('\n');
        }
        output.push_str(text);
        line_count += 1;
    }

    Some(ParsedCatNumberedCode {
        code: output,
        first_number: first_number?,
        line_count,
    })
}

fn strip_line_ending(line: &str) -> &str {
    let without_lf = line.strip_suffix('\n').unwrap_or(line);
    without_lf.strip_suffix('\r').unwrap_or(without_lf)
}

fn parse_cat_numbered_line(line: &str) -> Option<(u32, &str)> {
    let (prefix, text) = line.split_once('\t')?;
    let number = prefix.trim();
    if number.is_empty()
        || !prefix
            .chars()
            .all(|character| character == ' ' || character.is_ascii_digit())
    {
        return None;
    }

    Some((number.parse().ok()?, text))
}

pub(super) fn render_cat_numbered_code_block(
    parsed: ParsedCatNumberedCode,
    language: Option<Arc<Language>>,
    markdown_style: MarkdownStyle,
    copy_button_id: String,
    cx: &App,
) -> AnyElement {
    use std::fmt::Write as _;

    let ParsedCatNumberedCode {
        code,
        first_number,
        line_count,
    } = parsed;
    let last_number = first_number
        .saturating_add(u32::try_from(line_count.saturating_sub(1)).unwrap_or(u32::MAX));
    let gutter_width = last_number.to_string().len().max(1);
    let gutter_capacity = line_count * gutter_width + line_count.saturating_sub(1);

    let mut gutter = String::with_capacity(gutter_capacity);
    for i in 0..line_count {
        if i > 0 {
            gutter.push('\n');
        }
        let line_number = first_number.saturating_add(u32::try_from(i).unwrap_or(u32::MAX));
        let _ = write!(&mut gutter, "{line_number:>gutter_width$}");
    }

    let mut code_text_style = markdown_style.base_text_style.clone();
    code_text_style.refine(&markdown_style.code_block.text);

    let mut gutter_text_style = code_text_style.clone();
    gutter_text_style.color = cx.theme().colors().text_muted;

    let gutter_len = gutter.len();
    let gutter = StyledText::new(gutter).with_runs(vec![gutter_text_style.to_run(gutter_len)]);

    let code: SharedString = code.into();
    let code_runs = highlight_code_runs(&code, language.as_ref(), code_text_style, &markdown_style);
    let code_text = StyledText::new(code.clone()).with_runs(code_runs);

    let code_block_id = format!("read-file-code-block-{copy_button_id}");
    let code_scroll_id = format!("read-file-code-scroll-{copy_button_id}");
    let mut container = div()
        .id(code_block_id)
        .group("read-file-code-block")
        .relative()
        .w_full()
        .whitespace_nowrap();
    container.style().refine(&markdown_style.code_block);

    let mut code_scroll = div()
        .id(code_scroll_id)
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_x_scroll()
        .child(div().flex_none().child(code_text));
    code_scroll.style().restrict_scroll_to_axis = Some(true);

    container
        .child(
            h_flex()
                .items_start()
                .min_w_0()
                .w_full()
                .child(div().flex_none().pr_3().child(gutter))
                .child(code_scroll),
        )
        .child(
            h_flex()
                .w_4()
                .absolute()
                .top_0()
                .right_0()
                .justify_end()
                .visible_on_hover("read-file-code-block")
                .child(CopyButton::new(copy_button_id, code).tooltip_label("Copy Code")),
        )
        .into_any_element()
}

fn highlight_code_runs(
    code: &str,
    language: Option<&Arc<Language>>,
    code_text_style: TextStyle,
    markdown_style: &MarkdownStyle,
) -> Vec<TextRun> {
    if code.is_empty() {
        return Vec::new();
    }

    let Some(language) = language else {
        return vec![code_text_style.to_run(code.len())];
    };

    let mut runs = Vec::new();
    let mut offset = 0;
    for (range, highlight_id) in language.highlight_text(&Rope::from(code), 0..code.len()) {
        if range.start > offset {
            runs.push(code_text_style.to_run(range.start - offset));
        }

        let mut run_style = code_text_style.clone();
        if let Some(highlight) = markdown_style.syntax.get(highlight_id).cloned() {
            run_style = run_style.highlight(highlight);
        }
        runs.push(run_style.to_run(range.len()));
        offset = range.end;
    }

    if offset < code.len() {
        runs.push(code_text_style.to_run(code.len() - offset));
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cat_numbered_markdown_code_block() {
        let parsed = parse_cat_numbered_markdown_code_block(
            "```rs mav/crates/example.rs\n     2\tfn main() {\n     3\t    println!(\"hi\");\n     4\t}\n```\n",
        )
        .expect("cat-numbered block should parse");

        assert_eq!(parsed.line_count, 3);
        assert_eq!(parsed.first_number, 2);
        assert_eq!(parsed.code, "fn main() {\n    println!(\"hi\");\n}");
    }

    #[test]
    fn parses_cat_numbered_code_with_crlf_line_endings() {
        let parsed = parse_cat_numbered_code("     1\tline one\r\n     2\tline two\r\n")
            .expect("crlf-terminated cat-numbered code should parse");

        assert_eq!(parsed.line_count, 2);
        assert_eq!(parsed.first_number, 1);
        assert_eq!(parsed.code, "line one\nline two");
    }

    #[test]
    fn rejects_non_cat_numbered_code_block() {
        assert!(parse_cat_numbered_markdown_code_block("```rs\nfn main() {}\n```\n").is_none());
    }

    #[test]
    fn rejects_non_contiguous_cat_numbers() {
        assert!(
            parse_cat_numbered_markdown_code_block(
                "```rs\n     2\tlet a = 1;\n     4\tlet b = 2;\n```\n"
            )
            .is_none()
        );
    }
}
