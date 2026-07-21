use super::*;

pub fn patch_to_edit_commands(
    old_text: &str,
    patch: &str,
    cursor_offset: Option<usize>,
) -> Result<String> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let old_hashes: Vec<u8> = old_lines
        .iter()
        .map(|line| hash_line(line.as_bytes()))
        .collect();

    let mut result = String::new();
    let mut first_hunk = true;

    struct Hunk<'a> {
        line_range: Range<usize>,
        new_text_lines: Vec<&'a str>,
        cursor_line_offset_in_new_text: Option<(usize, usize)>,
    }

    // Parse the patch line by line. We only care about hunk headers,
    // context, deletions, and additions.
    let mut old_line_index: usize = 0;
    let mut current_hunk: Option<Hunk> = None;
    // Byte offset tracking within the hunk's new text for cursor placement.
    let mut new_text_byte_offset: usize = 0;
    // The line index of the last old line seen before/in the current hunk
    // (used for insert-after reference).
    let mut last_old_line_before_hunk: Option<usize> = None;

    fn flush_hunk(
        hunk: Hunk,
        last_old_line: Option<usize>,
        result: &mut String,
        old_hashes: &[u8],
    ) {
        if hunk.line_range.is_empty() {
            // Pure insertion — reference the old line to insert after when in bounds.
            if let Some(after) = last_old_line
                && let Some(&hash) = old_hashes.get(after)
            {
                write!(
                    result,
                    "{INSERT_COMMAND_MARKER}{}\n",
                    LineRef { index: after, hash }
                )
                .unwrap();
            } else {
                result.push_str(INSERT_COMMAND_MARKER);
                result.push('\n');
            }
        } else {
            let start = hunk.line_range.start;
            let end_exclusive = hunk.line_range.end;
            let deleted_line_count = end_exclusive.saturating_sub(start);

            if deleted_line_count == 1 {
                if let Some(&hash) = old_hashes.get(start) {
                    write!(
                        result,
                        "{SET_COMMAND_MARKER}{}\n",
                        LineRef { index: start, hash }
                    )
                    .unwrap();
                } else {
                    result.push_str(SET_COMMAND_MARKER);
                    result.push('\n');
                }
            } else {
                let end_inclusive = end_exclusive - 1;
                match (
                    old_hashes.get(start).copied(),
                    old_hashes.get(end_inclusive).copied(),
                ) {
                    (Some(start_hash), Some(end_hash)) => {
                        write!(
                            result,
                            "{SET_COMMAND_MARKER}{}-{}\n",
                            LineRef {
                                index: start,
                                hash: start_hash
                            },
                            LineRef {
                                index: end_inclusive,
                                hash: end_hash
                            }
                        )
                        .unwrap();
                    }
                    _ => {
                        result.push_str(SET_COMMAND_MARKER);
                        result.push('\n');
                    }
                }
            }
        }
        for (line_offset, line) in hunk.new_text_lines.iter().enumerate() {
            if let Some((cursor_line_offset, char_offset)) = hunk.cursor_line_offset_in_new_text
                && line_offset == cursor_line_offset
            {
                result.push_str(&line[..char_offset]);
                result.push_str(CURSOR_MARKER);
                result.push_str(&line[char_offset..]);
                continue;
            }

            result.push_str(line);
        }
    }

    for raw_line in patch.split_inclusive('\n') {
        if raw_line.starts_with("@@") {
            // Flush any pending change hunk from a previous patch hunk.
            if let Some(hunk) = current_hunk.take() {
                flush_hunk(hunk, last_old_line_before_hunk, &mut result, &old_hashes);
            }

            // Parse hunk header: @@ -old_start[,old_count] +new_start[,new_count] @@
            // We intentionally do not trust old_start as a direct local index into `old_text`,
            // because some patches are produced against a larger file region and carry
            // non-local line numbers. We keep indexing local by advancing from parsed patch lines.
            if first_hunk {
                new_text_byte_offset = 0;
                first_hunk = false;
            }
            continue;
        }

        if raw_line.starts_with("---") || raw_line.starts_with("+++") {
            continue;
        }
        if raw_line.starts_with("\\ No newline") {
            continue;
        }

        if raw_line.starts_with('-') {
            // Extend or start a change hunk with this deleted old line.
            match &mut current_hunk {
                Some(Hunk {
                    line_range: range, ..
                }) => range.end = old_line_index + 1,
                None => {
                    current_hunk = Some(Hunk {
                        line_range: old_line_index..old_line_index + 1,
                        new_text_lines: Vec::new(),
                        cursor_line_offset_in_new_text: None,
                    });
                }
            }
            old_line_index += 1;
        } else if let Some(added_content) = raw_line.strip_prefix('+') {
            // Place cursor marker if cursor_offset falls within this line.
            let mut cursor_line_offset = None;
            if let Some(cursor_off) = cursor_offset
                && (first_hunk
                    || cursor_off >= new_text_byte_offset
                        && cursor_off <= new_text_byte_offset + added_content.len())
            {
                let line_offset = added_content.floor_char_boundary(
                    cursor_off
                        .saturating_sub(new_text_byte_offset)
                        .min(added_content.len()),
                );
                cursor_line_offset = Some(line_offset);
            }

            new_text_byte_offset += added_content.len();

            let hunk = current_hunk.get_or_insert(Hunk {
                line_range: old_line_index..old_line_index,
                new_text_lines: vec![],
                cursor_line_offset_in_new_text: None,
            });
            hunk.new_text_lines.push(added_content);
            hunk.cursor_line_offset_in_new_text = cursor_line_offset
                .map(|offset_in_line| (hunk.new_text_lines.len() - 1, offset_in_line));
        } else {
            // Context line (starts with ' ' or is empty).
            if let Some(hunk) = current_hunk.take() {
                flush_hunk(hunk, last_old_line_before_hunk, &mut result, &old_hashes);
            }
            last_old_line_before_hunk = Some(old_line_index);
            old_line_index += 1;
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            new_text_byte_offset += content.len();
        }
    }

    // Flush final group.
    if let Some(hunk) = current_hunk.take() {
        flush_hunk(hunk, last_old_line_before_hunk, &mut result, &old_hashes);
    }

    // Trim a single trailing newline.
    if result.ends_with('\n') {
        result.pop();
    }

    if result.is_empty() {
        return Ok(NO_EDITS_COMMAND_MARKER.to_string());
    }

    Ok(result)
}
