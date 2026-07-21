//! Seed-Coder prompt format using SPM (Suffix-Prefix-Middle) FIM mode.
//!
//! Seed-Coder uses different FIM tokens and order than Qwen:
//! - SPM order: suffix comes FIRST, then prefix, then middle
//! - Tokens: `<[fim-suffix]>`, `<[fim-prefix]>`, `<[fim-middle]>`
//! - File markers: StarCoder-style `<filename>path` (single token + path)
//!
//! All context (related files, edit history) goes in the PREFIX section.
//! The suffix contains only code after the editable region.
//!
//! Example prompt:
//!
//! <[fim-suffix]>
//! code after editable region
//! <[fim-prefix]><filename>related/file.py
//! related file content
//!
//! <filename>edit_history
//! --- a/some_file.py
//! +++ b/some_file.py
//! -old
//! +new
//!
//! <filename>path/to/target_file.py
//! code before editable region
//! <<<<<<< CURRENT
//! code that
//! needs to<|user_cursor|>
//! be rewritten
//! =======
//! <[fim-middle]>
//!
//! Expected output (model generates):
//!
//! updated
//! code with
//! changes applied
//! >>>>>>> UPDATED

use super::*;

pub const FIM_SUFFIX: &str = "<[fim-suffix]>";
pub const FIM_PREFIX: &str = "<[fim-prefix]>";
pub const FIM_MIDDLE: &str = "<[fim-middle]>";
pub const FILE_MARKER: &str = "<filename>";

pub const START_MARKER: &str = "<<<<<<< CURRENT\n";
pub const SEPARATOR: &str = "=======\n";
pub const END_MARKER: &str = ">>>>>>> UPDATED\n";

pub const NO_EDITS: &str = "NO_EDITS\n";

pub fn special_tokens() -> &'static [&'static str] {
    &[
        FIM_SUFFIX,
        FIM_PREFIX,
        FIM_MIDDLE,
        FILE_MARKER,
        START_MARKER,
        SEPARATOR,
        END_MARKER,
        CURSOR_MARKER,
    ]
}

pub fn write_cursor_excerpt_section(
    prompt: &mut String,
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) {
    let section = build_cursor_prefix_section(path, context, editable_range, cursor_offset);
    prompt.push_str(&section);
}

pub fn format_prompt_with_budget(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
    events: &[Arc<Event>],
    related_files: &[RelatedFile],
    diagnostics: &[ActiveBufferDiagnostic],
    max_tokens: usize,
) -> String {
    let cursor_prefix_section =
        build_cursor_prefix_section(path, context, editable_range, cursor_offset);
    assemble_fim_prompt(
        context,
        editable_range,
        &cursor_prefix_section,
        events,
        related_files,
        diagnostics,
        None,
        max_tokens,
    )
}

pub fn assemble_fim_prompt(
    context: &str,
    editable_range: &Range<usize>,
    cursor_prefix_section: &str,
    events: &[Arc<Event>],
    related_files: &[RelatedFile],
    diagnostics: &[ActiveBufferDiagnostic],
    cursor_buffer_row: Option<u32>,
    max_tokens: usize,
) -> String {
    let suffix_section = build_suffix_section(context, editable_range);

    let suffix_tokens = estimate_tokens(suffix_section.len() + FIM_PREFIX.len());
    let cursor_prefix_tokens = estimate_tokens(cursor_prefix_section.len() + FIM_MIDDLE.len());
    let budget_after_cursor = max_tokens.saturating_sub(suffix_tokens + cursor_prefix_tokens);

    let edit_history_section = super::format_edit_history_within_budget(
        events,
        FILE_MARKER,
        "edit_history",
        budget_after_cursor,
        max_edit_event_count_for_format(&ZetaFormat::V0211SeedCoder),
    );
    let edit_history_tokens = estimate_tokens(edit_history_section.len() + "\n".len());
    let budget_after_edit_history = budget_after_cursor.saturating_sub(edit_history_tokens);

    let diagnostics_section = super::format_active_buffer_diagnostics_with_budget(
        diagnostics,
        cursor_buffer_row,
        budget_after_edit_history,
    );
    let diagnostics_tokens = estimate_tokens(diagnostics_section.len() + "\n".len());
    let budget_after_diagnostics = budget_after_edit_history.saturating_sub(diagnostics_tokens);

    let related_files_section = super::format_related_files_within_budget(
        related_files,
        FILE_MARKER,
        "",
        budget_after_diagnostics,
    );

    let mut prompt = String::new();
    prompt.push_str(&suffix_section);
    prompt.push_str(FIM_PREFIX);
    prompt.push_str(&diagnostics_section);
    if !diagnostics_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(&related_files_section);
    if !related_files_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(&edit_history_section);
    if !edit_history_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(cursor_prefix_section);
    prompt.push_str(FIM_MIDDLE);

    prompt
}

pub(crate) fn build_suffix_section(context: &str, editable_range: &Range<usize>) -> String {
    let mut section = String::new();
    section.push_str(FIM_SUFFIX);
    section.push_str(&context[editable_range.end..]);
    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}

fn build_cursor_prefix_section(
    path: &Path,
    context: &str,
    editable_range: &Range<usize>,
    cursor_offset: usize,
) -> String {
    let mut section = String::new();
    let path_str = path.to_string_lossy();
    write!(section, "{}{}\n", FILE_MARKER, path_str).ok();

    section.push_str(&context[..editable_range.start]);
    section.push_str(START_MARKER);
    section.push_str(&context[editable_range.start..cursor_offset]);
    section.push_str(CURSOR_MARKER);
    section.push_str(&context[cursor_offset..editable_range.end]);
    if !section.ends_with('\n') {
        section.push('\n');
    }
    section.push_str(SEPARATOR);
    section
}

/// Format patch as containing no changes if it's empty; otherwise return None.
pub(crate) fn no_edits(patch: &str) -> Option<String> {
    // Count lines in the patch
    let empty_patch = patch.lines().count() <= 3;
    if empty_patch {
        Some(format!("{NO_EDITS}{END_MARKER}"))
    } else {
        None
    }
}
