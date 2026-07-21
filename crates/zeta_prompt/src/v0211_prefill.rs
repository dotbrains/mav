use super::*;

pub fn special_tokens() -> &'static [&'static str] {
    v0131_git_merge_markers_prefix::special_tokens()
}

pub fn get_prefill(context: &str, editable_range: &Range<usize>) -> String {
    let editable_region = &context[editable_range.start..editable_range.end];

    let prefill_len = (editable_region.len() as f64 * PREFILL_RATIO) as usize;
    let prefill_len = editable_region.floor_char_boundary(prefill_len);

    // Find a token boundary to avoid splitting tokens in the prefill.
    // In Qwen2.5-Coder, \n is always the END of a token (e.g. `;\n`,
    // ` {\n`), and \n\n / \n\n\n are single tokens, so we must include
    // the \n and consume any consecutive \n characters after it.
    let prefill = &editable_region[..prefill_len];
    match prefill.rfind('\n') {
        Some(pos) => {
            let mut end = pos + 1;
            while end < editable_region.len() && editable_region.as_bytes().get(end) == Some(&b'\n')
            {
                end += 1;
            }
            editable_region[..end].to_string()
        }
        // No newline found. Fall back to splitting before the last space
        // (word-level boundary)
        None => match prefill.rfind(' ') {
            Some(pos) => prefill[..pos].to_string(),
            None => prefill.to_string(),
        },
    }
}
