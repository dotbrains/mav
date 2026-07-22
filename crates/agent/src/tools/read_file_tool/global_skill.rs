use super::*;

/// Read a file under the global skills directory directly via the filesystem,
/// bypassing project/worktree resolution. Used for skill resources that live
/// outside any worktree.
///
/// Skill resources are expected to be plain text (Markdown, scripts, configs).
/// Image rendering, the action log, and the buffer-backed outline path are
/// intentionally not exercised here — those are project concerns.
pub(super) async fn read_global_skill_file(
    canonical_path: &Path,
    fs: &dyn fs::Fs,
    start_line: Option<u32>,
    end_line: Option<u32>,
    requested_path: &str,
    event_stream: &ToolCallEventStream,
) -> Result<LanguageModelToolResultContent, LanguageModelToolResultContent> {
    let content = fs.load(canonical_path).await.map_err(tool_content_err)?;

    event_stream.update_fields(acp::ToolCallUpdateFields::new().locations(vec![
        acp::ToolCallLocation::new(canonical_path)
            .line(start_line.map(|line| line.saturating_sub(1))),
    ]));

    let (raw_text, first_line_number) = if start_line.is_some() || end_line.is_some() {
        // `split_inclusive` keeps each line's terminator attached, so CRLF stays
        // CRLF and the trailing newline of the last returned line is preserved —
        // matching `Buffer::text_for_range` in the buffer-backed path.
        let (start, end) = resolve_line_range(start_line, end_line);
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let start_idx = (start as usize).saturating_sub(1).min(lines.len());
        let end_idx = (end as usize).min(lines.len()).max(start_idx);
        (lines[start_idx..end_idx].concat(), start)
    } else {
        (content, 1)
    };

    let result_text = format_with_line_numbers(&raw_text, first_line_number);

    let markdown = MarkdownCodeBlock {
        tag: requested_path,
        text: &result_text,
    }
    .to_string();
    event_stream.update_fields(acp::ToolCallUpdateFields::new().content(vec![
        acp::ToolCallContent::Content(acp::Content::new(markdown)),
    ]));

    Ok(result_text.into())
}
