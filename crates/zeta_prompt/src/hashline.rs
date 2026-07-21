use std::fmt::Display;

pub const END_MARKER: &str = "<|fim_middle|>updated";
pub const START_MARKER: &str = "<|fim_middle|>current";

use super::*;

const SET_COMMAND_MARKER: &str = "<|set|>";
const INSERT_COMMAND_MARKER: &str = "<|insert|>";
pub const NO_EDITS_COMMAND_MARKER: &str = "<|no_edits|>";

pub fn special_tokens() -> &'static [&'static str] {
    return &[
        SET_COMMAND_MARKER,
        "<|set_range|>",
        INSERT_COMMAND_MARKER,
        NO_EDITS_COMMAND_MARKER,
        CURSOR_MARKER,
        "<|file_sep|>",
        "<|fim_prefix|>",
        "<|fim_suffix|>",
        "<|fim_middle|>",
    ];
}

/// A parsed line reference like `3:c3` (line index 3 with hash 0xc3).
#[derive(Debug, Clone, PartialEq, Eq)]
struct LineRef {
    index: usize,
    hash: u8,
}

impl Display for LineRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:02x}", self.index, self.hash)
    }
}

pub fn hash_line(line: &[u8]) -> u8 {
    let mut h: u8 = 0;
    for &byte in line {
        h = h.wrapping_add(byte);
    }
    return h;
}

/// Write the hashline-encoded editable region into `out`. Each line of
/// `editable_text` is prefixed with `{line_index}:{hash}|` and the cursor
/// marker is inserted at `cursor_offset_in_editable` (byte offset relative
/// to the start of `editable_text`).
pub fn write_hashline_editable_region(
    out: &mut String,
    editable_text: &str,
    cursor_offset_in_editable: usize,
) {
    let mut offset = 0;
    for (i, line) in editable_text.lines().enumerate() {
        let (head, cursor, tail) = if cursor_offset_in_editable > offset
            && cursor_offset_in_editable < offset + line.len()
        {
            (
                &line[..cursor_offset_in_editable - offset],
                CURSOR_MARKER,
                &line[cursor_offset_in_editable - offset..],
            )
        } else {
            (line, "", "")
        };
        write!(
            out,
            "\n{}|{head}{cursor}{tail}",
            LineRef {
                index: i,
                hash: hash_line(line.as_bytes())
            }
        )
        .unwrap();
        offset += line.len() + 1;
    }
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
    prompt.push_str(START_MARKER);

    let cursor_offset_in_editable = cursor_offset.saturating_sub(editable_range.start);
    let editable_region = &context[editable_range.clone()];
    write_hashline_editable_region(prompt, editable_region, cursor_offset_in_editable);

    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }

    prompt.push_str("<|fim_suffix|>\n");
    prompt.push_str(&context[editable_range.end..]);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }

    prompt.push_str(END_MARKER);
    prompt.push('\n');
}

/// A single edit command parsed from the model output.
#[derive(Debug)]
enum EditCommand<'a> {
    /// Replace a range of lines (inclusive on both ends). Single-line set is
    /// represented by `start == end`.
    Set {
        start: LineRef,
        end: LineRef,
        content: &'a str,
    },
    /// Insert new lines after the given line, or before the first line if
    /// `after` is `None`.
    Insert {
        after: Option<LineRef>,
        content: &'a str,
    },
}

/// Parse a line reference like `3:c3` into a `LineRef`.
fn parse_line_ref(s: &str) -> Option<LineRef> {
    let (idx_str, hash_str) = s.split_once(':')?;
    let index = idx_str.parse::<usize>().ok()?;
    let hash = u8::from_str_radix(hash_str, 16).ok()?;
    Some(LineRef { index, hash })
}

/// Parse the model output into a list of `EditCommand`s.
fn parse_edit_commands(model_output: &str) -> Vec<EditCommand<'_>> {
    let mut commands = Vec::new();
    let mut offset = 0usize;

    while offset < model_output.len() {
        let next_nl = model_output[offset..]
            .find('\n')
            .map(|i| offset + i)
            .unwrap_or(model_output.len());
        let line = &model_output[offset..next_nl];
        let line_end = if next_nl < model_output.len() {
            next_nl + 1
        } else {
            next_nl
        };

        let trimmed = line.trim();
        let (is_set, specifier) = if let Some(spec) = trimmed.strip_prefix(SET_COMMAND_MARKER) {
            (true, spec)
        } else if let Some(spec) = trimmed.strip_prefix(INSERT_COMMAND_MARKER) {
            (false, spec)
        } else {
            offset = line_end;
            continue;
        };

        let mut content_end = line_end;
        let mut scan = line_end;

        while scan < model_output.len() {
            let body_nl = model_output[scan..]
                .find('\n')
                .map(|i| scan + i)
                .unwrap_or(model_output.len());
            let body_line = &model_output[scan..body_nl];
            if body_line.trim().starts_with(SET_COMMAND_MARKER)
                || body_line.trim().starts_with(INSERT_COMMAND_MARKER)
            {
                break;
            }
            scan = if body_nl < model_output.len() {
                body_nl + 1
            } else {
                body_nl
            };
            content_end = scan;
        }

        let content = &model_output[line_end..content_end];

        if is_set {
            if let Some((start_str, end_str)) = specifier.split_once('-') {
                if let (Some(start), Some(end)) =
                    (parse_line_ref(start_str), parse_line_ref(end_str))
                {
                    commands.push(EditCommand::Set {
                        start,
                        end,
                        content,
                    });
                }
            } else if let Some(target) = parse_line_ref(specifier) {
                commands.push(EditCommand::Set {
                    start: target.clone(),
                    end: target,
                    content,
                });
            }
        } else {
            let after = parse_line_ref(specifier);
            commands.push(EditCommand::Insert { after, content });
        }

        offset = scan;
    }

    commands
}

/// Returns `true` if the model output contains `<|set|>` or `<|insert|>` commands
/// (as opposed to being a plain full-replacement output).
/// Strip the `{line_num}:{hash}|` prefixes from each line of a hashline-encoded
/// editable region, returning the plain text content.
pub fn strip_hashline_prefixes(region: &str) -> String {
    let mut decoded: String = region
        .lines()
        .map(|line| line.find('|').map_or(line, |pos| &line[pos + 1..]))
        .collect::<Vec<_>>()
        .join("\n");
    if region.ends_with('\n') {
        decoded.push('\n');
    }
    decoded
}

pub fn output_has_edit_commands(model_output: &str) -> bool {
    model_output.contains(SET_COMMAND_MARKER)
        || model_output.contains(INSERT_COMMAND_MARKER)
        || model_output.contains(NO_EDITS_COMMAND_MARKER)
}

/// Apply `<|set|>` and `<|insert|>` edit commands from the model output to the
/// original editable region text.
///
/// `editable_region` is the original text of the editable region (without hash
/// prefixes). `model_output` is the raw model response containing edit commands.
///
/// Returns the full replacement text for the editable region.
pub fn apply_edit_commands(editable_region: &str, model_output: &str) -> String {
    if model_output
        .trim_start()
        .starts_with(NO_EDITS_COMMAND_MARKER)
    {
        return editable_region.to_string();
    }

    let original_lines: Vec<&str> = editable_region.lines().collect();
    let old_hashes: Vec<u8> = original_lines
        .iter()
        .map(|line| hash_line(line.as_bytes()))
        .collect();

    let commands = parse_edit_commands(model_output);

    // For set operations: indexed by start line → Some((end line index, content))
    // For insert operations: indexed by line index → vec of content to insert after
    // Insert-before-first is tracked separately.
    let mut set_ops: Vec<Option<(usize, &str)>> = vec![None; original_lines.len()];
    let mut insert_before_first: Vec<&str> = Vec::new();
    let mut insert_after: Vec<Vec<&str>> = vec![Vec::new(); original_lines.len()];

    for command in &commands {
        match command {
            EditCommand::Set {
                start,
                end,
                content,
            } => {
                if start.index < old_hashes.len()
                    && end.index < old_hashes.len()
                    && start.index <= end.index
                    && old_hashes[start.index] == start.hash
                    && old_hashes[end.index] == end.hash
                {
                    set_ops[start.index] = Some((end.index, *content));
                }
            }
            EditCommand::Insert { after, content } => match after {
                None => insert_before_first.push(*content),
                Some(line_ref) => {
                    if line_ref.index < old_hashes.len()
                        && old_hashes[line_ref.index] == line_ref.hash
                    {
                        insert_after[line_ref.index].push(*content);
                    }
                }
            },
        }
    }

    let mut result = String::new();

    // Emit any insertions before the first line
    for content in &insert_before_first {
        result.push_str(content);
        if !content.ends_with('\n') {
            result.push('\n');
        }
    }

    let mut i = 0;
    while i < original_lines.len() {
        if let Some((end_index, replacement)) = set_ops[i].as_ref() {
            // Replace lines i..=end_index with the replacement content
            result.push_str(replacement);
            if !replacement.is_empty() && !replacement.ends_with('\n') {
                result.push('\n');
            }
            // Emit any insertions after the end of this set range
            if *end_index < insert_after.len() {
                for content in &insert_after[*end_index] {
                    result.push_str(content);
                    if !content.ends_with('\n') {
                        result.push('\n');
                    }
                }
            }
            i = end_index + 1;
        } else {
            // Keep the original line
            result.push_str(original_lines[i]);
            result.push('\n');
            // Emit any insertions after this line
            for content in &insert_after[i] {
                result.push_str(content);
                if !content.ends_with('\n') {
                    result.push('\n');
                }
            }
            i += 1;
        }
    }

    // Preserve trailing newline behavior: if the original ended with a
    // newline the result already has one; if it didn't, trim the extra one
    // we added.
    if !editable_region.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Convert a unified diff patch into hashline edit commands.
///
/// Parses the unified diff `patch` directly to determine which lines of
/// `old_text` are deleted/replaced and what new lines are added, then emits
/// `<|set|>` and `<|insert|>` edit commands referencing old lines by their
/// `{index}:{hash}` identifiers.
///
/// `cursor_offset` is an optional byte offset into the first hunk's new
/// text (context + additions) where the cursor marker should be placed.
mod patch_commands;
pub use patch_commands::patch_to_edit_commands;

#[cfg(test)]
mod tests;
