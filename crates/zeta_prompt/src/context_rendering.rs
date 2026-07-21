use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::*;

pub fn format_active_buffer_diagnostics_with_budget(
    diagnostics: &[ActiveBufferDiagnostic],
    cursor_buffer_row: Option<u32>,
    budget: usize,
) -> String {
    if diagnostics.is_empty() || budget == 0 {
        return String::new();
    }

    const MAX_DIAGNOSTICS: usize = 10;

    let mut diagnostic_indices = (0..diagnostics.len()).collect::<Vec<_>>();
    if let Some(cursor_buffer_row) = cursor_buffer_row {
        let distance = |index: &usize| {
            let range = &diagnostics[*index].snippet_buffer_row_range;
            u32::abs_diff(cursor_buffer_row, range.start)
                + u32::abs_diff(cursor_buffer_row, range.end)
        };
        // Only the closest `MAX_DIAGNOSTICS` are rendered below, so select that
        // prefix instead of fully sorting every diagnostic.
        if diagnostic_indices.len() > MAX_DIAGNOSTICS {
            diagnostic_indices.select_nth_unstable_by_key(MAX_DIAGNOSTICS, &distance);
            diagnostic_indices.truncate(MAX_DIAGNOSTICS);
        }
        diagnostic_indices.sort_unstable_by_key(&distance);
    }

    let mut output = format!("{}diagnostics\n", seed_coder::FILE_MARKER);
    let header_tokens = estimate_tokens(output.len());
    if header_tokens > budget {
        return String::new();
    }

    let mut used_tokens = header_tokens;
    let mut included_diagnostics = 0;
    for diagnostic_index in diagnostic_indices.into_iter().take(MAX_DIAGNOSTICS) {
        let diagnostic = &diagnostics[diagnostic_index];
        let snippet = clamp_text_to_token_count(&diagnostic.snippet, 256);

        let diagnostic_section = if snippet.is_empty() {
            format!("*{}*\n", diagnostic.message)
        } else {
            format!(
                "*{}*:\n```\n{}{}\n```\n",
                diagnostic.message,
                snippet,
                if snippet.len() < diagnostic.snippet.len() {
                    "..."
                } else {
                    ""
                }
            )
        };
        let diagnostic_tokens = estimate_tokens(diagnostic_section.len());
        if used_tokens + diagnostic_tokens > budget {
            break;
        }
        output.push_str(&diagnostic_section);
        used_tokens += diagnostic_tokens;
        included_diagnostics += 1;
    }

    if included_diagnostics == 0 {
        String::new()
    } else {
        output
    }
}

pub fn filter_redundant_excerpts(
    mut related_files: Vec<RelatedFile>,
    cursor_path: &Path,
    cursor_row_range: Range<u32>,
) -> Vec<RelatedFile> {
    for file in &mut related_files {
        if file.path.as_ref() == cursor_path {
            file.excerpts.retain(|excerpt| {
                excerpt.row_range.start < cursor_row_range.start
                    || excerpt.row_range.end > cursor_row_range.end
            });
        }
    }
    related_files.retain(|file| !file.excerpts.is_empty());
    related_files
}

pub fn format_edit_history_within_budget(
    events: &[Arc<Event>],
    file_marker: &str,
    edit_history_name: &str,
    max_tokens: usize,
    max_edit_event_count: usize,
) -> String {
    let header = format!("{}{}\n", file_marker, edit_history_name);
    let header_tokens = estimate_tokens(header.len());
    if header_tokens >= max_tokens {
        return String::new();
    }

    let mut event_strings: Vec<String> = Vec::new();
    let mut total_tokens = header_tokens;

    for event in events.iter().rev().take(max_edit_event_count) {
        let mut event_str = String::new();
        write_event(&mut event_str, event);
        let event_tokens = estimate_tokens(event_str.len());

        if total_tokens + event_tokens > max_tokens {
            break;
        }
        total_tokens += event_tokens;
        event_strings.push(event_str);
    }

    if event_strings.is_empty() {
        return String::new();
    }

    let mut result = header;
    for event_str in event_strings.iter().rev() {
        result.push_str(event_str);
    }
    result
}

fn excerpt_rendered_tokens(excerpt: &RelatedExcerpt, file_max_row: u32) -> usize {
    let needs_newline = !excerpt.text.ends_with('\n');
    let needs_ellipsis = excerpt.row_range.end < file_max_row;
    let len = excerpt.text.len()
        + if needs_newline { "\n".len() } else { 0 }
        + if needs_ellipsis { "...\n".len() } else { 0 };
    estimate_tokens(len)
}

pub fn format_related_files_within_budget(
    related_files: &[RelatedFile],
    file_prefix: &str,
    file_suffix: &str,
    max_tokens: usize,
) -> String {
    struct ExcerptCandidate {
        file_ix: usize,
        excerpt_ix: usize,
        order: usize,
    }

    let mut excerpt_candidates: Vec<ExcerptCandidate> = related_files
        .iter()
        .enumerate()
        .flat_map(|(file_ix, file)| {
            file.excerpts
                .iter()
                .enumerate()
                .map(move |(excerpt_ix, e)| ExcerptCandidate {
                    file_ix,
                    excerpt_ix,
                    order: e.order,
                })
        })
        .collect();

    // Pre-compute file header strings and their token costs.
    let file_headers: Vec<String> = related_files
        .iter()
        .map(|file| {
            let path_str = file.path.to_string_lossy();
            format!("{}{}\n", file_prefix, path_str)
        })
        .collect();

    // Sort the excerpts by their order and determine how many fit within the budget.
    let mut total_tokens = 0;
    let mut included_excerpt_count = 0_usize;
    let mut included_file_indices = vec![false; related_files.len()];
    excerpt_candidates.sort_by_key(|e| (e.order, e.file_ix, e.excerpt_ix));
    for candidate in &excerpt_candidates {
        let file = &related_files[candidate.file_ix];
        let excerpt = &file.excerpts[candidate.excerpt_ix];
        let file_already_included = included_file_indices[candidate.file_ix];
        let header_cost = if file_already_included {
            0
        } else {
            estimate_tokens(file_headers[candidate.file_ix].len() + file_suffix.len())
        };
        let excerpt_cost = excerpt_rendered_tokens(excerpt, file.max_row);
        if total_tokens + header_cost + excerpt_cost > max_tokens {
            break;
        }
        total_tokens += header_cost + excerpt_cost;
        if !file_already_included {
            included_file_indices[candidate.file_ix] = true;
        }
        included_excerpt_count += 1;
    }

    excerpt_candidates.truncate(included_excerpt_count);
    excerpt_candidates.sort_unstable_by_key(|c| (c.file_ix, c.excerpt_ix));

    // Render all of the files that fit within the token budget, in the original order.
    let mut result = String::new();
    let mut last_file_ix = None;
    for (candidate_ix, candidate) in excerpt_candidates.iter().enumerate() {
        if last_file_ix != Some(candidate.file_ix) {
            if last_file_ix.is_some() {
                result.push_str(file_suffix);
            }
            result.push_str(&file_headers[candidate.file_ix]);
            last_file_ix = Some(candidate.file_ix);
        }
        let file = &related_files[candidate.file_ix];
        let excerpt = &file.excerpts[candidate.excerpt_ix];
        result.push_str(&excerpt.text);
        if !result.ends_with('\n') {
            result.push('\n');
        }
        let next_excerpt_start = excerpt_candidates
            .get(candidate_ix + 1)
            .filter(|next| next.file_ix == candidate.file_ix)
            .map(|next| file.excerpts[next.excerpt_ix].row_range.start);
        if rows_omitted_after_excerpt(excerpt, next_excerpt_start, file.max_row) {
            result.push_str("...\n");
        }
    }

    result
}

/// Whether rows are omitted between this excerpt and the next rendered
/// excerpt of the same file (or the end of the file), in which case an
/// ellipsis line should be rendered.
pub fn rows_omitted_after_excerpt(
    excerpt: &RelatedExcerpt,
    next_excerpt_start: Option<u32>,
    file_max_row: u32,
) -> bool {
    match next_excerpt_start {
        Some(next_start) => excerpt.row_range.end < next_start,
        None => excerpt.row_range.end < file_max_row,
    }
}

pub fn write_related_files(
    prompt: &mut String,
    related_files: &[RelatedFile],
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for file in related_files {
        let start = prompt.len();
        let path_str = file.path.to_string_lossy();
        write!(prompt, "<|file_sep|>{}\n", path_str).ok();
        for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
            prompt.push_str(&excerpt.text);
            if !prompt.ends_with('\n') {
                prompt.push('\n');
            }
            let next_excerpt_start = file
                .excerpts
                .get(excerpt_ix + 1)
                .map(|next| next.row_range.start);
            if rows_omitted_after_excerpt(excerpt, next_excerpt_start, file.max_row) {
                prompt.push_str("...\n");
            }
        }
        let end = prompt.len();
        ranges.push(start..end);
    }
    ranges
}
