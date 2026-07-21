pub mod excerpt_ranges;
pub mod hashed_regions;
pub mod multi_region;
pub mod udiff;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use strum::{EnumIter, IntoEnumIterator as _, IntoStaticStr};

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

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct Zeta2PromptInput {
    pub cursor_path: Arc<Path>,
    pub cursor_excerpt: Arc<str>,
    pub cursor_offset_in_excerpt: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt_start_row: Option<u32>,
    pub events: Vec<Arc<Event>>,
    #[serde(default)]
    pub related_files: Option<Vec<RelatedFile>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_buffer_diagnostics: Vec<ActiveBufferDiagnostic>,
    /// These ranges let the server select model-appropriate subsets.
    pub excerpt_ranges: ExcerptRanges,
    /// Byte offset ranges within `cursor_excerpt` for all syntax nodes that
    /// contain `cursor_offset_in_excerpt`, ordered from innermost to outermost.
    /// When present, the server uses these to compute editable/context ranges
    /// instead of `excerpt_ranges`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax_ranges: Option<Vec<Range<usize>>>,
    #[serde(default)]
    pub in_open_source_repo: bool,
    #[serde(default)]
    pub can_collect_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FilePosition {
    pub row: u32,
    pub column: u32,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct Zeta3PromptInput {
    pub cursor_path: Arc<Path>,
    pub cursor_position: FilePosition,
    pub events: Vec<Arc<Event>>,
    pub editable_context: Vec<RelatedFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syntax_ranges: Vec<Range<usize>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_buffer_diagnostics: Vec<ActiveBufferDiagnostic>,
    #[serde(default)]
    pub in_open_source_repo: bool,
    #[serde(default)]
    pub can_collect_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
}

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[allow(non_camel_case_types)]
pub enum ZetaFormat {
    V0112MiddleAtEnd,
    V0113Ordered,
    V0114180EditableRegion,
    V0120GitMergeMarkers,
    #[default]
    V0131GitMergeMarkersPrefix,
    V0211Prefill,
    #[serde(alias = "Zeta2")]
    V0211SeedCoder,
    V0331SeedCoderModelPy,
    v0226Hashline,
    V0304VariableEdit,
    V0304SeedNoEdits,
    /// Multi-block marker spans with NO_EDITS sentinel.
    V0306SeedMultiRegions,
    /// Byte-exact marker spans; all intermediate markers emitted; repeated marker means no-edit.
    V0316SeedMultiRegions,
    /// V0316, but marker numbers are relative to the cursor block (e.g. -1, -0, +1).
    V0317SeedMultiRegions,
    /// V0316 with larger block sizes.
    #[serde(alias = "Zeta2.1")]
    V0318SeedMultiRegions,
    /// V0318-style markers over the full available current file excerpt with no related files.
    V0327SingleFile,
    /// V0318-style prompt with buffer diagnostics
    V0420Diagnostics,
    /// V0318-style multi-region format using Qwen FIM tokens and PSM ordering.
    V0608QwenMultiRegions,

    /// V0318-style marker-span output, but with content-hashed marker tags over rendered
    /// related-file context so the model can target jump edits. There is no cursor-centered
    /// editable region for this format.
    V0615HashRegions,
}

impl std::fmt::Display for ZetaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <&'static str>::from(self))
    }
}

impl ZetaFormat {
    pub fn parse(format_name: &str) -> Result<Self> {
        let lower = format_name.to_lowercase();

        // Exact case-insensitive match takes priority, bypassing ambiguity checks.
        for variant in ZetaFormat::iter() {
            if <&'static str>::from(&variant).to_lowercase() == lower {
                return Ok(variant);
            }
        }

        let mut results = ZetaFormat::iter().filter(|version| {
            <&'static str>::from(version)
                .to_lowercase()
                .contains(&lower)
        });
        let Some(result) = results.next() else {
            anyhow::bail!(
                "`{format_name}` did not match any of:\n{}",
                Self::options_as_string()
            );
        };
        if results.next().is_some() {
            anyhow::bail!(
                "`{format_name}` matched more than one of:\n{}",
                Self::options_as_string()
            );
        }
        Ok(result)
    }

    pub fn options_as_string() -> String {
        ZetaFormat::iter()
            .map(|format| format!("- {}\n", <&'static str>::from(format)))
            .collect::<Vec<_>>()
            .concat()
    }
}

fn empty_range() -> Range<usize> {
    0..0
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    BufferChange {
        path: Arc<Path>,
        old_path: Arc<Path>,
        diff: String,
        #[serde(default = "empty_range")]
        old_range: Range<usize>,
        #[serde(default = "empty_range")]
        new_range: Range<usize>,
        predicted: bool,
        in_open_source_repo: bool,
    },
}

impl Event {
    pub fn in_open_source_repo(&self) -> bool {
        match self {
            Event::BufferChange {
                in_open_source_repo,
                ..
            } => *in_open_source_repo,
        }
    }
}

pub fn write_event(prompt: &mut String, event: &Event) {
    fn write_path_as_unix_str(prompt: &mut String, path: &Path) {
        for component in path.components() {
            prompt.push('/');
            write!(prompt, "{}", component.as_os_str().display()).ok();
        }
    }
    match event {
        Event::BufferChange {
            path,
            old_path,
            diff,
            predicted,
            ..
        } => {
            if *predicted {
                prompt.push_str("// User accepted prediction:\n");
            }
            prompt.push_str("--- a");
            write_path_as_unix_str(prompt, old_path.as_ref());
            prompt.push_str("\n+++ b");
            write_path_as_unix_str(prompt, path.as_ref());
            prompt.push('\n');
            prompt.push_str(diff);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct ActiveBufferDiagnostic {
    pub severity: Option<i32>,
    pub message: String,
    pub snippet: String,
    pub snippet_buffer_row_range: Range<u32>,
    pub diagnostic_range_in_snippet: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct RelatedFile {
    pub path: Arc<Path>,
    pub max_row: u32,
    pub excerpts: Vec<RelatedExcerpt>,
    #[serde(default)]
    pub in_open_source_repo: bool,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct RelatedExcerpt {
    pub row_range: Range<u32>,
    pub text: Arc<str>,
    #[serde(default)]
    pub order: usize,
    #[serde(default)]
    pub context_source: ContextSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    #[default]
    Lsp,
    CursorExcerpt,
    CurrentFile,
    EditHistory,
    EditHistoryFile,
    GitLog,
    Bm25,
    OracleFile,
    OracleSnippet,
}

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

fn offset_range_to_row_range(text: &str, range: Range<usize>) -> Range<u32> {
    let start_row = text[0..range.start].matches('\n').count() as u32;
    let mut end_row = start_row + text[range.clone()].matches('\n').count() as u32;
    if !text[..range.end].ends_with('\n') {
        end_row += 1;
    }
    return start_row..end_row;
}

fn assemble_single_file_fim_prompt(
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
