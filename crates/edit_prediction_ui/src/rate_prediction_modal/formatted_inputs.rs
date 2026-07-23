use super::*;

impl RatePredictionsModal {
    pub(super) fn write_formatted_inputs(
        formatted_inputs: &mut String,
        inputs: &EditPredictionInputs,
    ) {
        match inputs {
            EditPredictionInputs::V2(inputs) => {
                Self::write_events(formatted_inputs, &inputs.events);
                Self::write_related_files(
                    formatted_inputs,
                    inputs.related_files.as_deref().unwrap_or_default(),
                );
                Self::write_cursor_excerpt(
                    formatted_inputs,
                    inputs.cursor_path.as_ref(),
                    inputs.cursor_excerpt.as_ref(),
                    inputs.cursor_offset_in_excerpt,
                );
            }
            EditPredictionInputs::V3(inputs) => {
                Self::write_events(formatted_inputs, &inputs.events);
                Self::write_related_files(formatted_inputs, &inputs.editable_context);
                Self::write_zeta3_cursor_excerpt(formatted_inputs, inputs);
            }
        }
    }

    fn write_events(formatted_inputs: &mut String, events: &[Arc<zeta_prompt::Event>]) {
        write!(formatted_inputs, "## Events\n\n").unwrap();

        for event in events {
            formatted_inputs.push_str("```diff\n");
            zeta_prompt::write_event(formatted_inputs, event.as_ref());
            formatted_inputs.push_str("```\n\n");
        }
    }

    fn write_related_files(formatted_inputs: &mut String, included_files: &[RelatedFile]) {
        write!(formatted_inputs, "## Related files\n\n").unwrap();

        for included_file in included_files {
            write!(formatted_inputs, "### {}\n\n", included_file.path.display()).unwrap();

            for excerpt in included_file.excerpts.iter() {
                write!(
                    formatted_inputs,
                    "```{}\n{}\n```\n",
                    included_file.path.display(),
                    excerpt.text
                )
                .unwrap();
            }
        }
    }

    fn write_zeta3_cursor_excerpt(formatted_inputs: &mut String, inputs: &Zeta3PromptInput) {
        let current_excerpt = inputs
            .editable_context
            .iter()
            .filter(|file| file.path == inputs.cursor_path)
            .flat_map(|file| file.excerpts.iter())
            .find_map(|excerpt| {
                if excerpt.context_source != ContextSource::CurrentFile {
                    return None;
                }

                Some((
                    excerpt,
                    Self::offset_for_position_in_excerpt(excerpt, inputs.cursor_position)?,
                ))
            });

        if let Some((excerpt, cursor_offset)) = current_excerpt {
            Self::write_cursor_excerpt(
                formatted_inputs,
                inputs.cursor_path.as_ref(),
                excerpt.text.as_ref(),
                cursor_offset,
            );
        } else {
            write!(formatted_inputs, "## Cursor Excerpt\n\n").unwrap();
            writeln!(
                formatted_inputs,
                "No current-file excerpt found for `{}` at row {}, column {}.",
                inputs.cursor_path.display(),
                inputs.cursor_position.row,
                inputs.cursor_position.column
            )
            .unwrap();
        }
    }

    fn write_cursor_excerpt(
        formatted_inputs: &mut String,
        cursor_path: &Path,
        cursor_excerpt: &str,
        cursor_offset: usize,
    ) {
        write!(formatted_inputs, "## Cursor Excerpt\n\n").unwrap();

        let mut cursor_offset = cursor_offset.min(cursor_excerpt.len());
        while !cursor_excerpt.is_char_boundary(cursor_offset) {
            cursor_offset = cursor_offset.saturating_sub(1);
        }
        writeln!(
            formatted_inputs,
            "```{}\n{}<CURSOR>{}\n```\n",
            cursor_path.display(),
            &cursor_excerpt[..cursor_offset],
            &cursor_excerpt[cursor_offset..],
        )
        .unwrap();
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
}
