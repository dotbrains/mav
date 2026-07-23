use std::ops::Range;

pub(super) fn extract_code_content_range(text: &str) -> Range<usize> {
    let text_len = text.len();
    if text_len == 0 {
        return 0..0;
    }

    let start_ticks = text.chars().take_while(|&c| c == '`').count();

    if start_ticks == 0 || start_ticks > text_len {
        return 0..text_len;
    }

    let end_ticks = text.chars().rev().take_while(|&c| c == '`').count();

    if end_ticks != start_ticks || text_len < start_ticks + end_ticks {
        return 0..text_len;
    }

    start_ticks..text_len - end_ticks
}

pub(crate) fn extract_code_block_content_range(text: &str) -> Range<usize> {
    let mut range = 0..text.len();
    if text.starts_with("```") {
        range.start += 3;

        if let Some(newline_ix) = text[range.clone()].find('\n') {
            range.start += newline_ix + 1;
        }
    }

    if !range.is_empty() && text.ends_with("```") {
        range.end -= 3;
    }
    if range.start > range.end {
        range.end = range.start;
    }
    range
}
