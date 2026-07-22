use language_model::LanguageModelToolResultContent;

pub(super) fn tool_content_err(e: impl std::fmt::Display) -> LanguageModelToolResultContent {
    LanguageModelToolResultContent::from(e.to_string())
}

/// Resolves the optional `start_line` / `end_line` inputs from the tool schema
/// to a concrete 1-indexed, inclusive `(start, end)` line range:
///
/// - `start` defaults to 1 and is clamped to `>= 1` (the model occasionally passes
///   `0` despite instructions to be 1-indexed).
/// - `end` defaults to `u32::MAX` and is clamped to `>= start`, so callers always
///   read at least one line even when the model passes `end < start`.
///
/// Callers translate this 1-indexed inclusive range to whichever coordinate
/// system their slicing API wants (e.g. 0-indexed exclusive row ranges for
/// `Buffer::text_for_range`).
pub(super) fn resolve_line_range(start_line: Option<u32>, end_line: Option<u32>) -> (u32, u32) {
    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(u32::MAX).max(start);
    (start, end)
}

/// Prefixes each line of `text` with its line number in `cat -n` format:
/// the line number is right-aligned in a 6-character field, followed by a
/// single tab, followed by the line's original content (including its
/// trailing newline if present). Numbering starts at `start_line`.
///
/// This format matches what the model expects in the edit tool, where the
/// line number prefix is `line number + tab` and everything after the tab is
/// the actual file content to match.
pub(super) fn format_with_line_numbers(text: &str, start_line: u32) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut output = String::with_capacity(text.len() + text.len() / 4);
    write_lines_numbered(&mut output, std::iter::once(text), start_line);
    output
}

/// Streams `cat -n`-style line-numbered output directly into `output` from an
/// iterator of string slices. Chunks do not need to align to line boundaries:
/// a single chunk may contain multiple newlines, span multiple lines, or end
/// mid-line. This lets callers consume `Buffer::text_for_range`'s `Chunks`
/// iterator without materializing the unnumbered text first.
pub(super) fn write_lines_numbered<'a>(
    output: &mut String,
    chunks: impl IntoIterator<Item = &'a str>,
    start_line: u32,
) {
    use std::fmt::Write as _;

    let mut line_number = start_line;
    let mut at_line_start = true;
    for chunk in chunks {
        let mut rest = chunk;
        while !rest.is_empty() {
            if at_line_start {
                // Writes to a `String` are infallible, so the `Result` can be ignored.
                let _ = write!(output, "{line_number:>6}\t");
                at_line_start = false;
            }
            match rest.find('\n') {
                Some(nl) => {
                    let (head, tail) = rest.split_at(nl + 1);
                    output.push_str(head);
                    line_number = line_number.saturating_add(1);
                    at_line_start = true;
                    rest = tail;
                }
                None => {
                    output.push_str(rest);
                    break;
                }
            }
        }
    }
}
