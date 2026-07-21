pub mod excerpt_ranges;
pub mod hashed_regions;
pub mod multi_region;
pub mod udiff;

use anyhow::{Result, anyhow};
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

pub use crate::excerpt_ranges::{
    ExcerptRanges, compute_editable_and_context_ranges, compute_legacy_excerpt_ranges,
};

pub const CURSOR_MARKER: &str = "<|user_cursor|>";

/// Use up to this amount of the editable region for prefill.
/// Larger values may result in more robust generation, but
/// this region becomes non-editable.
pub const PREFILL_RATIO: f64 = 0.1; // 10%

fn estimate_tokens(bytes: usize) -> usize {
    bytes / 3
}

/// Leave some slack to avoid overflow.
fn apply_prompt_budget_margin(max_tokens: usize) -> usize {
    (max_tokens as f64 * 0.9).floor() as usize
}

/// Ensure text fits into the tokens budget; trim by line boundaries if needed.
pub fn clamp_text_to_token_count(text: &str, max_tokens: usize) -> &str {
    if estimate_tokens(text.len()) <= max_tokens {
        return text;
    }

    let mut end_byte_offset = 0;

    for line in text.split_inclusive('\n') {
        if estimate_tokens(line.len() + end_byte_offset) > max_tokens {
            break;
        }

        end_byte_offset += line.len();
    }

    &text[..end_byte_offset]
}

mod types;
pub use types::{
    ActiveBufferDiagnostic, ContextSource, Event, FilePosition, RelatedExcerpt, RelatedFile,
    Zeta2PromptInput, Zeta3PromptInput, ZetaFormat, write_event,
};

pub fn prompt_input_contains_special_tokens(input: &Zeta2PromptInput, format: ZetaFormat) -> bool {
    special_tokens_for_format(format).iter().any(|token| {
        if let Some(line_token) = token.strip_suffix('\n') {
            input.cursor_excerpt.lines().any(|line| line == line_token)
        } else {
            input.cursor_excerpt.contains(token)
        }
    })
}

pub fn format_zeta_prompt(input: &Zeta2PromptInput, format: ZetaFormat) -> Option<String> {
    format_prompt_with_budget_for_format(input, format, max_prompt_tokens_for_format(format))
}

pub fn format_zeta3_prompt(input: &Zeta3PromptInput, format: ZetaFormat) -> Option<String> {
    match format {
        ZetaFormat::V0318SeedMultiRegions => {}
        _ => return None,
    }

    let (current_excerpt, cursor_offset_in_excerpt) = zeta3_current_file_excerpt(input)?;
    let (context, editable_range, context_range, cursor_offset) = resolve_zeta3_cursor_region(
        current_excerpt.text.as_ref(),
        cursor_offset_in_excerpt,
        &input.syntax_ranges,
        format,
    );
    let relative_row_range =
        offset_range_to_row_range(current_excerpt.text.as_ref(), context_range);
    let cursor_row_range = current_excerpt.row_range.start + relative_row_range.start
        ..current_excerpt.row_range.start + relative_row_range.end;
    let related_files = filter_redundant_excerpts(
        zeta3_related_files(input, current_excerpt),
        input.cursor_path.as_ref(),
        cursor_row_range,
    );

    format_resolved_prompt_with_budget(
        format,
        input.cursor_path.as_ref(),
        context,
        &editable_range,
        cursor_offset,
        &input.events,
        &related_files,
        &input.active_buffer_diagnostics,
        Some(input.cursor_position.row),
        max_prompt_tokens_for_format(format),
    )
}

fn max_prompt_tokens_for_format(format: ZetaFormat) -> usize {
    match format {
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304VariableEdit
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0608QwenMultiRegions => 4096,
        ZetaFormat::V0615HashRegions => 8000,
        ZetaFormat::V0420Diagnostics => 8192,
        ZetaFormat::V0327SingleFile => 16384,
    }
}

fn zeta3_current_file_excerpt(input: &Zeta3PromptInput) -> Option<(&RelatedExcerpt, usize)> {
    input
        .editable_context
        .iter()
        .filter(|file| file.path == input.cursor_path)
        .flat_map(|file| file.excerpts.iter())
        .find_map(|excerpt| {
            if excerpt.context_source != ContextSource::CurrentFile {
                return None;
            }
            Some((
                excerpt,
                offset_for_position_in_excerpt(excerpt, input.cursor_position)?,
            ))
        })
}

fn offset_for_position_in_excerpt(
    excerpt: &RelatedExcerpt,
    position: FilePosition,
) -> Option<usize> {
    if position.row < excerpt.row_range.start {
        return None;
    }

    let relative_row = (position.row - excerpt.row_range.start) as usize;
    let text = excerpt.text.as_ref();
    let mut row_start = 0;

    for row in 0..=relative_row {
        if row == relative_row {
            let row_end = text[row_start..]
                .find('\n')
                .map_or(text.len(), |offset| row_start + offset);
            let row_text = &text[row_start..row_end];
            let column =
                row_text.floor_char_boundary((position.column as usize).min(row_text.len()));
            return Some(row_start + column);
        }

        row_start += text[row_start..].find('\n')? + 1;
    }

    None
}

fn zeta3_related_files(
    input: &Zeta3PromptInput,
    current_excerpt: &RelatedExcerpt,
) -> Vec<RelatedFile> {
    input
        .editable_context
        .iter()
        .filter_map(|file| {
            let mut file = file.clone();
            if file.path == input.cursor_path {
                file.excerpts.retain(|excerpt| excerpt != current_excerpt);
            }
            (!file.excerpts.is_empty()).then_some(file)
        })
        .collect()
}

fn resolve_zeta3_cursor_region<'a>(
    cursor_excerpt: &'a str,
    cursor_offset: usize,
    syntax_ranges: &[Range<usize>],
    format: ZetaFormat,
) -> (&'a str, Range<usize>, Range<usize>, usize) {
    let (editable_tokens, context_tokens) = token_limits_for_format(format);
    let (editable_range, context_range) = compute_editable_and_context_ranges(
        cursor_excerpt,
        cursor_offset,
        syntax_ranges,
        editable_tokens,
        context_tokens,
    );

    adjust_cursor_region(cursor_excerpt, cursor_offset, editable_range, context_range)
}

mod format_metadata;
pub use format_metadata::{
    TrainingDelimiters, excerpt_ranges_for_format, special_tokens_for_format,
    stop_tokens_for_format, token_limits_for_format, training_delimiters_for_format,
};

mod cursor_excerpt;
pub use cursor_excerpt::{
    assemble_single_file_fim_prompt, offset_range_to_row_range,
    write_cursor_excerpt_section_for_format,
};

fn format_hash_region_related_files_within_budget(
    input: &Zeta2PromptInput,
    marker_table: &[hashed_regions::SnippetMarkers],
    cursor: &hashed_regions::RelatedFileCursor,
    max_tokens: usize,
) -> Option<String> {
    let related_files = input.related_files.as_deref()?;

    struct RenderedExcerpt {
        file_ix: usize,
        excerpt_ix: usize,
        order: usize,
        rendered: String,
    }

    let mut candidates = Vec::new();
    let mut required_candidate_ix = None;
    for (file_ix, file) in related_files.iter().enumerate() {
        for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
            let markers =
                hashed_regions::marker_table_for_excerpt(marker_table, file_ix, excerpt_ix);
            let mut rendered = String::new();
            if let Some(markers) = markers {
                let cursor_in_excerpt = (file_ix == cursor.file_ix
                    && excerpt_ix == cursor.excerpt_ix)
                    .then_some((cursor.offset_in_excerpt, CURSOR_MARKER));
                hashed_regions::write_snippet_with_markers(
                    &mut rendered,
                    &excerpt.text,
                    markers,
                    cursor_in_excerpt,
                );
            } else {
                rendered.push_str(&excerpt.text);
            }
            if !rendered.ends_with('\n') {
                rendered.push('\n');
            }

            if file_ix == cursor.file_ix && excerpt_ix == cursor.excerpt_ix {
                required_candidate_ix = Some(candidates.len());
            }

            candidates.push(RenderedExcerpt {
                file_ix,
                excerpt_ix,
                order: excerpt.order,
                rendered,
            });
        }
    }

    let required_candidate_ix = required_candidate_ix?;
    let file_headers: Vec<String> = related_files
        .iter()
        .map(|file| {
            let path = hashed_regions::related_file_patch_path(&input.cursor_path, &file.path)
                .iter()
                .map(|component| component.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            format!("{}{path}\n", seed_coder::FILE_MARKER)
        })
        .collect();

    let mut total_tokens = 0;
    let mut included = vec![false; candidates.len()];
    let mut file_included = vec![false; related_files.len()];

    let required = &candidates[required_candidate_ix];
    let required_cost =
        estimate_tokens(file_headers[required.file_ix].len() + required.rendered.len());
    if required_cost > max_tokens {
        return None;
    }
    total_tokens += required_cost;
    included[required_candidate_ix] = true;
    file_included[required.file_ix] = true;

    let mut selection_order: Vec<usize> = (0..candidates.len()).collect();
    selection_order.sort_by_key(|&candidate_ix| {
        let candidate = &candidates[candidate_ix];
        (candidate.order, candidate.file_ix, candidate.excerpt_ix)
    });

    for candidate_ix in selection_order {
        if included[candidate_ix] {
            continue;
        }
        let candidate = &candidates[candidate_ix];
        let header_cost = if file_included[candidate.file_ix] {
            0
        } else {
            estimate_tokens(file_headers[candidate.file_ix].len())
        };
        let excerpt_cost = estimate_tokens(candidate.rendered.len());
        if total_tokens + header_cost + excerpt_cost > max_tokens {
            continue;
        }
        total_tokens += header_cost + excerpt_cost;
        included[candidate_ix] = true;
        file_included[candidate.file_ix] = true;
    }

    let mut result = String::new();
    let mut last_file_ix = None;
    for (candidate_ix, candidate) in candidates.iter().enumerate() {
        if !included[candidate_ix] {
            continue;
        }
        if last_file_ix != Some(candidate.file_ix) {
            result.push_str(&file_headers[candidate.file_ix]);
            last_file_ix = Some(candidate.file_ix);
        }
        result.push_str(&candidate.rendered);

        let file = &related_files[candidate.file_ix];
        let excerpt = &file.excerpts[candidate.excerpt_ix];
        let next_excerpt_start = candidates
            .iter()
            .enumerate()
            .skip(candidate_ix + 1)
            .find(|(next_ix, next)| included[*next_ix] && next.file_ix == candidate.file_ix)
            .map(|(_, next)| file.excerpts[next.excerpt_ix].row_range.start);
        if rows_omitted_after_excerpt(excerpt, next_excerpt_start, file.max_row) {
            result.push_str("...\n");
        }
    }

    Some(result)
}

fn format_hash_regions_prompt_with_budget(
    input: &Zeta2PromptInput,
    max_tokens: usize,
) -> Option<String> {
    let marker_table = hashed_regions::build_marker_table(input);
    let cursor = hashed_regions::locate_cursor_in_related_files(input)?;
    hashed_regions::marker_table_for_excerpt(&marker_table, cursor.file_ix, cursor.excerpt_ix)?;

    let fixed_tokens = estimate_tokens(
        seed_coder::FIM_SUFFIX.len()
            + "\n".len()
            + seed_coder::FIM_PREFIX.len()
            + seed_coder::FIM_MIDDLE.len(),
    );
    let related_files_budget = max_tokens.saturating_sub(fixed_tokens);
    let related_files_section = format_hash_region_related_files_within_budget(
        input,
        &marker_table,
        &cursor,
        related_files_budget,
    )?;

    let mut prompt = String::new();
    prompt.push_str(seed_coder::FIM_SUFFIX);
    prompt.push('\n');
    prompt.push_str(seed_coder::FIM_PREFIX);
    prompt.push_str(&related_files_section);
    if !prompt.ends_with('\n') {
        prompt.push('\n');
    }
    prompt.push_str(seed_coder::FIM_MIDDLE);
    Some(prompt)
}

pub fn format_prompt_with_budget_for_format(
    input: &Zeta2PromptInput,
    format: ZetaFormat,
    max_tokens: usize,
) -> Option<String> {
    if format == ZetaFormat::V0615HashRegions {
        return format_hash_regions_prompt_with_budget(
            input,
            apply_prompt_budget_margin(max_tokens),
        );
    }

    let (context, editable_range, context_range, cursor_offset) =
        resolve_cursor_region(input, format);
    let empty_files = Vec::new();
    let input_related_files = input.related_files.as_deref().unwrap_or(&empty_files);
    let filtered_related_files = if format == ZetaFormat::V0615HashRegions {
        input_related_files.to_vec()
    } else if let Some(cursor_excerpt_start_row) = input.excerpt_start_row {
        let relative_row_range =
            offset_range_to_row_range(&input.cursor_excerpt, context_range.clone());
        let row_range = relative_row_range.start + cursor_excerpt_start_row
            ..relative_row_range.end + cursor_excerpt_start_row;
        filter_redundant_excerpts(
            input_related_files.to_vec(),
            input.cursor_path.as_ref(),
            row_range,
        )
    } else {
        input_related_files.to_vec()
    };
    let cursor_buffer_row = input.excerpt_start_row.map(|excerpt_start_row| {
        excerpt_start_row
            + input.cursor_excerpt[..context_range.start + cursor_offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count() as u32
    });

    format_resolved_prompt_with_budget(
        format,
        input.cursor_path.as_ref(),
        context,
        &editable_range,
        cursor_offset,
        &input.events,
        &filtered_related_files,
        &input.active_buffer_diagnostics,
        cursor_buffer_row,
        max_tokens,
    )
}

fn format_resolved_prompt_with_budget(
    format: ZetaFormat,
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
    events: &[Arc<Event>],
    related_files: &[RelatedFile],
    active_buffer_diagnostics: &[ActiveBufferDiagnostic],
    cursor_buffer_row: Option<u32>,
    max_tokens: usize,
) -> Option<String> {
    let prompt = match format {
        ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0420Diagnostics => {
            let mut cursor_section = String::new();

            write_cursor_excerpt_section_for_format(
                format,
                &mut cursor_section,
                path,
                context,
                editable_range,
                cursor_offset,
            );

            let budget_with_margin = apply_prompt_budget_margin(max_tokens);
            seed_coder::assemble_fim_prompt(
                context,
                editable_range,
                &cursor_section,
                events,
                related_files,
                if format == ZetaFormat::V0420Diagnostics {
                    active_buffer_diagnostics
                } else {
                    &[]
                },
                cursor_buffer_row,
                budget_with_margin,
            )
        }
        ZetaFormat::V0608QwenMultiRegions => {
            let mut cursor_section = String::new();

            write_cursor_excerpt_section_for_format(
                format,
                &mut cursor_section,
                path,
                context,
                editable_range,
                cursor_offset,
            );

            qwen::assemble_fim_prompt(
                context,
                editable_range,
                &cursor_section,
                events,
                related_files,
                apply_prompt_budget_margin(max_tokens),
            )
        }
        ZetaFormat::V0327SingleFile => {
            let mut cursor_section = String::new();
            write_cursor_excerpt_section_for_format(
                format,
                &mut cursor_section,
                path,
                context,
                editable_range,
                cursor_offset,
            );

            assemble_single_file_fim_prompt(
                context,
                editable_range,
                &cursor_section,
                events,
                apply_prompt_budget_margin(max_tokens),
            )
        }
        _ => {
            let mut cursor_section = String::new();
            write_cursor_excerpt_section_for_format(
                format,
                &mut cursor_section,
                path,
                context,
                editable_range,
                cursor_offset,
            );

            let mut remaining_budget = apply_prompt_budget_margin(max_tokens);
            let cursor_tokens = estimate_tokens(cursor_section.len());
            remaining_budget = remaining_budget.saturating_sub(cursor_tokens);

            let edit_history_section = format_edit_history_within_budget(
                events,
                "<|file_sep|>",
                "edit history",
                remaining_budget,
                max_edit_event_count_for_format(&format),
            );
            let edit_history_tokens = estimate_tokens(edit_history_section.len());
            remaining_budget = remaining_budget.saturating_sub(edit_history_tokens);

            let related_files_section = format_related_files_within_budget(
                related_files,
                "<|file_sep|>",
                "",
                remaining_budget,
            );

            let mut prompt = String::new();
            prompt.push_str(&related_files_section);
            prompt.push_str(&edit_history_section);
            prompt.push_str(&cursor_section);
            prompt
        }
    };
    let prompt_tokens = estimate_tokens(prompt.len());
    if prompt_tokens > max_tokens {
        return None;
    }
    return Some(prompt);
}

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

pub fn max_edit_event_count_for_format(format: &ZetaFormat) -> usize {
    match format {
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0304VariableEdit
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0608QwenMultiRegions
        | ZetaFormat::V0327SingleFile
        | ZetaFormat::V0615HashRegions => 6,
    }
}

pub fn get_prefill_for_format(
    format: ZetaFormat,
    context: &str,
    editable_range: &Range<usize>,
) -> String {
    match format {
        ZetaFormat::V0211Prefill => v0211_prefill::get_prefill(context, editable_range),
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304VariableEdit => String::new(),
        ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0608QwenMultiRegions
        | ZetaFormat::V0327SingleFile
        | ZetaFormat::V0615HashRegions => String::new(),
    }
}

pub fn output_end_marker_for_format(format: ZetaFormat) -> Option<&'static str> {
    match format {
        ZetaFormat::V0120GitMergeMarkers => Some(v0120_git_merge_markers::END_MARKER),
        ZetaFormat::V0131GitMergeMarkersPrefix => Some(v0131_git_merge_markers_prefix::END_MARKER),
        ZetaFormat::V0211Prefill => Some(v0131_git_merge_markers_prefix::END_MARKER),
        ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions => Some(seed_coder::END_MARKER),
        ZetaFormat::V0316SeedMultiRegions => Some(multi_region::V0316_END_MARKER),
        ZetaFormat::V0318SeedMultiRegions => Some(multi_region::V0318_END_MARKER),
        ZetaFormat::V0420Diagnostics => Some(multi_region::V0318_END_MARKER),
        ZetaFormat::V0608QwenMultiRegions => Some(qwen::END_MARKER),
        ZetaFormat::V0317SeedMultiRegions => Some(multi_region::V0317_END_MARKER),
        ZetaFormat::V0327SingleFile => Some(multi_region::V0327_END_MARKER),
        ZetaFormat::V0615HashRegions => Some(hashed_regions::V0615_END_MARKER),

        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304VariableEdit => None,
    }
}

mod output_encoding;
pub use output_encoding::{encode_patch_as_output_for_format, format_expected_output};

mod parsing;
pub use parsing::{
    CursorPosition, ParsedOutput, cursor_position_from_parsed_output, parse_zeta2_model_output,
    parse_zeta2_model_output_as_patch, parse_zeta3_model_output_as_patch,
    parsed_output_from_editable_region, parsed_output_to_patch,
};

pub fn excerpt_range_for_format(
    format: ZetaFormat,
    ranges: &ExcerptRanges,
) -> (Range<usize>, Range<usize>) {
    excerpt_ranges_for_format(format, ranges)
}

pub fn resolve_cursor_region(
    input: &Zeta2PromptInput,
    format: ZetaFormat,
) -> (&str, Range<usize>, Range<usize>, usize) {
    let (editable_range, context_range) = if format == ZetaFormat::V0327SingleFile {
        let (editable_tokens, _) = token_limits_for_format(format);
        let context_range = 0..input.cursor_excerpt.len();
        let editable_range = multi_region::compute_v0327_editable_range(
            &input.cursor_excerpt,
            input.cursor_offset_in_excerpt,
            editable_tokens,
        );
        (editable_range, context_range)
    } else if let Some(syntax_ranges) = &input.syntax_ranges {
        let (editable_tokens, context_tokens) = token_limits_for_format(format);
        compute_editable_and_context_ranges(
            &input.cursor_excerpt,
            input.cursor_offset_in_excerpt,
            syntax_ranges,
            editable_tokens,
            context_tokens,
        )
    } else {
        excerpt_range_for_format(format, &input.excerpt_ranges)
    };

    adjust_cursor_region(
        &input.cursor_excerpt,
        input.cursor_offset_in_excerpt,
        editable_range,
        context_range,
    )
}

fn adjust_cursor_region(
    cursor_excerpt: &str,
    cursor_offset: usize,
    editable_range: Range<usize>,
    context_range: Range<usize>,
) -> (&str, Range<usize>, Range<usize>, usize) {
    let context_start = context_range.start;
    let context_text = &cursor_excerpt[context_range.clone()];
    let adjusted_editable =
        (editable_range.start - context_start)..(editable_range.end - context_start);
    let adjusted_cursor = cursor_offset - context_start;

    (
        context_text,
        adjusted_editable,
        context_range,
        adjusted_cursor,
    )
}

pub fn get_prefill(input: &Zeta2PromptInput, format: ZetaFormat) -> String {
    let (context, editable_range, _, _) = resolve_cursor_region(input, format);
    get_prefill_for_format(format, context, &editable_range)
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

pub mod hashline;
pub mod qwen;
pub mod seed_coder;
mod v0112_middle_at_end;
mod v0113_ordered;
mod v0114180_editable_region;
pub mod v0120_git_merge_markers;
pub mod v0131_git_merge_markers_prefix;
pub mod v0211_prefill;
pub mod v0304_variable_edit;

#[cfg(test)]
mod tests;
/// The zeta1 prompt format
pub mod zeta1;
