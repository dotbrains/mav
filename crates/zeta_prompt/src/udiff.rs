use std::borrow::Cow;

pub fn strip_diff_path_prefix<'a>(diff: &'a str, prefix: &str) -> Cow<'a, str> {
    if prefix.is_empty() {
        return Cow::Borrowed(diff);
    }

    let prefix_with_slash = format!("{}/", prefix);
    let mut needs_rewrite = false;

    for line in diff.lines() {
        match DiffLine::parse(line) {
            DiffLine::OldPath { path } | DiffLine::NewPath { path } => {
                if path.starts_with(&prefix_with_slash) {
                    needs_rewrite = true;
                    break;
                }
            }
            _ => {}
        }
    }

    if !needs_rewrite {
        return Cow::Borrowed(diff);
    }

    let mut result = String::with_capacity(diff.len());
    for line in diff.lines() {
        match DiffLine::parse(line) {
            DiffLine::OldPath { path } => {
                let stripped = path
                    .strip_prefix(&prefix_with_slash)
                    .unwrap_or(path.as_ref());
                result.push_str(&format!("--- a/{}\n", stripped));
            }
            DiffLine::NewPath { path } => {
                let stripped = path
                    .strip_prefix(&prefix_with_slash)
                    .unwrap_or(path.as_ref());
                result.push_str(&format!("+++ b/{}\n", stripped));
            }
            _ => {
                result.push_str(line);
                result.push('\n');
            }
        }
    }

    Cow::Owned(result)
}

/// Strip unnecessary git metadata lines from a diff, keeping only the lines
/// needed for patch application: path headers (--- and +++), hunk headers (@@),
/// and content lines (+, -, space).
pub fn strip_diff_metadata(diff: &str) -> String {
    let mut result = String::new();

    for line in diff.lines() {
        let dominated = DiffLine::parse(line);
        match dominated {
            // Keep path headers, hunk headers, and content lines
            DiffLine::OldPath { .. }
            | DiffLine::NewPath { .. }
            | DiffLine::HunkHeader(_)
            | DiffLine::Context(_)
            | DiffLine::Deletion(_)
            | DiffLine::Addition(_)
            | DiffLine::NoNewlineAtEOF => {
                result.push_str(line);
                result.push('\n');
            }
            // Skip garbage lines (diff --git, index, etc.)
            DiffLine::Garbage(_) => {}
        }
    }

    result
}

pub const CURSOR_POSITION_MARKER: &str = "[CURSOR_POSITION]";
pub const INLINE_CURSOR_MARKER: &str = "<|user_cursor|>";

/// Extract cursor offset from a patch and return `(clean_patch, cursor_offset)`.
pub fn extract_cursor_from_patch(patch: &str) -> (String, Option<usize>) {
    let mut clean_patch = String::new();
    let mut cursor_offset = None;
    let mut line_start_offset = 0usize;

    for line in patch.lines() {
        if !clean_patch.is_empty() {
            clean_patch.push('\n');
        }

        match DiffLine::parse(line) {
            DiffLine::Addition(content) => {
                let clean_content = content.replace(INLINE_CURSOR_MARKER, "");
                if cursor_offset.is_none()
                    && let Some(marker_offset) = content.find(INLINE_CURSOR_MARKER)
                {
                    cursor_offset = Some(line_start_offset + marker_offset);
                }
                clean_patch.push('+');
                clean_patch.push_str(&clean_content);
                line_start_offset += clean_content.len() + 1;
            }
            DiffLine::Context(content) => {
                clean_patch.push_str(line);
                line_start_offset += content.len() + 1;
            }
            _ => clean_patch.push_str(line),
        }
    }

    if patch.ends_with('\n') && !clean_patch.is_empty() {
        clean_patch.push('\n');
    }

    (clean_patch, cursor_offset)
}

/// Find all byte offsets where `hunk.context` occurs as a substring of `text`.
///
/// If no exact matches are found and the context ends with `'\n'` but `text`
/// does not, retries without the trailing newline, accepting only a match at
/// the very end of `text`. When this fallback fires, the hunk's context is
/// trimmed and its edit ranges are clamped so that downstream code doesn't
/// index past the end of the matched region. This handles diffs that are
/// missing a `\ No newline at end of file` marker: the parser always appends
/// `'\n'` via `writeln!`, so the context can have a trailing newline that
/// doesn't exist in the source text.
mod apply;
pub use apply::{
    apply_diff_to_string, apply_diff_to_string_with_hunk_offset, disambiguate_by_line_number,
    find_context_candidates,
};

mod generator;
pub use generator::unified_diff_with_context;

pub fn encode_cursor_in_patch(patch: &str, cursor_offset: Option<usize>) -> String {
    let Some(cursor_offset) = cursor_offset else {
        return patch.to_string();
    };

    let mut result = String::new();
    let mut line_start_offset = 0usize;

    for line in patch.lines() {
        if !result.is_empty() {
            result.push('\n');
        }

        match DiffLine::parse(line) {
            DiffLine::Addition(content) => {
                let content = content.replace(INLINE_CURSOR_MARKER, "");
                let line_end_offset = line_start_offset + content.len();
                result.push('+');
                if cursor_offset >= line_start_offset
                    && cursor_offset <= line_end_offset
                    && let Some(before) = content.get(..cursor_offset - line_start_offset)
                    && let Some(after) = content.get(cursor_offset - line_start_offset..)
                {
                    result.push_str(before);
                    result.push_str(INLINE_CURSOR_MARKER);
                    result.push_str(after);
                } else {
                    result.push_str(&content);
                }
                line_start_offset = line_end_offset + 1;
            }
            DiffLine::Context(content) => {
                result.push_str(line);
                line_start_offset += content.len() + 1;
            }
            _ => result.push_str(line),
        }
    }

    if patch.ends_with('\n') {
        result.push('\n');
    }

    result
}

mod parser;
#[cfg(test)]
use parser::parse_header_path;
pub use parser::{DiffEvent, DiffLine, DiffParser, Edit, FileStatus, Hunk, HunkLocation};
#[cfg(test)]
mod tests {
    use super::*;

    mod apply;
    mod generator;
    mod metadata;
    mod parser;
}
