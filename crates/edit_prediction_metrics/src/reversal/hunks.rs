use zeta_prompt::udiff::apply_diff_to_string;

pub(super) fn apply_diff_to_string_lenient(diff_str: &str, text: &str) -> String {
    let hunks = parse_diff_hunks(diff_str);
    let mut result = text.to_string();

    for hunk in hunks {
        let hunk_diff = format!("--- a/file\n+++ b/file\n{}", format_hunk(&hunk));
        if let Ok(updated) = apply_diff_to_string(&hunk_diff, &result) {
            result = updated;
        }
    }

    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedHunk {
    pub(super) old_start: u32,
    pub(super) old_count: u32,
    pub(super) new_start: u32,
    pub(super) new_count: u32,
    pub(super) lines: Vec<HunkLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HunkLine {
    Context(String),
    Addition(String),
    Deletion(String),
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let line = line.strip_prefix("@@ -")?;
    let (old_part, rest) = line.split_once(' ')?;
    let rest = rest.strip_prefix('+')?;
    let (new_part, _) = rest.split_once(" @@")?;

    let (old_start, old_count) = if let Some((start, count)) = old_part.split_once(',') {
        (start.parse().ok()?, count.parse().ok()?)
    } else {
        (old_part.parse().ok()?, 1)
    };

    let (new_start, new_count) = if let Some((start, count)) = new_part.split_once(',') {
        (start.parse().ok()?, count.parse().ok()?)
    } else {
        (new_part.parse().ok()?, 1)
    };

    Some((old_start, old_count, new_start, new_count))
}

pub(super) fn parse_diff_hunks(diff: &str) -> Vec<ParsedHunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<ParsedHunk> = None;

    for line in diff.lines() {
        if let Some((old_start, old_count, new_start, new_count)) = parse_hunk_header(line) {
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }
            current_hunk = Some(ParsedHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(stripped) = line.strip_prefix('+') {
                hunk.lines.push(HunkLine::Addition(stripped.to_string()));
            } else if let Some(stripped) = line.strip_prefix('-') {
                hunk.lines.push(HunkLine::Deletion(stripped.to_string()));
            } else if let Some(stripped) = line.strip_prefix(' ') {
                hunk.lines.push(HunkLine::Context(stripped.to_string()));
            } else if line.is_empty() {
                hunk.lines.push(HunkLine::Context(String::new()));
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

fn format_hunk(hunk: &ParsedHunk) -> String {
    let mut result = format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
    );
    for line in &hunk.lines {
        match line {
            HunkLine::Context(text) => {
                result.push(' ');
                result.push_str(text);
                result.push('\n');
            }
            HunkLine::Addition(text) => {
                result.push('+');
                result.push_str(text);
                result.push('\n');
            }
            HunkLine::Deletion(text) => {
                result.push('-');
                result.push_str(text);
                result.push('\n');
            }
        }
    }
    result
}

pub(super) fn filter_diff_hunks_by_excerpt(
    diff: &str,
    excerpt_start_row: u32,
    excerpt_row_count: u32,
) -> (String, i32) {
    let hunks = parse_diff_hunks(diff);
    let excerpt_start_0based = excerpt_start_row;
    let excerpt_end_0based = excerpt_start_row + excerpt_row_count;

    let mut filtered_hunks = Vec::new();
    let mut cumulative_line_offset: i32 = 0;

    for hunk in hunks {
        let hunk_start_0based = hunk.new_start.saturating_sub(1);
        let hunk_end_0based = hunk_start_0based + hunk.new_count;

        let additions: i32 = hunk
            .lines
            .iter()
            .filter(|l| matches!(l, HunkLine::Addition(_)))
            .count() as i32;
        let deletions: i32 = hunk
            .lines
            .iter()
            .filter(|l| matches!(l, HunkLine::Deletion(_)))
            .count() as i32;
        let hunk_line_delta = additions - deletions;

        if hunk_end_0based <= excerpt_start_0based {
            cumulative_line_offset += hunk_line_delta;
            continue;
        }

        if hunk_start_0based >= excerpt_end_0based {
            continue;
        }

        let mut filtered_lines = Vec::new();
        let mut current_row_0based = hunk_start_0based;
        let mut filtered_old_count = 0u32;
        let mut filtered_new_count = 0u32;
        let mut first_included_row: Option<u32> = None;

        for line in &hunk.lines {
            match line {
                HunkLine::Context(text) => {
                    if current_row_0based >= excerpt_start_0based
                        && current_row_0based < excerpt_end_0based
                    {
                        if first_included_row.is_none() {
                            first_included_row = Some(current_row_0based);
                        }
                        filtered_lines.push(HunkLine::Context(text.clone()));
                        filtered_old_count += 1;
                        filtered_new_count += 1;
                    }
                    current_row_0based += 1;
                }
                HunkLine::Addition(text) => {
                    if current_row_0based >= excerpt_start_0based
                        && current_row_0based < excerpt_end_0based
                    {
                        if first_included_row.is_none() {
                            first_included_row = Some(current_row_0based);
                        }
                        filtered_lines.push(HunkLine::Addition(text.clone()));
                        filtered_new_count += 1;
                    }
                    current_row_0based += 1;
                }
                HunkLine::Deletion(text) => {
                    if current_row_0based >= excerpt_start_0based
                        && current_row_0based < excerpt_end_0based
                    {
                        if first_included_row.is_none() {
                            first_included_row = Some(current_row_0based);
                        }
                        filtered_lines.push(HunkLine::Deletion(text.clone()));
                        filtered_old_count += 1;
                    }
                }
            }
        }

        if !filtered_lines.is_empty() {
            let first_row = first_included_row.unwrap_or(excerpt_start_0based);
            let new_start_1based = (first_row - excerpt_start_0based) + 1;

            filtered_hunks.push(ParsedHunk {
                old_start: new_start_1based,
                old_count: filtered_old_count,
                new_start: new_start_1based,
                new_count: filtered_new_count,
                lines: filtered_lines,
            });
        }

        cumulative_line_offset += hunk_line_delta;
    }

    let mut result = String::new();
    for hunk in &filtered_hunks {
        result.push_str(&format_hunk(hunk));
    }

    (result, cumulative_line_offset)
}

pub(super) fn reverse_diff(diff: &str) -> String {
    let mut result: String = diff
        .lines()
        .map(|line| {
            if line.starts_with("--- ") {
                line.replacen("--- ", "+++ ", 1)
            } else if line.starts_with("+++ ") {
                line.replacen("+++ ", "--- ", 1)
            } else if line.starts_with('+') && !line.starts_with("+++") {
                format!("-{}", &line[1..])
            } else if line.starts_with('-') && !line.starts_with("---") {
                format!("+{}", &line[1..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if diff.ends_with('\n') {
        result.push('\n');
    }
    result
}
