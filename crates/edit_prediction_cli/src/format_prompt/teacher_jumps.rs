use super::*;

/// Teacher prompt for long-range edit prediction ("jumps"). All prompt
/// context — the cursor file and every related-file excerpt — is annotated
/// with hashed region markers (V0609HashedRegions), and the teacher may
/// output a sequence of marker-bounded edits targeting any of it.
pub struct TeacherJumpsPrompt;

impl TeacherJumpsPrompt {
    pub(crate) const USER_CURSOR_MARKER: &str = "<|user_cursor|>";
    pub(crate) const NO_EDITS: &str = "NO_EDITS";

    const MAX_HISTORY_TOKENS: usize = 4000;

    pub const DEFAULT_RELATED_FILES_BUDGET: usize = 8192;

    pub fn format_prompt(example: &Example, related_files_budget: usize) -> Result<String> {
        let prompt_inputs = example
            .prompt_inputs
            .as_ref()
            .context("example is missing prompt inputs")?;
        let marker_table = hashed_regions::build_marker_table(prompt_inputs);
        let cursor = hashed_regions::locate_cursor_in_related_files(prompt_inputs).context(
            "cursor position is not covered by any related-file excerpt of the cursor file; \
             teacher-jumps requires current-file context retrieval (e.g. `ep context --type=current-file,...`)",
        )?;

        let edit_history = Self::format_edit_history(&prompt_inputs);
        let context = Self::format_context(
            prompt_inputs,
            &marker_table,
            related_files_budget,
            cursor.file_ix,
        );
        let cursor_excerpt =
            Self::format_cursor_excerpt(example, prompt_inputs, &marker_table, &cursor)?;

        let prompt_template = crate::prompt_assets::get_prompt("teacher_jumps.md");
        let prompt = prompt_template
            .replace("{{context}}", &context)
            .replace("{{edit_history}}", &edit_history)
            .replace("{{cursor_excerpt}}", &cursor_excerpt);

        Ok(prompt)
    }

    pub fn parse(example: &Example, response: &str) -> Result<(String, Option<ActualCursor>)> {
        let no_edits = (String::new(), None);
        if let Some(last_codeblock) = extract_last_codeblock(&response) {
            if last_codeblock.trim() == Self::NO_EDITS {
                return Ok(no_edits);
            }
        }

        if response.trim().ends_with(Self::NO_EDITS) {
            return Ok(no_edits);
        }

        let prompt_inputs = example
            .prompt_inputs
            .as_ref()
            .context("example is missing prompt inputs")?;

        // The teacher emits reasoning plus a sequence of markdown code fences,
        // one per edit, each a marker-bounded span. Extract the spans from the
        // fences, then hand off to the shared hash-region patch assembler that
        // the student parser also uses.
        let codeblocks: Vec<String> = extract_all_codeblocks(response)
            .into_iter()
            .filter(|block| block.contains(hashed_regions::MARKER_TAG_PREFIX))
            .collect();
        if codeblocks.is_empty() {
            return Err(anyhow!(
                "no marker-bounded edit codeblocks found in model response"
            ));
        }

        let mut spans = Vec::with_capacity(codeblocks.len());
        for codeblock in &codeblocks {
            spans.push(hashed_regions::extract_marker_span(codeblock)?);
        }

        let (patch, cursor) = hashed_regions::build_patch_from_spans(
            prompt_inputs,
            &spans,
            Self::USER_CURSOR_MARKER,
        )?;

        let actual_cursor = cursor.map(|cursor| {
            ActualCursor::from_editable_region(
                &cursor.path,
                cursor.cursor_offset_in_new_text,
                &cursor.new_text,
                &cursor.old_text,
                0,
                cursor.start_row as usize,
            )
        });

        Ok((patch, actual_cursor))
    }

    fn format_edit_history(prompt_inputs: &Zeta2PromptInput) -> String {
        format_edit_history_within_budget(
            &prompt_inputs.events,
            "",
            "",
            Self::MAX_HISTORY_TOKENS,
            max_edit_event_count_for_format(&ZetaFormat::V0327SingleFile),
        )
    }

    /// Render related files with hashed region markers, within a token
    /// budget. Mirrors `zeta_prompt::format_related_files_within_budget`,
    /// but inserts marker tags into every included excerpt. The cursor file
    /// is skipped: it renders in its own prompt section via
    /// `format_cursor_excerpt`, and including it here would duplicate it.
    fn format_context(
        prompt_inputs: &Zeta2PromptInput,
        marker_table: &[SnippetMarkers],
        max_tokens: usize,
        cursor_file_ix: usize,
    ) -> String {
        let Some(related_files) = prompt_inputs.related_files.as_deref() else {
            return "(No context)".to_string();
        };
        if related_files.is_empty() {
            return "(No context)".to_string();
        }

        let estimate_tokens = |bytes: usize| bytes / 3;

        struct RenderedExcerpt {
            file_ix: usize,
            excerpt_ix: usize,
            order: usize,
            rendered: String,
        }

        let mut candidates = Vec::new();
        for (file_ix, file) in related_files.iter().enumerate() {
            if file_ix == cursor_file_ix {
                continue;
            }
            for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
                let markers = marker_table.iter().find_map(|snippet| {
                    (snippet.file_ix == file_ix && snippet.excerpt_ix == excerpt_ix)
                        .then_some(&snippet.markers)
                });
                let mut rendered = String::new();
                match markers {
                    Some(markers) => hashed_regions::write_snippet_with_markers(
                        &mut rendered,
                        &excerpt.text,
                        markers,
                        None,
                    ),
                    None => rendered.push_str(&excerpt.text),
                }
                if !rendered.ends_with('\n') {
                    rendered.push('\n');
                }
                candidates.push(RenderedExcerpt {
                    file_ix,
                    excerpt_ix,
                    order: excerpt.order,
                    rendered,
                });
            }
        }

        let file_headers: Vec<String> = related_files
            .iter()
            .map(|file| format!("`````{}\n", file.path.to_string_lossy()))
            .collect();
        let file_suffix = "`````\n\n";

        let mut selection_order: Vec<usize> = (0..candidates.len()).collect();
        selection_order.sort_by_key(|&candidate_ix| {
            let candidate = &candidates[candidate_ix];
            (candidate.order, candidate.file_ix, candidate.excerpt_ix)
        });

        let mut total_tokens = 0;
        let mut included = vec![false; candidates.len()];
        let mut file_included = vec![false; related_files.len()];
        for &candidate_ix in &selection_order {
            let candidate = &candidates[candidate_ix];
            let header_cost = if file_included[candidate.file_ix] {
                0
            } else {
                estimate_tokens(file_headers[candidate.file_ix].len() + file_suffix.len())
            };
            let excerpt_cost = estimate_tokens(candidate.rendered.len());
            if total_tokens + header_cost + excerpt_cost > max_tokens {
                break;
            }
            total_tokens += header_cost + excerpt_cost;
            file_included[candidate.file_ix] = true;
            included[candidate_ix] = true;
        }

        let mut result = String::new();
        let mut last_file_ix = None;
        for (candidate_ix, candidate) in candidates.iter().enumerate() {
            if !included[candidate_ix] {
                continue;
            }
            if last_file_ix != Some(candidate.file_ix) {
                if last_file_ix.is_some() {
                    result.push_str(file_suffix);
                }
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
            if zeta_prompt::rows_omitted_after_excerpt(excerpt, next_excerpt_start, file.max_row) {
                result.push_str("...\n");
            }
        }
        if last_file_ix.is_some() {
            result.push_str(file_suffix);
        }

        if result.is_empty() {
            "(No context)".to_string()
        } else {
            result
        }
    }

    /// Render the current file from its related-file entry, with marker tags
    /// and the user cursor injected. The current file gets its own prompt
    /// section but shares the related-file snippets and markers, so its
    /// content appears in the prompt exactly once.
    fn format_cursor_excerpt(
        example: &Example,
        prompt_inputs: &Zeta2PromptInput,
        marker_table: &[SnippetMarkers],
        cursor: &hashed_regions::RelatedFileCursor,
    ) -> Result<String> {
        let related_files = prompt_inputs
            .related_files
            .as_deref()
            .context("prompt inputs are missing related files")?;
        let file = related_files
            .get(cursor.file_ix)
            .context("cursor file index out of range")?;

        let path_str = example.spec.cursor_path.to_string_lossy();
        let mut result = format!("`````{path_str}\n");
        for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
            let markers = marker_table
                .iter()
                .find_map(|snippet| {
                    (snippet.file_ix == cursor.file_ix && snippet.excerpt_ix == excerpt_ix)
                        .then_some(&snippet.markers)
                })
                .context("marker table is missing a cursor file snippet")?;
            let cursor_in_excerpt = (excerpt_ix == cursor.excerpt_ix)
                .then_some((cursor.offset_in_excerpt, Self::USER_CURSOR_MARKER));
            hashed_regions::write_snippet_with_markers(
                &mut result,
                &excerpt.text,
                markers,
                cursor_in_excerpt,
            );
            if !result.ends_with('\n') {
                result.push('\n');
            }
            if excerpt.row_range.end < file.max_row {
                result.push_str("...\n");
            }
        }
        result.push_str("`````");

        Ok(result)
    }
}
