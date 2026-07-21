use super::*;

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

pub fn format_hash_regions_prompt_with_budget(
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
