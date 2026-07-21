use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::*;

pub fn write_cursor_excerpt_section_for_format(
    format: ZetaFormat,
    prompt: &mut String,
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) {
    match format {
        ZetaFormat::V0112MiddleAtEnd => v0112_middle_at_end::write_cursor_excerpt_section(
            prompt,
            path,
            context,
            editable_range,
            cursor_offset,
        ),
        ZetaFormat::V0113Ordered | ZetaFormat::V0114180EditableRegion => {
            v0113_ordered::write_cursor_excerpt_section(
                prompt,
                path,
                context,
                editable_range,
                cursor_offset,
            )
        }
        ZetaFormat::V0120GitMergeMarkers => v0120_git_merge_markers::write_cursor_excerpt_section(
            prompt,
            path,
            context,
            editable_range,
            cursor_offset,
        ),
        ZetaFormat::V0131GitMergeMarkersPrefix | ZetaFormat::V0211Prefill => {
            v0131_git_merge_markers_prefix::write_cursor_excerpt_section(
                prompt,
                path,
                context,
                editable_range,
                cursor_offset,
            )
        }
        ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0304SeedNoEdits => seed_coder::write_cursor_excerpt_section(
            prompt,
            path,
            context,
            editable_range,
            cursor_offset,
        ),
        ZetaFormat::v0226Hashline => hashline::write_cursor_excerpt_section(
            prompt,
            path,
            context,
            editable_range,
            cursor_offset,
        ),
        ZetaFormat::V0304VariableEdit => {
            v0304_variable_edit::write_cursor_excerpt_section(prompt, path, context, cursor_offset)
        }
        ZetaFormat::V0306SeedMultiRegions => {
            prompt.push_str(&build_v0306_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
            ));
        }
        ZetaFormat::V0316SeedMultiRegions => {
            prompt.push_str(&build_v0316_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
            ));
        }
        ZetaFormat::V0318SeedMultiRegions | ZetaFormat::V0420Diagnostics => {
            prompt.push_str(&build_v0318_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
                seed_coder::FILE_MARKER,
            ));
        }
        ZetaFormat::V0608QwenMultiRegions => {
            prompt.push_str(&build_v0318_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
                qwen::FILE_MARKER,
            ));
        }
        ZetaFormat::V0317SeedMultiRegions => {
            prompt.push_str(&build_v0317_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
            ));
        }
        ZetaFormat::V0327SingleFile => {
            prompt.push_str(&build_v0318_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
                seed_coder::FILE_MARKER,
            ));
        }
        ZetaFormat::V0615HashRegions => {
            prompt.push_str(&build_v0615_cursor_prefix(
                path,
                context,
                editable_range,
                cursor_offset,
            ));
        }
    }
}

fn build_v0306_cursor_prefix(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", seed_coder::FILE_MARKER, path_str).ok();

    section.push_str(&context[..editable_range.start]);
    section.push_str(seed_coder::START_MARKER);

    let editable_text = &context[editable_range.clone()];
    let cursor_in_editable = cursor_offset - editable_range.start;
    multi_region::write_editable_with_markers(
        &mut section,
        editable_text,
        cursor_in_editable,
        CURSOR_MARKER,
    );

    if !section.ends_with('\n') {
        section.push('\n');
    }
    section.push_str(seed_coder::SEPARATOR);
    section
}

fn build_v0316_cursor_prefix(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", seed_coder::FILE_MARKER, path_str).ok();

    section.push_str(&context[..editable_range.start]);

    let editable_text = &context[editable_range.clone()];
    let cursor_in_editable = cursor_offset - editable_range.start;
    multi_region::write_editable_with_markers_v0316(
        &mut section,
        editable_text,
        cursor_in_editable,
        CURSOR_MARKER,
    );

    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}

fn build_v0318_cursor_prefix(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
    file_marker: &str,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", file_marker, path_str).ok();

    section.push_str(&context[..editable_range.start]);

    let editable_text = &context[editable_range.clone()];
    let cursor_in_editable = cursor_offset - editable_range.start;
    multi_region::write_editable_with_markers_v0318(
        &mut section,
        editable_text,
        cursor_in_editable,
        CURSOR_MARKER,
    );

    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}

fn build_v0615_cursor_prefix(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", seed_coder::FILE_MARKER, path_str).ok();

    section.push_str(&context[..editable_range.start]);

    let editable_text = &context[editable_range.clone()];
    let cursor_in_editable = cursor_offset - editable_range.start;
    let markers = hashed_regions::markers_for_text(editable_text);
    hashed_regions::write_snippet_with_markers(
        &mut section,
        editable_text,
        &markers,
        Some((cursor_in_editable, CURSOR_MARKER)),
    );

    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}

fn build_v0317_cursor_prefix(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", seed_coder::FILE_MARKER, path_str).ok();

    section.push_str(&context[..editable_range.start]);

    let editable_text = &context[editable_range.clone()];
    let cursor_in_editable = cursor_offset - editable_range.start;
    multi_region::write_editable_with_markers_v0317(
        &mut section,
        editable_text,
        cursor_in_editable,
        CURSOR_MARKER,
    );

    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}

pub fn offset_range_to_row_range(text: &str, range: Range<usize>) -> Range<u32> {
    let start_row = text[0..range.start].matches('\n').count() as u32;
    let mut end_row = start_row + text[range.clone()].matches('\n').count() as u32;
    if !text[..range.end].ends_with('\n') {
        end_row += 1;
    }
    start_row..end_row
}

pub fn assemble_single_file_fim_prompt(
    context: &str,
    editable_range: &Range<usize>,
    cursor_prefix_section: &str,
    events: &[Arc<Event>],
    max_tokens: usize,
) -> String {
    let suffix_section = seed_coder::build_suffix_section(context, editable_range);

    let suffix_tokens = estimate_tokens(suffix_section.len() + seed_coder::FIM_PREFIX.len());
    let cursor_prefix_tokens =
        estimate_tokens(cursor_prefix_section.len() + seed_coder::FIM_MIDDLE.len());
    let budget_after_cursor = max_tokens.saturating_sub(suffix_tokens + cursor_prefix_tokens);

    let edit_history_section = format_edit_history_within_budget(
        events,
        seed_coder::FILE_MARKER,
        "edit_history",
        budget_after_cursor,
        max_edit_event_count_for_format(&ZetaFormat::V0327SingleFile),
    );

    let mut prompt = String::new();
    prompt.push_str(&suffix_section);
    prompt.push_str(seed_coder::FIM_PREFIX);
    prompt.push_str(&edit_history_section);
    if !edit_history_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(cursor_prefix_section);
    prompt.push_str(seed_coder::FIM_MIDDLE);
    prompt
}
