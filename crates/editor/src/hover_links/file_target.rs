use super::*;
use project::{Project, ResolvedPath};
use regex::Regex;
use std::sync::LazyLock;
use util::paths::PathWithPosition;

#[derive(Debug, Clone)]
pub struct ResolvedFileTarget {
    pub resolved_path: ResolvedPath,
    pub row: Option<u32>,
    pub column: Option<u32>,
}

impl ResolvedFileTarget {
    /// After opening a file, navigate the editor to the row/column position if present.
    pub fn navigate_item_to_position(
        &self,
        item: Box<dyn crate::ItemHandle>,
        cx: &mut AsyncWindowContext,
    ) {
        if let Some(row) = self.row {
            let col = self.column.unwrap_or(0);
            if let Some(active_editor) = item.downcast::<crate::Editor>() {
                active_editor
                    .downgrade()
                    .update_in(cx, |editor, window, cx| {
                        let row = row.saturating_sub(1);
                        let col = col.saturating_sub(1);
                        let Some(buffer) = editor.buffer().read(cx).as_singleton() else {
                            return;
                        };
                        let point = buffer
                            .read(cx)
                            .snapshot()
                            .point_from_external_input(row, col);
                        editor.go_to_singleton_buffer_point_silently(point, window, cx);
                    })
                    .log_err();
            }
        }
    }
}

pub(crate) async fn find_file(
    buffer: &Entity<language::Buffer>,
    project: Option<Entity<Project>>,
    position: text::Anchor,
    cx: &mut AsyncWindowContext,
) -> Option<(Range<text::Anchor>, ResolvedFileTarget)> {
    let project = project?;
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let scope = snapshot.language_scope_at(position);
    let (range, candidate_file_path) = surrounding_filename(&snapshot, position)?;
    let candidate_len = candidate_file_path.len();

    async fn check_path(
        candidate_file_path: &str,
        project: &Entity<Project>,
        buffer: &Entity<language::Buffer>,
        cx: &mut AsyncWindowContext,
    ) -> Option<ResolvedPath> {
        project
            .update(cx, |project, cx| {
                project.resolve_path_in_buffer(candidate_file_path, buffer, cx)
            })
            .await
            .filter(|s| s.is_file())
    }

    let pattern_candidates = link_pattern_file_candidates(&candidate_file_path);

    // Compute the highlight range for a pattern_range within the candidate string.
    let make_range = |pattern_range: &Range<usize>| -> Range<text::Anchor> {
        let offset_range = range.to_offset(&snapshot);
        let actual_start = offset_range.start + pattern_range.start;
        let actual_end = offset_range.end - (candidate_len - pattern_range.end);
        snapshot.anchor_before(actual_start)..snapshot.anchor_after(actual_end)
    };

    // For each candidate extracted by link_pattern_file_candidates, try resolving in order:
    // 1. The raw candidate string
    // 2. The path portion after stripping `:row:col` suffix
    // 3. With language-specific file extensions appended to raw candidate
    // 4. With language-specific file extensions appended to stripped path
    for (pattern_candidate, pattern_range) in &pattern_candidates {
        // Try the raw candidate first.
        if let Some(existing_path) = check_path(&pattern_candidate, &project, buffer, cx).await {
            return Some((
                make_range(pattern_range),
                ResolvedFileTarget {
                    resolved_path: existing_path,
                    row: None,
                    column: None,
                },
            ));
        }

        // Parse row:col suffix once per candidate for use in fallback attempts.
        // This handles patterns like `file.rs:83:1`, `file.rs:83`, and `file.rs:20:in`.
        let parsed = PathWithPosition::parse_str(pattern_candidate);
        let parsed_path = parsed.path.to_string_lossy();

        // Try resolving just the path portion (without :row:col).
        if parsed.row.is_some() {
            if let Some(existing_path) = check_path(&parsed_path, &project, buffer, cx).await {
                return Some((
                    make_range(pattern_range),
                    ResolvedFileTarget {
                        resolved_path: existing_path,
                        row: parsed.row,
                        column: parsed.column,
                    },
                ));
            }
        }

        // Try with language-specific suffixes.
        if let Some(scope) = &scope {
            for suffix in scope.path_suffixes() {
                if pattern_candidate.ends_with(format!(".{suffix}").as_str()) {
                    continue;
                }

                let suffixed_candidate = format!("{pattern_candidate}.{suffix}");
                if let Some(existing_path) =
                    check_path(&suffixed_candidate, &project, buffer, cx).await
                {
                    return Some((
                        make_range(pattern_range),
                        ResolvedFileTarget {
                            resolved_path: existing_path,
                            row: None,
                            column: None,
                        },
                    ));
                }
            }

            // Try with language-specific suffixes on the stripped path.
            if parsed.row.is_some() {
                for suffix in scope.path_suffixes() {
                    if parsed_path.ends_with(&format!(".{suffix}")) {
                        continue;
                    }

                    let suffixed_candidate = format!("{parsed_path}.{suffix}");
                    if let Some(existing_path) =
                        check_path(&suffixed_candidate, &project, buffer, cx).await
                    {
                        return Some((
                            make_range(pattern_range),
                            ResolvedFileTarget {
                                resolved_path: existing_path,
                                row: parsed.row,
                                column: parsed.column,
                            },
                        ));
                    }
                }
            }
        }
    }
    None
}

// Generates candidate file paths by stripping common punctuation wrappers.
// Handles markdown patterns like [title](path), `path`, (path), as well as
// partial wrappers where punctuation only appears on one side (e.g. path) or path`).
// Returns candidates ordered from most-specific (most trimmed) to least-specific (raw).
fn link_pattern_file_candidates(candidate: &str) -> Vec<(String, Range<usize>)> {
    static MD_LINK_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"]\(([^)]*)\)").expect("Failed to create REGEX"));

    // Punctuation that commonly wraps file paths in prose/markdown
    const LEADING_PUNCTUATION: &[char] = &['`', '(', '[', '{', '<', '"', '\''];
    const TRAILING_PUNCTUATION: &[char] = &[
        '`', ')', ']', '}', '>', '"', '\'', '.', ',', ':', ';', '!', '?',
    ];

    let candidate_len = candidate.len();
    let mut candidates = Vec::new();

    // Trim leading and trailing punctuation iteratively
    let mut start = 0;
    let mut end = candidate_len;

    // Trim leading punctuation
    for ch in candidate.chars() {
        if LEADING_PUNCTUATION.contains(&ch) {
            start += ch.len_utf8();
        } else {
            break;
        }
    }

    // Trim trailing punctuation
    for ch in candidate.chars().rev() {
        if TRAILING_PUNCTUATION.contains(&ch) {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }

    // Add trimmed candidate first (highest priority) if it differs from original
    if start < end && (start > 0 || end < candidate_len) {
        candidates.push((candidate[start..end].to_string(), start..end));
    }

    // Extract markdown link destination: [title](path) or ](path) -> path
    // This also handles bare (path) wrapping.
    if let Some(captures) = MD_LINK_REGEX.captures(candidate) {
        if let Some(link) = captures.get(1) {
            let link_str = link.as_str().to_string();
            let link_range = link.range();
            // Avoid duplicate if punctuation trimming already found this
            if !candidates.iter().any(|(s, _)| s == &link_str) {
                candidates.push((link_str, link_range));
            }
        }
    }

    // Always include the raw candidate as fallback (lowest priority)
    candidates.push((candidate.to_string(), 0..candidate_len));

    candidates
}

fn surrounding_filename(
    snapshot: &language::BufferSnapshot,
    position: text::Anchor,
) -> Option<(Range<text::Anchor>, String)> {
    const LIMIT: usize = 2048;

    let offset = position.to_offset(&snapshot);
    let mut token_start = offset;
    let mut token_end = offset;
    let mut found_start = false;
    let mut found_end = false;
    let mut inside_quotes = false;

    let mut filename = String::new();

    let mut backwards = snapshot.reversed_chars_at(offset).take(LIMIT).peekable();
    while let Some(ch) = backwards.next() {
        // Escaped whitespace
        if ch.is_whitespace() && backwards.peek() == Some(&'\\') {
            filename.push(ch);
            token_start -= ch.len_utf8();
            backwards.next();
            token_start -= '\\'.len_utf8();
            continue;
        }
        if ch.is_whitespace() {
            found_start = true;
            break;
        }
        // Quote characters open a quoted region that is stripped from the
        // returned filename. Backticks and parens are NOT treated this way —
        // they are kept as part of the token so that downstream candidate
        // generation (link_pattern_file_candidates) can trim them and produce
        // a tight highlight range via make_range.
        if (ch == '"' || ch == '\'') && !inside_quotes {
            found_start = true;
            inside_quotes = true;
            break;
        }

        filename.push(ch);
        token_start -= ch.len_utf8();
    }
    if !found_start && token_start != 0 {
        return None;
    }

    filename = filename.chars().rev().collect();

    let mut forwards = snapshot
        .chars_at(offset)
        .take(LIMIT - (offset - token_start))
        .peekable();
    while let Some(ch) = forwards.next() {
        // Skip escaped whitespace
        if ch == '\\' && forwards.peek().is_some_and(|ch| ch.is_whitespace()) {
            token_end += ch.len_utf8();
            let whitespace = forwards.next().unwrap();
            token_end += whitespace.len_utf8();
            filename.push(whitespace);
            continue;
        }

        if ch.is_whitespace() {
            found_end = true;
            break;
        }
        if ch == '"' || ch == '\'' {
            // If we're inside quotes, we stop when we come across the next quote
            if inside_quotes {
                found_end = true;
                break;
            } else {
                // Otherwise, we skip the quote
                inside_quotes = true;
                token_end += ch.len_utf8();
                continue;
            }
        }
        filename.push(ch);
        token_end += ch.len_utf8();
    }

    if !found_end && (token_end - token_start >= LIMIT) {
        return None;
    }

    if filename.is_empty() {
        return None;
    }

    let range = snapshot.anchor_before(token_start)..snapshot.anchor_after(token_end);

    Some((range, filename))
}
