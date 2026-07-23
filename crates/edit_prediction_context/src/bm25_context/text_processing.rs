use std::{collections::HashMap, ops::Range};

pub(super) fn add_term_frequencies(
    term_frequencies: &mut HashMap<String, usize>,
    tokens: Vec<String>,
    weight: usize,
) {
    for token in tokens {
        *term_frequencies.entry(token).or_default() += weight;
    }
}

pub(super) fn chunk_line_ranges(
    lines: &[&str],
    target_line_count: usize,
    overlap_line_count: usize,
) -> Vec<Range<usize>> {
    if lines.is_empty() || target_line_count == 0 {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let ideal_end = start.saturating_add(target_line_count).min(lines.len());
        let mut end = ideal_end;
        if ideal_end < lines.len()
            && let Some(boundary) =
                empty_line_boundary_near(lines, start, ideal_end, overlap_line_count)
        {
            end = boundary;
        }
        if end <= start {
            end = ideal_end;
        }
        if end <= start {
            break;
        }

        ranges.push(start..end);
        if end == lines.len() {
            break;
        }

        let next_start = end.saturating_sub(overlap_line_count);
        start = if next_start <= start { end } else { next_start };
    }

    ranges
}

fn empty_line_boundary_near(
    lines: &[&str],
    start: usize,
    ideal_end: usize,
    overlap_line_count: usize,
) -> Option<usize> {
    let search_start = ideal_end.saturating_sub(overlap_line_count).max(start + 1);
    let search_end = ideal_end
        .saturating_add(overlap_line_count)
        .min(lines.len());

    (search_start..search_end)
        .filter(|row| lines[*row].trim().is_empty())
        .min_by_key(|row| row.abs_diff(ideal_end))
        .map(|row| row + 1)
}

pub(super) fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

pub(super) fn text_for_line_range(text: &str, range: Range<usize>) -> String {
    lines(text)
        .into_iter()
        .skip(range.start)
        .take(range.end.saturating_sub(range.start))
        .collect()
}

pub(super) fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut segment = String::new();

    for character in text.chars() {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            segment.push(character);
        } else {
            push_segment_tokens(&segment, &mut tokens);
            segment.clear();
        }
    }
    push_segment_tokens(&segment, &mut tokens);

    tokens
}

fn push_segment_tokens(segment: &str, tokens: &mut Vec<String>) {
    if segment.is_empty() {
        return;
    }

    let mut segment_tokens = Vec::new();
    push_token(segment, &mut segment_tokens);
    for part in segment.split(['_', '-']).filter(|part| !part.is_empty()) {
        push_token(part, &mut segment_tokens);
        for camel_part in camel_case_parts(part) {
            push_token(camel_part, &mut segment_tokens);
        }
    }

    let mut unique_segment_tokens = Vec::new();
    for token in segment_tokens {
        if !unique_segment_tokens.contains(&token) {
            unique_segment_tokens.push(token);
        }
    }
    tokens.extend(unique_segment_tokens);
}

fn camel_case_parts(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut previous = None;

    for (index, character) in text.char_indices() {
        if index > 0
            && character.is_uppercase()
            && previous
                .is_some_and(|previous: char| previous.is_lowercase() || previous.is_numeric())
        {
            parts.push(&text[start..index]);
            start = index;
        }
        previous = Some(character);
    }

    if start < text.len() {
        parts.push(&text[start..]);
    }

    parts
}

fn push_token(token: &str, tokens: &mut Vec<String>) {
    let token = token.to_lowercase();
    if token.len() <= 1
        || token.len() > 128
        || !token.chars().any(|character| character.is_alphabetic())
    {
        return;
    }
    tokens.push(token);
}
