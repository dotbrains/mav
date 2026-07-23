use super::*;

pub struct TeacherPrompt;

impl TeacherPrompt {
    pub(crate) const EDITABLE_REGION_START: &str = "<|editable_region_start|>\n";
    pub(crate) const EDITABLE_REGION_END: &str = "\n<|editable_region_end|>";
    pub(crate) const USER_CURSOR_MARKER: &str = "<|user_cursor|>";
    pub(crate) const NO_EDITS: &str = "NO_EDITS";

    /// Truncate edit history to this number of last lines
    const MAX_HISTORY_LINES: usize = 128;

    pub fn format_prompt(
        example: &Example,
        editable_range: Range<usize>,
        context_range: Range<usize>,
        include_diagnostics: bool,
    ) -> String {
        let edit_history = Self::format_edit_history(&example.spec.edit_history);
        let context = Self::format_context(example);
        let cursor_excerpt = Self::format_cursor_excerpt(example, editable_range, context_range);
        let diagnostics = include_diagnostics
            .then(|| Self::format_diagnostics(example))
            .map(|diagnostics| format!("# 4. Diagnostics\n\n{diagnostics}"));

        let prompt_template = crate::prompt_assets::get_prompt("teacher.md");
        let prompt = prompt_template
            .replace("{{context}}", &context)
            .replace("{{edit_history}}", &edit_history)
            .replace("{{diagnostics}}", diagnostics.as_deref().unwrap_or(""))
            .replace("{{cursor_excerpt}}", &cursor_excerpt);

        prompt
    }

    pub fn parse(example: &Example, response: &str) -> Result<(String, Option<ActualCursor>)> {
        // Check if the model indicated no edits are needed
        let no_edits = (String::new(), None);
        if let Some(last_codeblock) = extract_last_codeblock(&response) {
            if last_codeblock.trim() == Self::NO_EDITS {
                return Ok(no_edits);
            }
        }

        if response
            .trim_end_matches(&[' ', '\n', '`'])
            .ends_with(Self::NO_EDITS)
        {
            return Ok(no_edits);
        }

        // Extract updated (new) editable region from the model response.
        let new_editable_region = Self::extract_editable_region(&response)?;
        let cursor_offset = new_editable_region.find(Self::USER_CURSOR_MARKER);
        let mut new_editable_region = new_editable_region.replace(Self::USER_CURSOR_MARKER, "");
        let old_editable_region = Self::extract_editable_region(
            &example
                .prompt
                .as_ref()
                .context("example prompt missing")?
                .input,
        )?
        .replace(Self::USER_CURSOR_MARKER, "");

        let prompt_inputs = example
            .prompt_inputs
            .as_ref()
            .context("example is missing prompt inputs")?;

        // Normalize leading newlines: if old starts with newline but new doesn't,
        // prepend newline to new to preserve whitespace structure.
        // This handles the case where the model drops the leading blank line.
        if old_editable_region.starts_with('\n') && !new_editable_region.starts_with('\n') {
            new_editable_region.insert(0, '\n');
        }

        let excerpt = prompt_inputs.cursor_excerpt.as_ref();
        let (editable_region_offset, _) = excerpt
            .match_indices(&old_editable_region)
            .min_by_key(|(index, _)| index.abs_diff(prompt_inputs.cursor_offset_in_excerpt))
            .context("editable region not found in prompt content")?;
        let editable_region_start_line = excerpt[..editable_region_offset].matches('\n').count();

        let editable_region_lines = old_editable_region.lines().count() as u32;
        let diff = language::unified_diff_with_context(
            &old_editable_region,
            &new_editable_region,
            editable_region_start_line as u32,
            editable_region_start_line as u32,
            editable_region_lines,
        );

        let diff = indoc::formatdoc! {"
            --- a/{path}
            +++ b/{path}
            {diff}",
            path = example.spec.cursor_path.to_string_lossy(),
            diff = diff,
        };

        let actual_cursor = cursor_offset.map(|editable_region_cursor_offset| {
            ActualCursor::from_editable_region(
                &example.spec.cursor_path,
                editable_region_cursor_offset,
                &new_editable_region,
                excerpt,
                editable_region_offset,
                editable_region_start_line,
            )
        });

        Ok((diff, actual_cursor))
    }

    fn format_edit_history(edit_history: &str) -> String {
        let lines: Vec<&str> = edit_history.lines().collect();

        if lines.is_empty() {
            return "(No edit history)".to_string();
        }

        if lines.len() > Self::MAX_HISTORY_LINES {
            let truncated = lines[lines.len() - Self::MAX_HISTORY_LINES..].join("\n");
            format!("{truncated}\n[...truncated...]")
        } else {
            lines.join("\n")
        }
    }

    pub fn format_context(example: &Example) -> String {
        let related_files = example
            .prompt_inputs
            .as_ref()
            .and_then(|pi| pi.related_files.as_deref());

        let Some(related_files) = related_files else {
            return "(No context)".to_string();
        };

        if related_files.is_empty() {
            return "(No context)".to_string();
        }

        let prefix = "`````";
        let suffix = "`````\n\n";
        let max_tokens = 1024;
        zeta_prompt::format_related_files_within_budget(related_files, &prefix, &suffix, max_tokens)
    }

    fn format_cursor_excerpt(
        example: &Example,
        editable_range: Range<usize>,
        context_range: Range<usize>,
    ) -> String {
        let mut result = String::new();

        let prompt_inputs = example.prompt_inputs.as_ref().unwrap();
        let excerpt = prompt_inputs.cursor_excerpt.as_ref();
        let cursor_offset = prompt_inputs.cursor_offset_in_excerpt;

        let path_str = example.spec.cursor_path.to_string_lossy();
        result.push_str(&format!("`````{path_str}\n"));
        result.push_str(&excerpt[context_range.start..editable_range.start]);
        result.push_str(Self::EDITABLE_REGION_START);
        result.push_str(&excerpt[editable_range.start..cursor_offset]);
        result.push_str(Self::USER_CURSOR_MARKER);
        result.push_str(&excerpt[cursor_offset..editable_range.end]);
        result.push_str(Self::EDITABLE_REGION_END);
        result.push_str(&excerpt[editable_range.end..context_range.end]);
        result.push_str("\n`````");

        result
    }

    pub fn extract_editable_region(text: &str) -> Result<String> {
        let start = text
            .rfind(Self::EDITABLE_REGION_START)
            .map_or(0, |pos| pos + Self::EDITABLE_REGION_START.len());
        let end = text.rfind(Self::EDITABLE_REGION_END).unwrap_or(text.len());

        if start >= end {
            return Err(anyhow!("Invalid editable region markers"));
        }

        let region = &text[start..end];
        Ok(region.strip_suffix('\n').unwrap_or(region).to_string())
    }

    fn format_diagnostics(example: &Example) -> String {
        let Some(prompt_inputs) = example.prompt_inputs.as_ref() else {
            return "No Diagnostics".to_string();
        };

        let cursor_buffer_row = prompt_inputs.excerpt_start_row.map(|excerpt_start_row| {
            excerpt_start_row
                + prompt_inputs.cursor_excerpt[..prompt_inputs.cursor_offset_in_excerpt]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count() as u32
        });
        let diagnostics = zeta_prompt::format_active_buffer_diagnostics_with_budget(
            &prompt_inputs.active_buffer_diagnostics,
            cursor_buffer_row,
            2_000,
        );

        let diagnostics = diagnostics
            .strip_prefix("<filename>diagnostics\n")
            .unwrap_or(&diagnostics);

        if diagnostics.is_empty() {
            "No Diagnostics".to_string()
        } else {
            diagnostics.to_string()
        }
    }
}
