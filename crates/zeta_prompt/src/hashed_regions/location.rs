use crate::{ContextSource, RelatedExcerpt, RelatedFile, Zeta2PromptInput};
use anyhow::{Context as _, Result};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use super::SnippetMarkers;

pub struct RelatedFileCursor {
    pub file_ix: usize,
    pub excerpt_ix: usize,
    pub offset_in_excerpt: usize,
}

pub(crate) struct ParseSnippet<'a> {
    pub(crate) file_ix: usize,
    first_excerpt_ix: usize,
    last_excerpt_ix: usize,
    end_row: u32,
    pub(crate) text: Cow<'a, str>,
    pub(crate) markers: Vec<(String, usize)>,
}

pub fn related_file_patch_path(cursor_path: &Path, related_path: &Path) -> PathBuf {
    let stripped: PathBuf = related_path.iter().skip(1).collect();
    if stripped == cursor_path {
        return stripped;
    }

    let cursor_first_component = cursor_path.components().next();
    let related_first_component = related_path.components().next();
    if related_first_component.is_some()
        && cursor_first_component != related_first_component
        && related_path.components().count() > 1
    {
        stripped
    } else {
        related_path.to_path_buf()
    }
}

fn line_start_offset(text: &str, row: usize) -> Option<usize> {
    let mut offset = 0;
    for _ in 0..row {
        offset += text[offset..].find('\n')? + 1;
    }
    Some(offset)
}

pub fn locate_cursor_in_related_files(input: &Zeta2PromptInput) -> Option<RelatedFileCursor> {
    let related_files = input.related_files.as_deref()?;
    let excerpt_start_row = input.excerpt_start_row?;
    let cursor_offset = input.cursor_offset_in_excerpt;
    let excerpt_prefix = input.cursor_excerpt.get(..cursor_offset)?;
    let cursor_row = excerpt_start_row + excerpt_prefix.matches('\n').count() as u32;
    let cursor_column = cursor_offset - excerpt_prefix.rfind('\n').map_or(0, |pos| pos + 1);

    for (file_ix, file) in related_files.iter().enumerate() {
        if related_file_patch_path(&input.cursor_path, &file.path) != input.cursor_path.as_ref() {
            continue;
        }

        for (excerpt_ix, excerpt) in file.excerpts.iter().enumerate() {
            if cursor_row < excerpt.row_range.start || cursor_row > excerpt.row_range.end {
                continue;
            }
            let row_in_excerpt = (cursor_row - excerpt.row_range.start) as usize;
            let line_start = line_start_offset(&excerpt.text, row_in_excerpt)?;
            let line_len = excerpt.text[line_start..]
                .lines()
                .next()
                .unwrap_or("")
                .len();
            if cursor_column <= line_len {
                return Some(RelatedFileCursor {
                    file_ix,
                    excerpt_ix,
                    offset_in_excerpt: line_start + cursor_column,
                });
            }
        }
    }

    None
}

/// Ensure the cursor file is represented by a related-file excerpt that covers
/// the cursor, synthesizing one from `cursor_excerpt` when it isn't.
///
/// All hashed-region context — including the current file — is addressed
/// through `related_files` (see module docs), so a prompt built from a
/// `Zeta2PromptInput` whose `related_files` don't cover the cursor cannot be
/// rendered or parsed. Inputs produced by current-file context retrieval
/// (`ContextSource::CurrentFile`) are already covered and left untouched; this
/// normalizes the rest (e.g. raw settled-data samples, or any caller that
/// didn't run current-file retrieval) from the `cursor_excerpt` the input
/// already carries, so the format is usable without re-running context
/// collection.
///
/// When synthesis is needed, any pre-existing excerpts of the cursor file are
/// **replaced** by the synthesized window: the renderer emits excerpts verbatim
/// without coalescing, so keeping overlapping fragments would duplicate lines
/// with conflicting markers. Other related files are left untouched.
///
/// Returns whether the cursor file is covered after the call (already, or via
/// the synthesized excerpt). Returns `false` only when coverage couldn't be
/// established — e.g. a missing `excerpt_start_row` or an empty
/// `cursor_excerpt` — in which case the input is left unchanged.
pub fn ensure_cursor_file_excerpt(input: &mut Zeta2PromptInput) -> bool {
    if locate_cursor_in_related_files(input).is_some() {
        return true;
    }
    let Some(excerpt_start_row) = input.excerpt_start_row else {
        return false;
    };
    if input.cursor_excerpt.is_empty() {
        return false;
    }

    let cursor_excerpt = input.cursor_excerpt.clone();
    let end_row = excerpt_start_row + cursor_excerpt.matches('\n').count() as u32;
    let synthesized = RelatedExcerpt {
        row_range: excerpt_start_row..end_row,
        text: cursor_excerpt,
        order: 0,
        context_source: ContextSource::CurrentFile,
    };

    let cursor_path = input.cursor_path.clone();
    let in_open_source_repo = input.in_open_source_repo;
    let related_files = input.related_files.get_or_insert_with(Vec::new);
    if let Some(file) = related_files
        .iter_mut()
        .find(|file| related_file_patch_path(&cursor_path, &file.path) == cursor_path.as_ref())
    {
        file.max_row = file.max_row.max(end_row);
        file.excerpts = vec![synthesized];
    } else {
        related_files.insert(
            0,
            RelatedFile {
                path: cursor_path,
                max_row: end_row,
                excerpts: vec![synthesized],
                in_open_source_repo,
            },
        );
    }

    // Confirm the synthesized excerpt actually covers the cursor (guards against
    // a cursor offset that lies outside the excerpt text).
    locate_cursor_in_related_files(input).is_some()
}

pub fn marker_table_for_excerpt(
    marker_table: &[SnippetMarkers],
    file_ix: usize,
    excerpt_ix: usize,
) -> Option<&[(String, usize)]> {
    marker_table.iter().find_map(|snippet| {
        (snippet.file_ix == file_ix && snippet.excerpt_ix == excerpt_ix)
            .then_some(snippet.markers.as_slice())
    })
}

pub(crate) fn merge_contiguous_snippets(
    input: &Zeta2PromptInput,
    marker_table: Vec<SnippetMarkers>,
) -> Result<Vec<ParseSnippet<'_>>> {
    let related_files = input
        .related_files
        .as_deref()
        .context("prompt inputs are missing related files")?;
    let mut snippets: Vec<ParseSnippet> = Vec::new();
    for snippet in marker_table {
        let file = related_files
            .get(snippet.file_ix)
            .context("related file index out of range")?;
        let excerpt = file
            .excerpts
            .get(snippet.excerpt_ix)
            .context("related excerpt index out of range")?;
        if let Some(last) = snippets.last_mut()
            && last.file_ix == snippet.file_ix
            && last.last_excerpt_ix + 1 == snippet.excerpt_ix
            && last.end_row == excerpt.row_range.start
        {
            let text = last.text.to_mut();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            let base = text.len();
            text.push_str(&excerpt.text);
            last.markers.extend(
                snippet
                    .markers
                    .into_iter()
                    .map(|(id, offset)| (id, base + offset)),
            );
            last.last_excerpt_ix = snippet.excerpt_ix;
            last.end_row = excerpt.row_range.end;
        } else {
            snippets.push(ParseSnippet {
                file_ix: snippet.file_ix,
                first_excerpt_ix: snippet.excerpt_ix,
                last_excerpt_ix: snippet.excerpt_ix,
                end_row: excerpt.row_range.end,
                text: Cow::Borrowed(excerpt.text.as_ref()),
                markers: snippet.markers,
            });
        }
    }
    Ok(snippets)
}

pub(crate) fn snippet_path_and_start_row(
    input: &Zeta2PromptInput,
    snippet: &ParseSnippet<'_>,
) -> Result<(PathBuf, u32)> {
    let related_files = input
        .related_files
        .as_deref()
        .context("prompt inputs are missing related files")?;
    let file = related_files
        .get(snippet.file_ix)
        .context("related file index out of range")?;
    let excerpt = file
        .excerpts
        .get(snippet.first_excerpt_ix)
        .context("related excerpt index out of range")?;
    Ok((
        related_file_patch_path(&input.cursor_path, &file.path),
        excerpt.row_range.start,
    ))
}
