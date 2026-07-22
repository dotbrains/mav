use super::*;

pub(super) fn build_slash_command_label(
    command: &AvailableCommand,
    source_highlight_id: Option<HighlightId>,
) -> CodeLabel {
    build_slash_item_label(&command.name, command.source.as_ref(), source_highlight_id)
}

/// Build the autocomplete-popup label for a slash menu item, appending
/// the item origin after the name when one is present and non-empty.
/// The suffix is styled with the muted `variable` highlight and excluded
/// from the fuzzy filter range so typing the source doesn't match the entry.
fn build_slash_item_label(
    name: &Arc<str>,
    source: Option<&SharedString>,
    source_highlight_id: Option<HighlightId>,
) -> CodeLabel {
    let source = source.filter(|source| !source.is_empty());
    let Some(source) = source else {
        return CodeLabel::plain(name.to_string(), None);
    };
    let mut builder = CodeLabelBuilder::default();
    builder.push_str(name, None);
    builder.push_str(" ", None);
    builder.push_str(source, source_highlight_id);
    // The filter range defaults to the entire label after `build()`,
    // which would let the source text participate in fuzzy filtering.
    // Slash commands are matched up-front in `search_slash_commands`
    // against the command name, and the editor doesn't re-filter
    // (`filter_completions()` is false), so this is mostly defensive
    // — but it keeps the displayed filter consistent with what we
    // actually matched against.
    builder.respan_filter_range(Some(name));
    builder.build()
}

pub(super) fn build_code_label_for_path(
    file: &str,
    directory: Option<&str>,
    line_number: Option<u32>,
    label_max_chars: usize,
    cx: &App,
) -> CodeLabel {
    let variable_highlight_id = cx
        .theme()
        .syntax()
        .highlight_id("variable")
        .map(HighlightId::new);
    let mut label = CodeLabelBuilder::default();

    label.push_str(file, None);
    label.push_str(" ", None);

    if let Some(directory) = directory {
        let file_name_chars = file.chars().count();
        // Account for: file_name + space (ellipsis is handled by truncate_and_remove_front)
        let directory_max_chars = label_max_chars
            .saturating_sub(file_name_chars)
            .saturating_sub(1);
        let truncated_directory = truncate_and_remove_front(directory, directory_max_chars.max(5));
        label.push_str(&truncated_directory, variable_highlight_id);
    }
    if let Some(line_number) = line_number {
        label.push_str(&format!(" L{}", line_number), variable_highlight_id);
    }
    label.build()
}
