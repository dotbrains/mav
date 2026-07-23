use super::*;

pub(crate) fn is_list_prefix_row(
    row: MultiBufferRow,
    buffer: &MultiBufferSnapshot,
    language: &LanguageScope,
) -> bool {
    let Some((snapshot, range)) = buffer.buffer_line_for_row(row) else {
        return false;
    };

    let num_of_whitespaces = snapshot
        .chars_for_range(range.clone())
        .take_while(|c| c.is_whitespace())
        .count();

    let task_list_prefixes: Vec<_> = language
        .task_list()
        .into_iter()
        .flat_map(|config| {
            config
                .prefixes
                .iter()
                .map(|p| p.as_ref())
                .collect::<Vec<_>>()
        })
        .collect();
    let unordered_list_markers: Vec<_> = language
        .unordered_list()
        .iter()
        .map(|marker| marker.as_ref())
        .collect();
    let all_prefixes: Vec<_> = task_list_prefixes
        .into_iter()
        .chain(unordered_list_markers)
        .collect();
    if let Some(max_prefix_len) = all_prefixes.iter().map(|p| p.len()).max() {
        let candidate: String = snapshot
            .chars_for_range(range.clone())
            .skip(num_of_whitespaces)
            .take(max_prefix_len)
            .collect();
        if all_prefixes
            .iter()
            .any(|prefix| candidate.starts_with(*prefix))
        {
            return true;
        }
    }

    let ordered_list_candidate: String = snapshot
        .chars_for_range(range)
        .skip(num_of_whitespaces)
        .take(ORDERED_LIST_MAX_MARKER_LEN)
        .collect();
    for ordered_config in language.ordered_list() {
        let regex = match Regex::new(&ordered_config.pattern) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let Some(captures) = regex.captures(&ordered_list_candidate) {
            return captures.get(0).is_some();
        }
    }

    false
}

#[derive(Debug)]
pub(crate) enum NewlineConfig {
    /// Insert newline with optional additional indent and optional extra blank line
    Newline {
        additional_indent: IndentSize,
        extra_line_additional_indent: Option<IndentSize>,
        prevent_auto_indent: bool,
    },
    /// Clear the current line
    ClearCurrentLine,
    /// Unindent the current line and add continuation
    UnindentCurrentLine { continuation: Arc<str> },
}

impl NewlineConfig {
    pub(crate) fn has_extra_line(&self) -> bool {
        matches!(
            self,
            Self::Newline {
                extra_line_additional_indent: Some(_),
                ..
            }
        )
    }

    pub(crate) fn insert_extra_newline_brackets(
        buffer: &MultiBufferSnapshot,
        range: Range<MultiBufferOffset>,
        language: &language::LanguageScope,
    ) -> bool {
        let leading_whitespace_len = buffer
            .reversed_chars_at(range.start)
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .map(|c| c.len_utf8())
            .sum::<usize>();
        let trailing_whitespace_len = buffer
            .chars_at(range.end)
            .take_while(|c| c.is_whitespace() && *c != '\n')
            .map(|c| c.len_utf8())
            .sum::<usize>();
        let range = range.start - leading_whitespace_len..range.end + trailing_whitespace_len;

        language.brackets().any(|(pair, enabled)| {
            let pair_start = pair.start.trim_end();
            let pair_end = pair.end.trim_start();

            enabled
                && pair.newline
                && buffer.contains_str_at(range.end, pair_end)
                && buffer.contains_str_at(
                    range.start.saturating_sub_usize(pair_start.len()),
                    pair_start,
                )
        })
    }

    pub(crate) fn insert_extra_newline_tree_sitter(
        buffer: &MultiBufferSnapshot,
        range: Range<MultiBufferOffset>,
    ) -> bool {
        let (buffer, range) = match buffer
            .range_to_buffer_ranges(range.start..range.end)
            .as_slice()
        {
            [(buffer_snapshot, range, _)] => (*buffer_snapshot, range.clone()),
            _ => return false,
        };
        let pair = {
            let mut result: Option<BracketMatch<usize>> = None;

            for pair in buffer
                .all_bracket_ranges(range.start.0..range.end.0)
                .filter(move |pair| {
                    pair.open_range.start <= range.start.0 && pair.close_range.end >= range.end.0
                })
            {
                let len = pair.close_range.end - pair.open_range.start;

                if let Some(existing) = &result {
                    let existing_len = existing.close_range.end - existing.open_range.start;
                    if len > existing_len {
                        continue;
                    }
                }

                result = Some(pair);
            }

            result
        };
        let Some(pair) = pair else {
            return false;
        };
        pair.newline_only
            && buffer
                .chars_for_range(pair.open_range.end..range.start.0)
                .chain(buffer.chars_for_range(range.end.0..pair.close_range.start))
                .all(|c| c.is_whitespace() && c != '\n')
    }
}

pub(crate) fn comment_delimiter_for_newline(
    start_point: &Point,
    buffer: &MultiBufferSnapshot,
    language: &LanguageScope,
) -> Option<Arc<str>> {
    let delimiters = language.line_comment_prefixes();
    let max_len_of_delimiter = delimiters.iter().map(|delimiter| delimiter.len()).max()?;
    let (snapshot, range) = buffer.buffer_line_for_row(MultiBufferRow(start_point.row))?;

    let num_of_whitespaces = snapshot
        .chars_for_range(range.clone())
        .take_while(|c| c.is_whitespace())
        .count();
    let comment_candidate = snapshot
        .chars_for_range(range.clone())
        .skip(num_of_whitespaces)
        .take(max_len_of_delimiter + 2)
        .collect::<String>();
    let (delimiter, trimmed_len, is_repl) = delimiters
        .iter()
        .filter_map(|delimiter| {
            let prefix = delimiter.trim_end();
            if comment_candidate.starts_with(prefix) {
                let is_repl = if let Some(stripped_comment) = comment_candidate.strip_prefix(prefix)
                {
                    stripped_comment.starts_with(" %%")
                } else {
                    false
                };
                Some((delimiter, prefix.len(), is_repl))
            } else {
                None
            }
        })
        .max_by_key(|(_, len, _)| *len)?;

    if let Some(BlockCommentConfig {
        start: block_start, ..
    }) = language.block_comment()
    {
        let block_start_trimmed = block_start.trim_end();
        if block_start_trimmed.starts_with(delimiter.trim_end()) {
            let line_content = snapshot
                .chars_for_range(range.clone())
                .skip(num_of_whitespaces)
                .take(block_start_trimmed.len())
                .collect::<String>();

            if line_content.starts_with(block_start_trimmed) {
                return None;
            }
        }
    }

    let cursor_is_placed_after_comment_marker =
        num_of_whitespaces + trimmed_len <= start_point.column as usize;
    if cursor_is_placed_after_comment_marker {
        if !is_repl {
            return Some(delimiter.clone());
        }

        let line_content_after_cursor: String = snapshot
            .chars_for_range(range)
            .skip(start_point.column as usize)
            .collect();

        if line_content_after_cursor.trim().is_empty() {
            return None;
        } else {
            return Some(delimiter.clone());
        }
    } else {
        None
    }
}

pub(crate) fn documentation_delimiter_for_newline(
    start_point: &Point,
    buffer: &MultiBufferSnapshot,
    language: &LanguageScope,
    newline_config: &mut NewlineConfig,
) -> Option<Arc<str>> {
    let BlockCommentConfig {
        start: start_tag,
        end: end_tag,
        prefix: delimiter,
        tab_size: len,
    } = language.documentation_comment()?;
    let is_within_block_comment = buffer
        .language_scope_at(*start_point)
        .is_some_and(|scope| scope.override_name() == Some("comment"));
    if !is_within_block_comment {
        return None;
    }

    let (snapshot, range) = buffer.buffer_line_for_row(MultiBufferRow(start_point.row))?;

    let num_of_whitespaces = snapshot
        .chars_for_range(range.clone())
        .take_while(|c| c.is_whitespace())
        .count();

    // It is safe to use a column from MultiBufferPoint in context of a single buffer ranges, because we're only ever looking at a single line at a time.
    let column = start_point.column;
    let cursor_is_after_start_tag = {
        let start_tag_len = start_tag.len();
        let start_tag_line = snapshot
            .chars_for_range(range.clone())
            .skip(num_of_whitespaces)
            .take(start_tag_len)
            .collect::<String>();
        if start_tag_line.starts_with(start_tag.as_ref()) {
            num_of_whitespaces + start_tag_len <= column as usize
        } else {
            false
        }
    };

    let cursor_is_after_delimiter = {
        let delimiter_trim = delimiter.trim_end();
        let delimiter_line = snapshot
            .chars_for_range(range.clone())
            .skip(num_of_whitespaces)
            .take(delimiter_trim.len())
            .collect::<String>();
        if delimiter_line.starts_with(delimiter_trim) {
            num_of_whitespaces + delimiter_trim.len() <= column as usize
        } else {
            false
        }
    };

    let mut needs_extra_line = false;
    let mut extra_line_additional_indent = IndentSize::spaces(0);

    let cursor_is_before_end_tag_if_exists = {
        let mut char_position = 0u32;
        let mut end_tag_offset = None;

        'outer: for chunk in snapshot.text_for_range(range) {
            if let Some(byte_pos) = chunk.find(&**end_tag) {
                let chars_before_match = chunk[..byte_pos].chars().count() as u32;
                end_tag_offset = Some(char_position + chars_before_match);
                break 'outer;
            }
            char_position += chunk.chars().count() as u32;
        }

        if let Some(end_tag_offset) = end_tag_offset {
            let cursor_is_before_end_tag = column <= end_tag_offset;
            if cursor_is_after_start_tag {
                if cursor_is_before_end_tag {
                    needs_extra_line = true;
                }
                let cursor_is_at_start_of_end_tag = column == end_tag_offset;
                if cursor_is_at_start_of_end_tag {
                    extra_line_additional_indent.len = *len;
                }
            }
            cursor_is_before_end_tag
        } else {
            true
        }
    };

    if (cursor_is_after_start_tag || cursor_is_after_delimiter)
        && cursor_is_before_end_tag_if_exists
    {
        let additional_indent = if cursor_is_after_start_tag {
            IndentSize::spaces(*len)
        } else {
            IndentSize::spaces(0)
        };

        *newline_config = NewlineConfig::Newline {
            additional_indent,
            extra_line_additional_indent: if needs_extra_line {
                Some(extra_line_additional_indent)
            } else {
                None
            },
            prevent_auto_indent: true,
        };
        Some(delimiter.clone())
    } else {
        None
    }
}

pub(crate) fn list_delimiter_for_newline(
    start_point: &Point,
    buffer: &MultiBufferSnapshot,
    language: &LanguageScope,
    newline_config: &mut NewlineConfig,
) -> Option<Arc<str>> {
    let (snapshot, range) = buffer.buffer_line_for_row(MultiBufferRow(start_point.row))?;

    let num_of_whitespaces = snapshot
        .chars_for_range(range.clone())
        .take_while(|c| c.is_whitespace())
        .count();

    let task_list_entries: Vec<_> = language
        .task_list()
        .into_iter()
        .flat_map(|config| {
            config
                .prefixes
                .iter()
                .map(|prefix| (prefix.as_ref(), config.continuation.as_ref()))
        })
        .collect();
    let unordered_list_entries: Vec<_> = language
        .unordered_list()
        .iter()
        .map(|marker| (marker.as_ref(), marker.as_ref()))
        .collect();

    let all_entries: Vec<_> = task_list_entries
        .into_iter()
        .chain(unordered_list_entries)
        .collect();

    if let Some(max_prefix_len) = all_entries.iter().map(|(p, _)| p.len()).max() {
        let candidate: String = snapshot
            .chars_for_range(range.clone())
            .skip(num_of_whitespaces)
            .take(max_prefix_len)
            .collect();

        if let Some((prefix, continuation)) = all_entries
            .iter()
            .filter(|(prefix, _)| candidate.starts_with(*prefix))
            .max_by_key(|(prefix, _)| prefix.len())
        {
            let end_of_prefix = num_of_whitespaces + prefix.len();
            let cursor_is_after_prefix = end_of_prefix <= start_point.column as usize;
            let has_content_after_marker = snapshot
                .chars_for_range(range)
                .skip(end_of_prefix)
                .any(|c| !c.is_whitespace());

            if has_content_after_marker && cursor_is_after_prefix {
                return Some((*continuation).into());
            }

            if start_point.column as usize == end_of_prefix {
                if num_of_whitespaces == 0 {
                    *newline_config = NewlineConfig::ClearCurrentLine;
                } else {
                    *newline_config = NewlineConfig::UnindentCurrentLine {
                        continuation: (*continuation).into(),
                    };
                }
            }

            return None;
        }
    }

    let candidate: String = snapshot
        .chars_for_range(range.clone())
        .skip(num_of_whitespaces)
        .take(ORDERED_LIST_MAX_MARKER_LEN)
        .collect();

    for ordered_config in language.ordered_list() {
        let regex = match Regex::new(&ordered_config.pattern) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if let Some(captures) = regex.captures(&candidate) {
            let full_match = captures.get(0)?;
            let marker_len = full_match.len();
            let end_of_prefix = num_of_whitespaces + marker_len;
            let cursor_is_after_prefix = end_of_prefix <= start_point.column as usize;

            let has_content_after_marker = snapshot
                .chars_for_range(range)
                .skip(end_of_prefix)
                .any(|c| !c.is_whitespace());

            if has_content_after_marker && cursor_is_after_prefix {
                let number: u32 = captures.get(1)?.as_str().parse().ok()?;
                let continuation = ordered_config
                    .format
                    .replace("{1}", &(number + 1).to_string());
                return Some(continuation.into());
            }

            if start_point.column as usize == end_of_prefix {
                let continuation = ordered_config.format.replace("{1}", "1");
                if num_of_whitespaces == 0 {
                    *newline_config = NewlineConfig::ClearCurrentLine;
                } else {
                    *newline_config = NewlineConfig::UnindentCurrentLine {
                        continuation: continuation.into(),
                    };
                }
            }

            return None;
        }
    }

    None
}
