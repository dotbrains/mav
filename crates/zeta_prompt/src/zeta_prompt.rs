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

mod hash_region_prompt;

mod prompt_formatting;
pub use prompt_formatting::{
    format_prompt_with_budget_for_format, format_zeta_prompt, format_zeta3_prompt,
    prompt_input_contains_special_tokens,
};
pub(crate) use prompt_formatting::{resolve_zeta3_cursor_region, zeta3_current_file_excerpt};

mod context_rendering;
pub use context_rendering::{
    filter_redundant_excerpts, format_active_buffer_diagnostics_with_budget,
    format_edit_history_within_budget, format_related_files_within_budget,
    rows_omitted_after_excerpt, write_related_files,
};

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
