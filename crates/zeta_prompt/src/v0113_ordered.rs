use super::*;

pub fn special_tokens() -> &'static [&'static str] {
    &[
        "<|fim_prefix|>",
        "<|fim_suffix|>",
        "<|fim_middle|>",
        "<|file_sep|>",
        CURSOR_MARKER,
    ]
}

pub fn write_cursor_excerpt_section(
    prompt: &mut String,
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) {
    let path_str = path.to_string_lossy();
    write!(prompt, "<|file_sep|>{}\n", path_str).ok();

    prompt.push_str("<|fim_prefix|>\n");
    prompt.push_str(&context[..editable_range.start]);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }

    prompt.push_str("<|fim_middle|>current\n");
    prompt.push_str(&context[editable_range.start..cursor_offset]);
    prompt.push_str(CURSOR_MARKER);
    prompt.push_str(&context[cursor_offset..editable_range.end]);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }

    prompt.push_str("<|fim_suffix|>\n");
    prompt.push_str(&context[editable_range.end..]);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }

    prompt.push_str("<|fim_middle|>updated\n");
}
