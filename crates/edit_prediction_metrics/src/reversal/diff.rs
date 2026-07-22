use std::iter;
use std::ops::Range;
use std::sync::Arc;

use crate::tokenize::tokenize;
use imara_diff::{
    Algorithm, diff,
    intern::{InternedInput, Token},
    sources::lines_with_terminator,
};

pub(super) fn text_diff(old_text: &str, new_text: &str) -> Vec<(Range<usize>, Arc<str>)> {
    let empty: Arc<str> = Arc::default();
    let mut edits = Vec::new();
    let mut hunk_input = InternedInput::default();
    let input = InternedInput::new(
        lines_with_terminator(old_text),
        lines_with_terminator(new_text),
    );

    diff_internal(&input, &mut |old_byte_range,
                                new_byte_range,
                                old_rows,
                                new_rows| {
        if should_perform_token_diff_within_hunk(
            &old_byte_range,
            &new_byte_range,
            &old_rows,
            &new_rows,
        ) {
            let old_offset = old_byte_range.start;
            let new_offset = new_byte_range.start;
            hunk_input.clear();
            hunk_input.update_before(tokenize(&old_text[old_byte_range]).into_iter());
            hunk_input.update_after(tokenize(&new_text[new_byte_range]).into_iter());
            diff_internal(&hunk_input, &mut |old_byte_range, new_byte_range, _, _| {
                let old_byte_range =
                    old_offset + old_byte_range.start..old_offset + old_byte_range.end;
                let new_byte_range =
                    new_offset + new_byte_range.start..new_offset + new_byte_range.end;
                let replacement_text = if new_byte_range.is_empty() {
                    empty.clone()
                } else {
                    new_text[new_byte_range].into()
                };
                edits.push((old_byte_range, replacement_text));
            });
        } else {
            let replacement_text = if new_byte_range.is_empty() {
                empty.clone()
            } else {
                new_text[new_byte_range].into()
            };
            edits.push((old_byte_range, replacement_text));
        }
    });

    edits
}

pub(super) fn char_diff<'a>(old_text: &'a str, new_text: &'a str) -> Vec<(Range<usize>, &'a str)> {
    let mut input: InternedInput<&str> = InternedInput::default();
    input.update_before(tokenize_chars(old_text));
    input.update_after(tokenize_chars(new_text));
    let mut edits = Vec::new();

    diff_internal(&input, &mut |old_byte_range, new_byte_range, _, _| {
        let replacement = if new_byte_range.is_empty() {
            ""
        } else {
            &new_text[new_byte_range]
        };
        edits.push((old_byte_range, replacement));
    });

    edits
}

fn should_perform_token_diff_within_hunk(
    old_byte_range: &Range<usize>,
    new_byte_range: &Range<usize>,
    old_row_range: &Range<u32>,
    new_row_range: &Range<u32>,
) -> bool {
    const MAX_TOKEN_DIFF_LEN: usize = 512;
    const MAX_TOKEN_DIFF_LINE_COUNT: usize = 8;

    !old_byte_range.is_empty()
        && !new_byte_range.is_empty()
        && old_byte_range.len() <= MAX_TOKEN_DIFF_LEN
        && new_byte_range.len() <= MAX_TOKEN_DIFF_LEN
        && old_row_range.len() <= MAX_TOKEN_DIFF_LINE_COUNT
        && new_row_range.len() <= MAX_TOKEN_DIFF_LINE_COUNT
}

fn diff_internal(
    input: &InternedInput<&str>,
    on_change: &mut dyn FnMut(Range<usize>, Range<usize>, Range<u32>, Range<u32>),
) {
    let mut old_offset = 0;
    let mut new_offset = 0;
    let mut old_token_ix = 0;
    let mut new_token_ix = 0;

    diff(
        Algorithm::Histogram,
        input,
        |old_tokens: Range<u32>, new_tokens: Range<u32>| {
            old_offset += token_len(
                input,
                &input.before[old_token_ix as usize..old_tokens.start as usize],
            );
            new_offset += token_len(
                input,
                &input.after[new_token_ix as usize..new_tokens.start as usize],
            );
            let old_len = token_len(
                input,
                &input.before[old_tokens.start as usize..old_tokens.end as usize],
            );
            let new_len = token_len(
                input,
                &input.after[new_tokens.start as usize..new_tokens.end as usize],
            );
            let old_byte_range = old_offset..old_offset + old_len;
            let new_byte_range = new_offset..new_offset + new_len;
            old_token_ix = old_tokens.end;
            new_token_ix = new_tokens.end;
            old_offset = old_byte_range.end;
            new_offset = new_byte_range.end;
            on_change(old_byte_range, new_byte_range, old_tokens, new_tokens);
        },
    );
}

fn tokenize_chars(text: &str) -> impl Iterator<Item = &str> {
    let mut chars = text.char_indices();
    iter::from_fn(move || {
        let (start, character) = chars.next()?;
        Some(&text[start..start + character.len_utf8()])
    })
}

fn token_len(input: &InternedInput<&str>, tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|token| input.interner[*token].len())
        .sum()
}
