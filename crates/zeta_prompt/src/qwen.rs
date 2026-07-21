use super::*;

pub const FIM_PREFIX: &str = "<|fim_prefix|>";
pub const FIM_SUFFIX: &str = "<|fim_suffix|>";
pub const FIM_MIDDLE: &str = "<|fim_middle|>";
pub const FILE_MARKER: &str = "<|file_sep|>";
pub const END_MARKER: &str = "<|im_end|>";

pub fn assemble_fim_prompt(
    context: &str,
    editable_range: &Range<usize>,
    cursor_prefix_section: &str,
    events: &[Arc<Event>],
    related_files: &[RelatedFile],
    max_tokens: usize,
) -> String {
    let suffix_section = build_suffix_section(context, editable_range);

    let cursor_prefix_tokens = estimate_tokens(cursor_prefix_section.len() + FIM_PREFIX.len());
    let suffix_tokens = estimate_tokens(suffix_section.len() + FIM_SUFFIX.len() + FIM_MIDDLE.len());
    let budget_after_cursor = max_tokens.saturating_sub(cursor_prefix_tokens + suffix_tokens);

    let edit_history_section = super::format_edit_history_within_budget(
        events,
        FILE_MARKER,
        "edit_history",
        budget_after_cursor,
        max_edit_event_count_for_format(&ZetaFormat::V0608QwenMultiRegions),
    );
    let edit_history_tokens = estimate_tokens(edit_history_section.len() + "\n".len());
    let budget_after_edit_history = budget_after_cursor.saturating_sub(edit_history_tokens);

    let related_files_section = super::format_related_files_within_budget(
        related_files,
        FILE_MARKER,
        "",
        budget_after_edit_history,
    );

    let mut prompt = String::new();
    prompt.push_str(FIM_PREFIX);
    prompt.push_str(&related_files_section);
    if !related_files_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(&edit_history_section);
    if !edit_history_section.is_empty() {
        prompt.push('\n');
    }
    prompt.push_str(cursor_prefix_section);
    prompt.push_str(&suffix_section);
    prompt.push_str(FIM_MIDDLE);
    prompt
}

fn build_suffix_section(context: &str, editable_range: &Range<usize>) -> String {
    let mut section = String::new();
    section.push_str(FIM_SUFFIX);
    section.push_str(&context[editable_range.end..]);
    if !section.ends_with('\n') {
        section.push('\n');
    }
    section
}
