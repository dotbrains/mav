use super::*;
use std::fmt::Write;

pub const CURSOR_MARKER: &str = "<|user_cursor_is_here|>";
pub const START_OF_FILE_MARKER: &str = "<|start_of_file|>";
pub const EDITABLE_REGION_START_MARKER: &str = "<|editable_region_start|>";
pub const EDITABLE_REGION_END_MARKER: &str = "<|editable_region_end|>";

const INSTRUCTION_HEADER: &str = concat!(
    "### Instruction:\n",
    "You are a code completion assistant and your task is to analyze user edits and then rewrite an ",
    "excerpt that the user provides, suggesting the appropriate edits within the excerpt, taking ",
    "into account the cursor location.\n\n",
    "### User Edits:\n\n"
);
const EXCERPT_HEADER: &str = "\n\n### User Excerpt:\n\n";
const RESPONSE_HEADER: &str = "\n\n### Response:\n";

/// Formats a complete zeta1 prompt from the input events and excerpt.
pub fn format_zeta1_prompt(input_events: &str, input_excerpt: &str) -> String {
    let mut prompt = String::with_capacity(
        INSTRUCTION_HEADER.len()
            + input_events.len()
            + EXCERPT_HEADER.len()
            + input_excerpt.len()
            + RESPONSE_HEADER.len(),
    );
    prompt.push_str(INSTRUCTION_HEADER);
    prompt.push_str(input_events);
    prompt.push_str(EXCERPT_HEADER);
    prompt.push_str(input_excerpt);
    prompt.push_str(RESPONSE_HEADER);
    prompt
}

/// Formats a complete zeta1 prompt from a `Zeta2PromptInput` using the given
/// editable and context byte-offset ranges within `cursor_excerpt`.
pub fn format_zeta1_from_input(
    input: &Zeta2PromptInput,
    editable_range: Range<usize>,
    context_range: Range<usize>,
) -> String {
    let events = format_zeta1_events(&input.events);
    let excerpt = format_zeta1_excerpt(input, editable_range, context_range);
    format_zeta1_prompt(&events, &excerpt)
}

/// Formats events in zeta1 style (oldest first).
fn format_zeta1_events(events: &[Arc<Event>]) -> String {
    let mut result = String::new();
    for event in events
        .iter()
        .skip(events.len().saturating_sub(max_edit_event_count_for_format(
            &ZetaFormat::V0114180EditableRegion,
        )))
    {
        let event_string = format_zeta1_event(event);
        if event_string.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str(&event_string);
    }
    result
}

fn format_zeta1_event(event: &Event) -> String {
    match event {
        Event::BufferChange {
            path,
            old_path,
            diff,
            ..
        } => {
            let mut prompt = String::new();
            if old_path != path {
                writeln!(
                    prompt,
                    "User renamed {} to {}\n",
                    old_path.display(),
                    path.display()
                )
                .ok();
            }
            if !diff.is_empty() {
                write!(
                    prompt,
                    "User edited {}:\n```diff\n{}\n```",
                    path.display(),
                    diff
                )
                .ok();
            }
            prompt
        }
    }
}

/// Formats the excerpt section of a zeta1 prompt using byte-offset ranges
/// within `cursor_excerpt`.
fn format_zeta1_excerpt(
    input: &Zeta2PromptInput,
    editable_range: Range<usize>,
    context_range: Range<usize>,
) -> String {
    let path_str = input.cursor_path.to_string_lossy();
    let excerpt = &*input.cursor_excerpt;
    let cursor_offset = input.cursor_offset_in_excerpt;

    let mut prompt = String::new();
    writeln!(&mut prompt, "```{path_str}").ok();

    let starts_at_file_beginning = input.excerpt_start_row == Some(0) && context_range.start == 0;
    if starts_at_file_beginning {
        writeln!(&mut prompt, "{START_OF_FILE_MARKER}").ok();
    }

    prompt.push_str(&excerpt[context_range.start..editable_range.start]);

    writeln!(&mut prompt, "{EDITABLE_REGION_START_MARKER}").ok();
    prompt.push_str(&excerpt[editable_range.start..cursor_offset]);
    prompt.push_str(CURSOR_MARKER);
    prompt.push_str(&excerpt[cursor_offset..editable_range.end]);
    write!(&mut prompt, "\n{EDITABLE_REGION_END_MARKER}").ok();

    prompt.push_str(&excerpt[editable_range.end..context_range.end]);
    write!(prompt, "\n```").ok();

    prompt
}

/// Cleans zeta1 model output by extracting content between editable region
/// markers and converting the zeta1 cursor marker to the universal one.
/// Returns `None` if the output doesn't contain the expected markers.
pub fn clean_zeta1_model_output(output: &str) -> Option<String> {
    let content = output.replace(CURSOR_MARKER, "");

    let content_start = content
        .find(EDITABLE_REGION_START_MARKER)
        .map(|pos| pos + EDITABLE_REGION_START_MARKER.len())
        .map(|pos| {
            if content.as_bytes().get(pos) == Some(&b'\n') {
                pos + 1
            } else {
                pos
            }
        })
        .unwrap_or(0);

    let content_end = content
        .find(EDITABLE_REGION_END_MARKER)
        .map(|pos| {
            if pos > 0 && content.as_bytes().get(pos - 1) == Some(&b'\n') {
                pos - 1
            } else {
                pos
            }
        })
        .unwrap_or(content.len());

    if content_start > content_end {
        return Some(String::new());
    }

    let extracted = &content[content_start..content_end];

    let cursor_offset = output.find(CURSOR_MARKER).map(|zeta1_cursor_pos| {
        let text_before_cursor = output[..zeta1_cursor_pos].replace(CURSOR_MARKER, "");
        let text_before_cursor = text_before_cursor
            .find(EDITABLE_REGION_START_MARKER)
            .map(|pos| {
                let after_marker = pos + EDITABLE_REGION_START_MARKER.len();
                if text_before_cursor.as_bytes().get(after_marker) == Some(&b'\n') {
                    after_marker + 1
                } else {
                    after_marker
                }
            })
            .unwrap_or(0);
        let offset_in_extracted = zeta1_cursor_pos
            .saturating_sub(text_before_cursor)
            .min(extracted.len());
        offset_in_extracted
    });

    let mut result = String::with_capacity(extracted.len() + super::CURSOR_MARKER.len());
    if let Some(offset) = cursor_offset {
        result.push_str(&extracted[..offset]);
        result.push_str(super::CURSOR_MARKER);
        result.push_str(&extracted[offset..]);
    } else {
        result.push_str(extracted);
    }

    Some(result)
}
