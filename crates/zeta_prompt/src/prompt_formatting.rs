use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::*;

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

pub(crate) fn zeta3_current_file_excerpt(
    input: &Zeta3PromptInput,
) -> Option<(&RelatedExcerpt, usize)> {
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

pub(crate) fn resolve_zeta3_cursor_region<'a>(
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

pub fn format_prompt_with_budget_for_format(
    input: &Zeta2PromptInput,
    format: ZetaFormat,
    max_tokens: usize,
) -> Option<String> {
    if format == ZetaFormat::V0615HashRegions {
        return hash_region_prompt::format_hash_regions_prompt_with_budget(
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
    Some(prompt)
}
