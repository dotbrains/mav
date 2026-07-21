//! A prompt format with no fixed editable region. The entire context is shown
//! to the model, and it chooses which text to replace by outputting surrounding
//! context lines with `<|fim_middle|>` and `<|fim_suffix|>` delimiting the new
//! text.
//!
//! Example prompt:
//!
//! <|file_sep|>path/to/file.py
//! zero
//! one
//! two
//! three<|user_cursor|>
//! four
//! five
//! <|fim_prefix|>
//
//! Expected output (model generates):
//!
//! two
//! <|fim_middle|>
//! THREE
//! <|fim_suffix|>
//! four
//!
//! The output means: find "two\n...\nfour" in the context, and replace
//! everything between "two\n" and "four" with "THREE\n".

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
    cursor_offset: usize,
) {
    let path_str = path.to_string_lossy();
    write!(prompt, "<|file_sep|>{}\n", path_str).ok();

    prompt.push_str(&context[..cursor_offset]);
    prompt.push_str(CURSOR_MARKER);
    prompt.push_str(&context[cursor_offset..]);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }
    prompt.push_str("<|fim_prefix|>\n")
}

/// Apply a variable-edit model output to the original context text.
///
/// The model output has the form:
///
/// - prefix context lines
/// - `<|fim_middle|>`
/// - new text
/// - `<|fim_suffix|>`
/// - suffix context lines
///
/// We locate the prefix/suffix context lines in the original text and replace
/// everything between them with the new text.
pub fn apply_variable_edit(context: &str, model_output: &str) -> Result<(Range<usize>, String)> {
    let (prefix_context, rest) = model_output
        .split_once("<|fim_middle|>\n")
        .or_else(|| model_output.split_once("<|fim_middle|>"))
        .ok_or_else(|| anyhow::anyhow!("missing <|fim_middle|> in model output"))?;

    let (new_text, suffix_context) = rest
        .split_once("<|fim_suffix|>\n")
        .or_else(|| rest.split_once("<|fim_suffix|>"))
        .unwrap_or((rest, ""));

    let suffix_context = if prefix_context.is_empty() && !suffix_context.is_empty() {
        suffix_context.strip_prefix('\n').unwrap_or(suffix_context)
    } else {
        suffix_context
    };

    let prefix_offset = find_substring_at_line_boundary(context, prefix_context)
        .ok_or_else(|| anyhow!("could not locate prefix lines"))?
        + prefix_context.len();
    let suffix_offset = if suffix_context.is_empty() {
        context.len()
    } else {
        find_substring_at_line_boundary(&context[prefix_offset..], suffix_context)
            .ok_or_else(|| anyhow!("could not locate suffix lines"))?
            + prefix_offset
    };

    let edit_range = prefix_offset..suffix_offset;
    return Ok((edit_range, new_text.to_string()));
}

fn find_substring_at_line_boundary(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }

    haystack.match_indices(needle).find_map(|(offset, _)| {
        let matched_line_start = offset == 0 || haystack[..offset].ends_with('\n');
        matched_line_start.then_some(offset)
    })
}

/// Convert a unified diff patch into the variable-edit output format.
///
/// Parses `patch` as a unified diff against `old_text` and produces model
/// output with context lines surrounding `<|fim_middle|>` / `<|fim_suffix|>`
/// delimiters. The diff is resolved by content matching rather than line
/// numbers.
pub fn patch_to_variable_edit_output(
    old_text: &str,
    patch: &str,
    cursor_offset: Option<usize>,
) -> Result<String> {
    // Parse the unified diff into hunks. Each hunk has an `old_context`
    // string (context + deleted lines interleaved in order) and a list of
    // edits expressed as byte ranges within that context plus replacement
    // text.
    let hunks = parse_hunks(patch);
    if hunks.is_empty() {
        return Ok(String::new());
    }

    // Apply each hunk by finding its old_context in the text and
    // performing the edits. We search forward from where the previous
    // hunk ended so that hunks are applied in order.
    let mut new_text = old_text.to_string();
    let mut search_from: usize = 0;
    let mut first_hunk_pos: Option<usize> = None;

    for hunk in &hunks {
        let context_pos = new_text[search_from..]
            .find(&hunk.old_context)
            .map(|pos| pos + search_from)
            .ok_or_else(|| anyhow::anyhow!("could not locate hunk context in text"))?;

        if first_hunk_pos.is_none() {
            first_hunk_pos = Some(context_pos);
        }

        // Apply edits in reverse order so byte offsets remain valid.
        for edit in hunk.edits.iter().rev() {
            let abs_start = context_pos + edit.range.start;
            let abs_end = context_pos + edit.range.end;
            new_text.replace_range(abs_start..abs_end, &edit.text);
        }

        // Advance past this hunk's region in the (now modified) text.
        let new_region_len: usize = hunk.edits.iter().fold(hunk.old_context.len(), |len, edit| {
            len + edit.text.len() - (edit.range.end - edit.range.start)
        });
        search_from = context_pos + new_region_len;
    }

    // Now we have old_text and new_text. Find the changed line range by
    // comparing them.
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();

    // Find first differing line.
    let first_changed_row = old_lines
        .iter()
        .zip(new_lines.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| old_lines.len().min(new_lines.len()));

    // Find last differing line (from the end).
    let max_suffix = old_lines.len().min(new_lines.len()) - first_changed_row;
    let common_suffix = old_lines
        .iter()
        .rev()
        .zip(new_lines.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();

    let old_end = old_lines.len() - common_suffix;
    let new_end = new_lines.len() - common_suffix;

    if first_changed_row == old_end && first_changed_row == new_end {
        return Ok(String::new());
    }

    // Build the replacement text from new_lines[first_diff..new_end].
    let mut merged_new_text = String::new();
    for line in &new_lines[first_changed_row..new_end] {
        merged_new_text.push_str(line);
        merged_new_text.push('\n');
    }

    // cursor_offset is relative to the first hunk's new content in
    // new_text. Translate it to an offset within merged_new_text, which
    // only contains lines first_diff..new_end of new_text.
    if let Some(hunk_offset) = cursor_offset {
        let hunk_start = first_hunk_pos.unwrap_or(0);
        let absolute_pos = hunk_start + hunk_offset;

        // Byte offset where first_diff starts in new_text.
        let merged_start: usize = new_lines[..first_changed_row]
            .iter()
            .map(|line| line.len() + 1)
            .sum();

        if absolute_pos >= merged_start {
            let relative_offset = absolute_pos - merged_start;
            if relative_offset <= merged_new_text.len() {
                merged_new_text.insert_str(relative_offset, CURSOR_MARKER);
            }
        }
    }

    // Build output with 2 lines of context above and below.
    let context_lines_count = 2;
    let mut prefix_start = first_changed_row.saturating_sub(context_lines_count);
    let mut suffix_end = (old_end + context_lines_count).min(old_lines.len());

    fn count_matches(line_range: Range<usize>, lines: &[&str]) -> usize {
        let pattern = &lines[line_range];
        let pattern_len = pattern.len();

        let mut count = 0;
        for offset in 0..=lines.len() - pattern_len {
            if &lines[offset..offset + pattern_len] == pattern {
                count += 1;
            }
        }
        count
    }

    // Expand prefix and suffix until they are unique
    while prefix_start > 0 {
        if count_matches(prefix_start..first_changed_row, &old_lines) > 1 {
            prefix_start -= 1;
        } else {
            break;
        }
    }
    while suffix_end < old_lines.len() {
        if count_matches(old_end..suffix_end, &old_lines) > 1 {
            suffix_end += 1;
        } else {
            break;
        }
    }

    let mut output = String::new();
    for line in &old_lines[prefix_start..first_changed_row] {
        output.push_str(line);
        output.push('\n');
    }
    output.push_str("<|fim_middle|>\n");
    output.push_str(&merged_new_text);
    output.push_str("<|fim_suffix|>\n");
    for line in &old_lines[old_end..suffix_end] {
        output.push_str(line);
        output.push('\n');
    }

    Ok(output)
}

struct ParsedHunk {
    old_context: String,
    edits: Vec<ParsedEdit>,
}

struct ParsedEdit {
    range: Range<usize>,
    text: String,
}

/// Parse a unified diff into content-based hunks. Each hunk contains an
/// `old_context` string (context lines + deleted lines, which together
/// form the text that should be found in the original) and a list of edits
/// expressed as byte ranges within that context.
fn parse_hunks(patch: &str) -> Vec<ParsedHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<ParsedHunk> = None;

    for line in patch.lines() {
        if line.starts_with("@@") {
            if let Some(hunk) = current.take() {
                if !hunk.old_context.is_empty() || !hunk.edits.is_empty() {
                    hunks.push(hunk);
                }
            }
            current = Some(ParsedHunk {
                old_context: String::new(),
                edits: Vec::new(),
            });
        } else if line.starts_with("---") || line.starts_with("+++") {
            continue;
        } else if let Some(hunk) = &mut current {
            if let Some(added) = line.strip_prefix('+') {
                let pos = hunk.old_context.len();
                if let Some(last_edit) = hunk.edits.last_mut() {
                    if last_edit.range.end == pos {
                        writeln!(&mut last_edit.text, "{added}").ok();
                        continue;
                    }
                }
                hunk.edits.push(ParsedEdit {
                    range: pos..pos,
                    text: format!("{added}\n"),
                });
            } else if let Some(removed) = line.strip_prefix('-') {
                let start = hunk.old_context.len();
                writeln!(&mut hunk.old_context, "{removed}").ok();
                let end = hunk.old_context.len();
                if let Some(last_edit) = hunk.edits.last_mut() {
                    if last_edit.range.end == start {
                        last_edit.range.end = end;
                        continue;
                    }
                }
                hunk.edits.push(ParsedEdit {
                    range: start..end,
                    text: String::new(),
                });
            } else {
                let ctx = line.strip_prefix(' ').unwrap_or(line);
                writeln!(&mut hunk.old_context, "{ctx}").ok();
            }
        }
    }

    if let Some(hunk) = current {
        if !hunk.old_context.is_empty() || !hunk.edits.is_empty() {
            hunks.push(hunk);
        }
    }

    hunks
}

#[cfg(test)]
mod tests;
