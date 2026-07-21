use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::Path;

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParsedOutput {
    /// Text that should replace the editable region
    pub new_editable_region: String,
    /// The byte range within `cursor_excerpt` that this replacement applies to
    pub range_in_excerpt: Range<usize>,
    /// Byte offset of the cursor marker within `new_editable_region`, if present
    pub cursor_offset_in_new_editable_region: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CursorPosition {
    pub path: String,
    pub row: usize,
    pub column: usize,
    pub offset: usize,
    pub editable_region_offset: usize,
}

pub fn parsed_output_from_editable_region(
    range_in_excerpt: Range<usize>,
    mut new_editable_region: String,
) -> ParsedOutput {
    let cursor_offset_in_new_editable_region = new_editable_region.find(CURSOR_MARKER);
    if let Some(offset) = cursor_offset_in_new_editable_region {
        new_editable_region.replace_range(offset..offset + CURSOR_MARKER.len(), "");
    }

    ParsedOutput {
        new_editable_region,
        range_in_excerpt,
        cursor_offset_in_new_editable_region,
    }
}

/// Parse model output for the given zeta format
pub fn parse_zeta2_model_output(
    output: &str,
    format: ZetaFormat,
    prompt_inputs: &Zeta2PromptInput,
) -> Result<ParsedOutput> {
    let output = match output_end_marker_for_format(format) {
        Some(marker) => output.strip_suffix(marker).unwrap_or(output),
        None => output,
    };

    let (context, editable_range_in_context, context_range, cursor_offset) =
        resolve_cursor_region(prompt_inputs, format);
    let context_start = context_range.start;
    let old_editable_region = &context[editable_range_in_context.clone()];
    let cursor_offset_in_editable = cursor_offset.saturating_sub(editable_range_in_context.start);

    let (range_in_context, output) = match format {
        ZetaFormat::v0226Hashline => (
            editable_range_in_context,
            if hashline::output_has_edit_commands(output) {
                hashline::apply_edit_commands(old_editable_region, output)
            } else {
                output.to_string()
            },
        ),
        ZetaFormat::V0304VariableEdit => v0304_variable_edit::apply_variable_edit(context, output)?,
        ZetaFormat::V0304SeedNoEdits => (
            editable_range_in_context,
            if output.starts_with(seed_coder::NO_EDITS) {
                old_editable_region.to_string()
            } else {
                output.to_string()
            },
        ),
        ZetaFormat::V0306SeedMultiRegions => (
            editable_range_in_context,
            if output.starts_with(seed_coder::NO_EDITS) {
                old_editable_region.to_string()
            } else {
                multi_region::apply_marker_span(old_editable_region, output)?
            },
        ),
        ZetaFormat::V0316SeedMultiRegions => (
            editable_range_in_context,
            multi_region::apply_marker_span_v0316(old_editable_region, output)?,
        ),
        ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0608QwenMultiRegions => (
            editable_range_in_context,
            multi_region::apply_marker_span_v0318(old_editable_region, output)?,
        ),
        ZetaFormat::V0317SeedMultiRegions => (
            editable_range_in_context,
            multi_region::apply_marker_span_v0317(
                old_editable_region,
                output,
                Some(cursor_offset_in_editable),
            )?,
        ),
        ZetaFormat::V0327SingleFile => (
            editable_range_in_context,
            multi_region::apply_marker_span_v0318(old_editable_region, output)?,
        ),
        ZetaFormat::V0615HashRegions => {
            anyhow::bail!(
                "V0615HashRegions output addresses related-file context; use parse_zeta2_model_output_as_patch"
            )
        }
        _ => (editable_range_in_context, output.to_string()),
    };

    let range_in_excerpt =
        range_in_context.start + context_start..range_in_context.end + context_start;

    Ok(parsed_output_from_editable_region(range_in_excerpt, output))
}

pub fn parse_zeta2_model_output_as_patch(
    output: &str,
    format: ZetaFormat,
    prompt_inputs: &Zeta2PromptInput,
) -> Result<String> {
    if format == ZetaFormat::V0615HashRegions {
        return hashed_regions::parse_output_as_patch(prompt_inputs, output, CURSOR_MARKER);
    }

    let parsed = parse_zeta2_model_output(output, format, prompt_inputs)?;
    parsed_output_to_patch(prompt_inputs, parsed)
}

pub fn parse_zeta3_model_output_as_patch(
    output: &str,
    format: ZetaFormat,
    input: &Zeta3PromptInput,
) -> Result<String> {
    match format {
        ZetaFormat::V0318SeedMultiRegions => {}
        _ => anyhow::bail!("unsupported Zeta3 output format: {format}"),
    }

    let output = match output_end_marker_for_format(format) {
        Some(marker) => output.strip_suffix(marker).unwrap_or(output),
        None => output,
    };
    let (current_excerpt, cursor_offset_in_excerpt) = zeta3_current_file_excerpt(input)
        .ok_or_else(|| anyhow!("Zeta3 input is missing current-file editable context at cursor"))?;
    let (context, editable_range_in_context, context_range, _) = resolve_zeta3_cursor_region(
        current_excerpt.text.as_ref(),
        cursor_offset_in_excerpt,
        &input.syntax_ranges,
        format,
    );
    let old_editable_region = &context[editable_range_in_context.clone()];
    let range_in_excerpt = editable_range_in_context.start + context_range.start
        ..editable_range_in_context.end + context_range.start;
    let parsed = parsed_output_from_editable_region(
        range_in_excerpt,
        multi_region::apply_marker_span_v0318(old_editable_region, output)?,
    );

    parsed_output_to_patch_for_excerpt(
        input.cursor_path.as_ref(),
        current_excerpt.text.as_ref(),
        current_excerpt.row_range.start,
        parsed,
    )
}

pub fn cursor_position_from_parsed_output(
    prompt_inputs: &Zeta2PromptInput,
    parsed: &ParsedOutput,
) -> Option<CursorPosition> {
    let cursor_offset = parsed.cursor_offset_in_new_editable_region?;
    let editable_region_offset = parsed.range_in_excerpt.start;
    let excerpt = prompt_inputs.cursor_excerpt.as_ref();

    let editable_region_start_line = excerpt[..editable_region_offset].matches('\n').count();

    let new_editable_region = &parsed.new_editable_region;
    let prefix_end = cursor_offset.min(new_editable_region.len());
    let new_region_prefix = &new_editable_region[..prefix_end];

    let row = editable_region_start_line + new_region_prefix.matches('\n').count();

    let column = match new_region_prefix.rfind('\n') {
        Some(last_newline) => cursor_offset - last_newline - 1,
        None => {
            let content_prefix = &excerpt[..editable_region_offset];
            let content_column = match content_prefix.rfind('\n') {
                Some(last_newline) => editable_region_offset - last_newline - 1,
                None => editable_region_offset,
            };
            content_column + cursor_offset
        }
    };

    Some(CursorPosition {
        path: prompt_inputs.cursor_path.to_string_lossy().into_owned(),
        row,
        column,
        offset: editable_region_offset + cursor_offset,
        editable_region_offset: cursor_offset,
    })
}

pub fn parsed_output_to_patch(
    prompt_inputs: &Zeta2PromptInput,
    parsed: ParsedOutput,
) -> Result<String> {
    parsed_output_to_patch_for_excerpt(
        prompt_inputs.cursor_path.as_ref(),
        prompt_inputs.cursor_excerpt.as_ref(),
        0,
        parsed,
    )
}

fn parsed_output_to_patch_for_excerpt(
    path: &Path,
    excerpt: &str,
    excerpt_start_row: u32,
    parsed: ParsedOutput,
) -> Result<String> {
    let range_in_excerpt = parsed.range_in_excerpt;
    let old_text = excerpt[range_in_excerpt.clone()].to_string();
    let mut new_text = parsed.new_editable_region;

    let mut old_text_normalized = old_text;
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    if !old_text_normalized.is_empty() && !old_text_normalized.ends_with('\n') {
        old_text_normalized.push('\n');
    }

    let editable_region_offset = range_in_excerpt.start;
    let editable_region_start_line =
        excerpt_start_row + excerpt[..editable_region_offset].matches('\n').count() as u32;
    let editable_region_lines = old_text_normalized.lines().count() as u32;

    let diff = udiff::unified_diff_with_context(
        &old_text_normalized,
        &new_text,
        editable_region_start_line,
        editable_region_start_line,
        editable_region_lines,
    );

    let path = path.to_string_lossy().trim_start_matches('/').to_string();
    let formatted_diff = format!("--- a/{path}\n+++ b/{path}\n{diff}");

    Ok(udiff::encode_cursor_in_patch(
        &formatted_diff,
        parsed.cursor_offset_in_new_editable_region,
    ))
}
