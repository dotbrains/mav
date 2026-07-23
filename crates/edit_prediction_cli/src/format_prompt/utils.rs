use super::*;

pub(crate) fn line_start_offset(text: &str, row: usize) -> Option<usize> {
    let mut offset = 0;
    for _ in 0..row {
        offset += text[offset..].find('\n')? + 1;
    }
    Some(offset)
}

/// Extract the cursor excerpt from an example.
/// First tries to extract from an existing prompt, then falls back to constructing from prompt_inputs.
pub fn extract_cursor_excerpt_from_example(example: &Example) -> Option<String> {
    // If we have the original prompt, extract the cursor excerpt from it
    if let Some(prompt) = &example.prompt {
        // Find "# 3. Current File" section and extract the content
        if let Some(start) = prompt.input.find("# 3. Current File") {
            let content_start = prompt.input[start..].find('`').map(|i| start + i)?;
            let backtick_count = prompt.input[content_start..]
                .chars()
                .take_while(|&c| c == '`')
                .count();
            let content_start = content_start + backtick_count;

            // Find the path line and skip it
            let newline_pos = prompt.input[content_start..].find('\n')?;
            let text_start = content_start + newline_pos + 1;

            // Find the closing backticks
            let closing_pattern = "`".repeat(backtick_count);
            let text_end = prompt.input[text_start..].find(&closing_pattern)?;
            let cursor_excerpt = &prompt.input[text_start..text_start + text_end];

            let path_str = example.spec.cursor_path.to_string_lossy();
            return Some(format!("`````{path_str}\n{cursor_excerpt}`````"));
        }
    }

    // Fallback: construct from prompt_inputs if available
    let prompt_inputs = example.prompt_inputs.as_ref()?;
    let excerpt = prompt_inputs.cursor_excerpt.as_ref();
    let cursor_offset = prompt_inputs.cursor_offset_in_excerpt;

    // Simple fallback: just show content around cursor with markers
    let path_str = example.spec.cursor_path.to_string_lossy();
    let mut result = format!("`````{path_str}\n");
    result.push_str(TeacherPrompt::EDITABLE_REGION_START);
    result.push_str(&excerpt[..cursor_offset]);
    result.push_str(TeacherPrompt::USER_CURSOR_MARKER);
    result.push_str(&excerpt[cursor_offset..]);
    result.push_str(TeacherPrompt::EDITABLE_REGION_END);
    result.push_str("\n`````");

    Some(result)
}

/// Extract all top-level fenced codeblocks from `text`, in order.
///
/// A fence opens with 3+ backticks (optionally followed by an info string)
/// and closes with a line of at least as many backticks, so codeblocks that
/// themselves contain shorter fences are handled.
pub(crate) fn extract_all_codeblocks(text: &str) -> Vec<String> {
    let mut codeblocks = Vec::new();
    let mut current_block: Option<(usize, Vec<&str>)> = None;

    for line in text.lines() {
        match &mut current_block {
            None => {
                let backtick_count = line.chars().take_while(|&c| c == '`').count();
                if backtick_count >= 3 {
                    current_block = Some((backtick_count, Vec::new()));
                }
            }
            Some((opening_count, lines)) => {
                let trimmed = line.trim();
                if trimmed.len() >= *opening_count && trimmed.chars().all(|c| c == '`') {
                    let mut content = lines.join("\n");
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    codeblocks.push(content);
                    current_block = None;
                } else {
                    lines.push(line);
                }
            }
        }
    }

    codeblocks
}

pub(crate) fn extract_last_codeblock(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();

    // Search from the end for a closing fence (line containing only backticks, 3+)
    let mut closing_line_idx = None;
    let mut backtick_count = 0;

    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.len() >= 3 && line.chars().all(|c| c == '`') {
            closing_line_idx = Some(i);
            backtick_count = line.len();
            break;
        }
    }

    let closing_idx = closing_line_idx?;

    // Search backwards for matching opening fence
    // Opening fence starts with same backtick count, possibly followed by language/metadata
    let opening_pattern = "`".repeat(backtick_count);

    for i in (0..closing_idx).rev() {
        let line = lines[i];
        if line.starts_with(&opening_pattern) {
            // Ensure it's exactly the right number of backticks (not more)
            let rest = &line[backtick_count..];
            if rest.is_empty() || !rest.starts_with('`') {
                // Found matching opening fence
                // Extract content between opening and closing (exclusive)
                if closing_idx > i + 1 {
                    let content = lines[i + 1..closing_idx].join("\n");
                    // Preserve trailing newline to match previous behavior
                    return Some(format!("{}\n", content));
                } else {
                    // Empty block
                    return Some(String::new());
                }
            }
        }
    }

    None
}
