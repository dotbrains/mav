use super::*;

#[derive(Debug, Default, Clone)]
pub struct Patch {
    pub header: String,
    pub hunks: Vec<Hunk>,
}

pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
}

impl ToString for Patch {
    fn to_string(&self) -> String {
        let mut result = self.header.clone();
        for hunk in &self.hunks {
            let current_file = hunk.filename.clone();
            if hunk.is_file_creation() {
                result.push_str("--- /dev/null\n");
            } else {
                result.push_str(&format!("--- a/{}\n", current_file));
            }
            if hunk.is_file_deletion() {
                result.push_str("+++ /dev/null\n");
            } else {
                result.push_str(&format!("+++ b/{}\n", current_file));
            }
            result.push_str(&hunk.to_string());
        }

        result
    }
}

impl Patch {
    /// Parse a unified diff (git style) string into a `Patch`.
    pub fn parse_unified_diff(unified_diff: &str) -> Patch {
        let mut current_file = String::new();
        let mut is_filename_inherited = false;
        let mut hunk = Hunk::default();
        let mut patch = Patch::default();
        let mut in_header = true;

        for line in unified_diff.lines() {
            if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("@@") {
                in_header = false;
            }

            if in_header {
                patch.header.push_str(format!("{}\n", &line).as_ref());
                continue;
            }

            if line.starts_with("@@") {
                if !hunk.lines.is_empty() {
                    patch.hunks.push(hunk);
                }
                hunk = Hunk::from_header(line, &current_file, is_filename_inherited);
                is_filename_inherited = true;
            } else if let Some(path) = line.strip_prefix("--- ") {
                is_filename_inherited = false;
                let path = path.trim().strip_prefix("a/").unwrap_or(path);
                if path != "/dev/null" {
                    current_file = path.into();
                }
            } else if let Some(path) = line.strip_prefix("+++ ") {
                is_filename_inherited = false;
                let path = path.trim().strip_prefix("b/").unwrap_or(path);
                if path != "/dev/null" {
                    current_file = path.into();
                }
            } else if let Some(line) = line.strip_prefix("+") {
                hunk.lines.push(PatchLine::Addition(line.to_string()));
            } else if let Some(line) = line.strip_prefix("-") {
                hunk.lines.push(PatchLine::Deletion(line.to_string()));
            } else if let Some(line) = line.strip_prefix(" ") {
                hunk.lines.push(PatchLine::Context(line.to_string()));
            } else {
                hunk.lines.push(PatchLine::Garbage(line.to_string()));
            }
        }

        if !hunk.lines.is_empty() {
            patch.hunks.push(hunk);
        }

        let header_lines = patch.header.lines().collect::<Vec<&str>>();
        let len = header_lines.len();
        if len >= 2 {
            if header_lines[len - 2].starts_with("diff --git")
                && header_lines[len - 1].starts_with("index ")
            {
                patch.header = header_lines[..len - 2].join("\n") + "\n";
            }
        }
        if patch.header.trim().is_empty() {
            patch.header = String::new();
        }

        patch
    }

    /// Drop hunks that contain no additions or deletions.
    pub fn remove_empty_hunks(&mut self) {
        self.hunks.retain(|hunk| {
            hunk.lines
                .iter()
                .any(|line| matches!(line, PatchLine::Addition(_) | PatchLine::Deletion(_)))
        });
    }

    /// Make sure there are no more than `context_lines` lines of context around each change.
    pub fn normalize_hunks(&mut self, context_lines: usize) {
        for hunk in &mut self.hunks {
            // Find indices of all changes (additions and deletions)
            let change_indices: Vec<usize> = hunk
                .lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| match line {
                    PatchLine::Addition(_) | PatchLine::Deletion(_) => Some(i),
                    _ => None,
                })
                .collect();

            // If there are no changes, clear the hunk (it's all context)
            if change_indices.is_empty() {
                hunk.lines.clear();
                hunk.old_count = 0;
                hunk.new_count = 0;
                continue;
            }

            // Determine the range to keep
            let first_change = change_indices[0];
            let last_change = change_indices[change_indices.len() - 1];

            let start = first_change.saturating_sub(context_lines);
            let end = (last_change + context_lines + 1).min(hunk.lines.len());

            // Count lines trimmed from the beginning
            let (old_lines_before, new_lines_before) = count_lines(&hunk.lines[0..start]);

            // Keep only the lines in range + garbage
            let garbage_before = hunk.lines[..start]
                .iter()
                .filter(|line| matches!(line, PatchLine::Garbage(_)));
            let garbage_after = hunk.lines[end..]
                .iter()
                .filter(|line| matches!(line, PatchLine::Garbage(_)));

            hunk.lines = garbage_before
                .chain(hunk.lines[start..end].iter())
                .chain(garbage_after)
                .cloned()
                .collect();

            // Update hunk header
            let (old_count, new_count) = count_lines(&hunk.lines);
            hunk.old_start += old_lines_before as isize;
            hunk.new_start += new_lines_before as isize;
            hunk.old_count = old_count as isize;
            hunk.new_count = new_count as isize;
        }
    }

    /// Count total added and removed lines
    pub fn stats(&self) -> DiffStats {
        let mut added = 0;
        let mut removed = 0;

        for hunk in &self.hunks {
            for line in &hunk.lines {
                match line {
                    PatchLine::Addition(_) => added += 1,
                    PatchLine::Deletion(_) => removed += 1,
                    _ => {}
                }
            }
        }

        DiffStats { added, removed }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Hunk {
    pub old_start: isize,
    pub old_count: isize,
    pub new_start: isize,
    pub new_count: isize,
    pub comment: String,
    pub filename: String,
    pub is_filename_inherited: bool,
    pub lines: Vec<PatchLine>,
}

impl ToString for Hunk {
    fn to_string(&self) -> String {
        let header = self.header_string();
        let lines = self
            .lines
            .iter()
            .map(|line| line.to_string() + "\n")
            .collect::<Vec<String>>()
            .concat();
        format!("{header}\n{lines}")
    }
}

impl Hunk {
    /// Returns true if this hunk represents a file creation (old side is empty).
    pub fn is_file_creation(&self) -> bool {
        self.old_start == 0 && self.old_count == 0
    }

    /// Returns true if this hunk represents a file deletion (new side is empty).
    pub fn is_file_deletion(&self) -> bool {
        self.new_start == 0 && self.new_count == 0
    }

    /// Render the hunk header
    pub fn header_string(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@ {}",
            self.old_start,
            self.old_count,
            self.new_start,
            self.new_count,
            self.comment.clone()
        )
        .trim_end()
        .into()
    }

    /// Create a `Hunk` from a raw header line and associated filename.
    pub fn from_header(header: &str, filename: &str, is_filename_inherited: bool) -> Self {
        let (old_start, old_count, new_start, new_count, comment) = Self::parse_hunk_header(header);
        Self {
            old_start,
            old_count,
            new_start,
            new_count,
            comment,
            filename: filename.to_string(),
            is_filename_inherited,
            lines: Vec::new(),
        }
    }

    /// Parse hunk headers like `@@ -3,2 +3,2 @@ some garbage"
    fn parse_hunk_header(line: &str) -> (isize, isize, isize, isize, String) {
        let header_part = line.trim_start_matches("@@").trim();
        let parts: Vec<&str> = header_part.split_whitespace().collect();

        if parts.len() < 2 {
            return (0, 0, 0, 0, String::new());
        }

        let old_part = parts[0].trim_start_matches('-');
        let new_part = parts[1].trim_start_matches('+');

        let (old_start, old_count) = Hunk::parse_hunk_header_range(old_part);
        let (new_start, new_count) = Hunk::parse_hunk_header_range(new_part);

        let comment = if parts.len() > 2 {
            parts[2..]
                .join(" ")
                .trim_start_matches("@@")
                .trim()
                .to_string()
        } else {
            String::new()
        };

        (
            old_start as isize,
            old_count as isize,
            new_start as isize,
            new_count as isize,
            comment,
        )
    }

    fn parse_hunk_header_range(part: &str) -> (usize, usize) {
        let (old_start, old_count) = if part.contains(',') {
            let old_parts: Vec<&str> = part.split(',').collect();
            (
                old_parts[0].parse().unwrap_or(0),
                old_parts[1].parse().unwrap_or(0),
            )
        } else {
            (part.parse().unwrap_or(0), 1)
        };
        (old_start, old_count)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchLine {
    Context(String),
    Addition(String),
    Deletion(String),
    HunkHeader(usize, usize, usize, usize, String),
    FileStartMinus(String),
    FileStartPlus(String),
    Garbage(String),
}

impl PatchLine {
    pub fn parse(line: &str) -> Self {
        if let Some(line) = line.strip_prefix("+") {
            Self::Addition(line.to_string())
        } else if let Some(line) = line.strip_prefix("-") {
            Self::Deletion(line.to_string())
        } else if let Some(line) = line.strip_prefix(" ") {
            Self::Context(line.to_string())
        } else {
            Self::Garbage(line.to_string())
        }
    }
}

impl ToString for PatchLine {
    fn to_string(&self) -> String {
        match self {
            PatchLine::Context(line) => format!(" {}", line),
            PatchLine::Addition(line) => format!("+{}", line),
            PatchLine::Deletion(line) => format!("-{}", line),
            PatchLine::HunkHeader(old_start, old_end, new_start, new_end, comment) => format!(
                "@@ -{},{} +{},{} @@ {}",
                old_start, old_end, new_start, new_end, comment
            )
            .trim_end()
            .into(),
            PatchLine::FileStartMinus(filename) => format!("--- {}", filename),
            PatchLine::FileStartPlus(filename) => format!("+++ {}", filename),
            PatchLine::Garbage(line) => line.to_string(),
        }
    }
}

fn count_lines(lines: &[PatchLine]) -> (usize, usize) {
    lines.iter().fold((0, 0), |(old, new), line| match line {
        PatchLine::Context(_) => (old + 1, new + 1),
        PatchLine::Deletion(_) => (old + 1, new),
        PatchLine::Addition(_) => (old, new + 1),
        _ => (old, new),
    })
}
