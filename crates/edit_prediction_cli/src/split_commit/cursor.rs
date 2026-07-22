use super::*;

fn locate_end_of_last_edit(patch: &Patch) -> Option<CursorPosition> {
    let loc = locate_edited_line(patch, -1)?;

    let (line, column, line_length) = match &loc.patch_line {
        PatchLine::Addition(content) => (loc.target_line_number, content.len(), content.len()),
        PatchLine::Deletion(_) => (loc.target_line_number, 1, 1),
        _ => return None,
    };

    Some(CursorPosition {
        file: loc.filename,
        line,
        column,
        line_length,
    })
}

/// Locate the beginning of the first edit in a patch.
fn locate_beginning_of_first_edit(patch: &Patch) -> Option<CursorPosition> {
    let loc = locate_edited_line(patch, 0)?;

    let hunk = patch.hunks.get(loc.hunk_index)?;
    let line_length = if loc.line_index_within_hunk > 0 {
        if let Some(prev_line) = hunk.lines.get(loc.line_index_within_hunk - 1) {
            let content = match prev_line {
                PatchLine::Context(s) | PatchLine::Addition(s) | PatchLine::Deletion(s) => s,
                _ => return None,
            };
            content.len().max(1) - 1
        } else {
            0
        }
    } else {
        0
    };

    let line = loc.target_line_number.saturating_sub(1).max(1);
    let column = line_length.saturating_sub(1);

    Some(CursorPosition {
        file: loc.filename,
        line,
        column,
        line_length,
    })
}

/// Sample cursor position according to the following rules:
/// 1. 80% chance of cursor being at the end of the source patch
/// 2. 20% chance of cursor being at the beginning of the target patch
/// 3. 20% chance of adding a jitter offset
pub fn sample_cursor_position(
    split_commit: &SplitCommit,
    rng: &mut dyn rand::RngCore,
) -> Option<CursorPosition> {
    // End of history
    let src_patch = Patch::parse_unified_diff(&split_commit.source_patch);
    let src_cursor = locate_end_of_last_edit(&src_patch);

    // Beginning of target
    let tgt_patch = Patch::parse_unified_diff(&split_commit.target_patch);
    let tgt_cursor = locate_beginning_of_first_edit(&tgt_patch);

    // Randomly pick a cursor position
    let prefer_source = rng.random_bool(0.8);
    let mut cursor = if prefer_source {
        src_cursor.or(tgt_cursor)
    } else {
        tgt_cursor.or(src_cursor)
    };

    // Possible add jitter
    let should_jitter = rng.random_bool(0.2);
    if should_jitter {
        if let Some(cursor) = cursor.as_mut() {
            let col_offset = rng.random_range(1..=5);
            if rng.random_bool(0.5) {
                cursor.column = cursor
                    .column
                    .saturating_add(col_offset)
                    .min(cursor.line_length);
            } else {
                cursor.column = cursor.column.saturating_sub(col_offset);
            }
        }
    }

    cursor
}

/// Get cursor excerpt from the patches.
///
/// This extracts the lines around the cursor position with a cursor marker.
pub fn get_cursor_excerpt(
    cursor: &CursorPosition,
    source_patch: &str,
    target_patch: &str,
) -> Option<String> {
    let mut excerpt_lines: Vec<String> = Vec::new();
    let mut excerpt_first_line: usize = 0;

    // Search in the last hunk of source patch
    let src = Patch::parse_unified_diff(source_patch);
    if let Some(loc) = locate_edited_line(&src, -1) {
        if loc.filename == cursor.file && loc.target_line_number == cursor.line {
            if let Some(hunk) = src.hunks.get(loc.hunk_index) {
                excerpt_first_line = hunk.new_start as usize;
                for line in &hunk.lines {
                    match line {
                        PatchLine::Addition(s) | PatchLine::Context(s) => {
                            excerpt_lines.push(s.clone());
                        }
                        _ => {}
                    }
                }
                // If hunk only has deletions (file deletion), include deletion lines
                if excerpt_lines.is_empty() {
                    excerpt_first_line = hunk.old_start as usize;
                    for line in &hunk.lines {
                        match line {
                            PatchLine::Deletion(s) => {
                                excerpt_lines.push(s.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Search in target patch if not found
    if excerpt_lines.is_empty() {
        let tgt = Patch::parse_unified_diff(target_patch);
        // Search all hunks for the cursor file, not just the first edit's hunk
        for hunk in &tgt.hunks {
            if hunk.filename == cursor.file {
                excerpt_first_line = hunk.new_start as usize;
                // First try to collect deletions and context (what exists before edits)
                for line in &hunk.lines {
                    match line {
                        PatchLine::Deletion(s) | PatchLine::Context(s) => {
                            excerpt_lines.push(s.clone());
                        }
                        _ => {}
                    }
                }
                // If hunk only has additions (no deletions/context), include all lines
                // This handles cases like adding to an empty file or section
                if excerpt_lines.is_empty() {
                    for line in &hunk.lines {
                        match line {
                            PatchLine::Addition(s)
                            | PatchLine::Deletion(s)
                            | PatchLine::Context(s) => {
                                excerpt_lines.push(s.clone());
                            }
                            _ => {}
                        }
                    }
                }
                if !excerpt_lines.is_empty() {
                    break;
                }
            }
        }
    }

    // Also search source patch hunks if still not found (for fallback cursor case)
    if excerpt_lines.is_empty() {
        for hunk in &src.hunks {
            if hunk.filename == cursor.file {
                excerpt_first_line = hunk.new_start as usize;
                for line in &hunk.lines {
                    match line {
                        PatchLine::Addition(s) | PatchLine::Context(s) => {
                            excerpt_lines.push(s.clone());
                        }
                        _ => {}
                    }
                }
                // If hunk only has deletions, include deletion lines
                if excerpt_lines.is_empty() {
                    excerpt_first_line = hunk.old_start as usize;
                    for line in &hunk.lines {
                        match line {
                            PatchLine::Deletion(s) => {
                                excerpt_lines.push(s.clone());
                            }
                            _ => {}
                        }
                    }
                }
                if !excerpt_lines.is_empty() {
                    break;
                }
            }
        }
    }

    if excerpt_lines.is_empty() {
        return None;
    }

    // Add cursor marker
    for (i, line) in excerpt_lines.iter_mut().enumerate() {
        let line_num = excerpt_first_line + i;
        if line_num == cursor.line {
            let col = cursor.column.min(line.len());
            // Ensure we split at a valid UTF-8 character boundary
            let col = if line.is_char_boundary(col) {
                col
            } else {
                // Find the nearest valid character boundary
                (0..=col)
                    .rev()
                    .find(|&i| line.is_char_boundary(i))
                    .unwrap_or(0)
            };
            let (before, after) = line.split_at(col);
            *line = format!("{}<|user_cursor|>{}", before, after);
            break;
        }
    }

    Some(excerpt_lines.join("\n"))
}
