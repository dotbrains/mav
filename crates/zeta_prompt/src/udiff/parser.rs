use std::{
    borrow::Cow,
    fmt::{Display, Write},
    mem,
    ops::Range,
};

use anyhow::{Context as _, Result};

struct PatchFile<'a> {
    old_path: Cow<'a, str>,
    new_path: Cow<'a, str>,
}

pub struct DiffParser<'a> {
    current_file: Option<PatchFile<'a>>,
    current_line: Option<(&'a str, DiffLine<'a>)>,
    hunk: Hunk,
    diff: std::str::Lines<'a>,
    pending_start_line: Option<u32>,
    processed_no_newline: bool,
    last_diff_op: LastDiffOp,
}

#[derive(Clone, Copy, Default)]
enum LastDiffOp {
    #[default]
    None,
    Context,
    Deletion,
    Addition,
}

#[derive(Debug, PartialEq)]
pub enum DiffEvent<'a> {
    Hunk {
        path: Cow<'a, str>,
        hunk: Hunk,
        status: FileStatus,
    },
    FileEnd {
        renamed_to: Option<Cow<'a, str>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Created,
    Modified,
    Deleted,
}

#[derive(Debug, Default, PartialEq)]
pub struct Hunk {
    pub context: String,
    pub edits: Vec<Edit>,
    pub start_line: Option<u32>,
}

impl Hunk {
    pub fn is_empty(&self) -> bool {
        self.context.is_empty() && self.edits.is_empty()
    }
}

#[derive(Debug, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl<'a> DiffParser<'a> {
    pub fn new(diff: &'a str) -> Self {
        let mut diff = diff.lines();
        let current_line = diff.next().map(|line| (line, DiffLine::parse(line)));
        DiffParser {
            current_file: None,
            hunk: Hunk::default(),
            current_line,
            diff,
            pending_start_line: None,
            processed_no_newline: false,
            last_diff_op: LastDiffOp::None,
        }
    }

    pub fn next(&mut self) -> Result<Option<DiffEvent<'a>>> {
        loop {
            let (hunk_done, file_done) = match self.current_line.as_ref().map(|e| &e.1) {
                Some(DiffLine::OldPath { .. }) | Some(DiffLine::Garbage(_)) | None => (true, true),
                Some(DiffLine::HunkHeader(_)) => (true, false),
                _ => (false, false),
            };

            if hunk_done {
                if let Some(file) = &self.current_file
                    && !self.hunk.is_empty()
                {
                    let status = if file.old_path == "/dev/null" {
                        FileStatus::Created
                    } else if file.new_path == "/dev/null" {
                        FileStatus::Deleted
                    } else {
                        FileStatus::Modified
                    };
                    let path = if status == FileStatus::Created {
                        file.new_path.clone()
                    } else {
                        file.old_path.clone()
                    };
                    let mut hunk = mem::take(&mut self.hunk);
                    hunk.start_line = self.pending_start_line.take();
                    self.processed_no_newline = false;
                    self.last_diff_op = LastDiffOp::None;
                    return Ok(Some(DiffEvent::Hunk { path, hunk, status }));
                }
            }

            if file_done {
                if let Some(PatchFile { old_path, new_path }) = self.current_file.take() {
                    return Ok(Some(DiffEvent::FileEnd {
                        renamed_to: if old_path != new_path && old_path != "/dev/null" {
                            Some(new_path)
                        } else {
                            None
                        },
                    }));
                }
            }

            let Some((line, parsed_line)) = self.current_line.take() else {
                break;
            };

            (|| {
                match parsed_line {
                    DiffLine::OldPath { path } => {
                        self.current_file = Some(PatchFile {
                            old_path: path,
                            new_path: "".into(),
                        });
                    }
                    DiffLine::NewPath { path } => {
                        if let Some(current_file) = &mut self.current_file {
                            current_file.new_path = path
                        }
                    }
                    DiffLine::HunkHeader(location) => {
                        if let Some(loc) = location {
                            self.pending_start_line = Some(loc.start_line_old);
                        }
                    }
                    DiffLine::Context(ctx) => {
                        if self.current_file.is_some() {
                            writeln!(&mut self.hunk.context, "{ctx}")?;
                            self.last_diff_op = LastDiffOp::Context;
                        }
                    }
                    DiffLine::Deletion(del) => {
                        if self.current_file.is_some() {
                            let range = self.hunk.context.len()
                                ..self.hunk.context.len() + del.len() + '\n'.len_utf8();
                            if let Some(last_edit) = self.hunk.edits.last_mut()
                                && last_edit.range.end == range.start
                            {
                                last_edit.range.end = range.end;
                            } else {
                                self.hunk.edits.push(Edit {
                                    range,
                                    text: String::new(),
                                });
                            }
                            writeln!(&mut self.hunk.context, "{del}")?;
                            self.last_diff_op = LastDiffOp::Deletion;
                        }
                    }
                    DiffLine::Addition(add) => {
                        if self.current_file.is_some() {
                            let range = self.hunk.context.len()..self.hunk.context.len();
                            if let Some(last_edit) = self.hunk.edits.last_mut()
                                && last_edit.range.end == range.start
                            {
                                writeln!(&mut last_edit.text, "{add}").unwrap();
                            } else {
                                self.hunk.edits.push(Edit {
                                    range,
                                    text: format!("{add}\n"),
                                });
                            }
                            self.last_diff_op = LastDiffOp::Addition;
                        }
                    }
                    DiffLine::NoNewlineAtEOF => {
                        if !self.processed_no_newline {
                            self.processed_no_newline = true;
                            match self.last_diff_op {
                                LastDiffOp::Addition => {
                                    // Remove trailing newline from the last addition
                                    if let Some(last_edit) = self.hunk.edits.last_mut() {
                                        last_edit.text.pop();
                                    }
                                }
                                LastDiffOp::Deletion => {
                                    // Remove trailing newline from context (which includes the deletion)
                                    self.hunk.context.pop();
                                    if let Some(last_edit) = self.hunk.edits.last_mut() {
                                        last_edit.range.end -= 1;
                                    }
                                }
                                LastDiffOp::Context | LastDiffOp::None => {
                                    // Remove trailing newline from context
                                    self.hunk.context.pop();
                                }
                            }
                        }
                    }
                    DiffLine::Garbage(_) => {}
                }

                anyhow::Ok(())
            })()
            .with_context(|| format!("on line:\n\n```\n{}```", line))?;

            self.current_line = self.diff.next().map(|line| (line, DiffLine::parse(line)));
        }

        anyhow::Ok(None)
    }
}

#[derive(Debug, PartialEq)]
pub enum DiffLine<'a> {
    OldPath { path: Cow<'a, str> },
    NewPath { path: Cow<'a, str> },
    HunkHeader(Option<HunkLocation>),
    Context(&'a str),
    Deletion(&'a str),
    Addition(&'a str),
    NoNewlineAtEOF,
    Garbage(&'a str),
}

#[derive(Debug, PartialEq)]
pub struct HunkLocation {
    pub start_line_old: u32,
    pub count_old: u32,
    pub start_line_new: u32,
    pub count_new: u32,
}

impl<'a> DiffLine<'a> {
    pub fn parse(line: &'a str) -> Self {
        Self::try_parse(line).unwrap_or(Self::Garbage(line))
    }

    fn try_parse(line: &'a str) -> Option<Self> {
        if line.starts_with("\\ No newline") {
            return Some(Self::NoNewlineAtEOF);
        }
        if let Some(header) = line.strip_prefix("---").and_then(eat_required_whitespace) {
            let path = parse_header_path("a/", header);
            Some(Self::OldPath { path })
        } else if let Some(header) = line.strip_prefix("+++").and_then(eat_required_whitespace) {
            Some(Self::NewPath {
                path: parse_header_path("b/", header),
            })
        } else if let Some(header) = line.strip_prefix("@@").and_then(eat_required_whitespace) {
            if header.starts_with("...") {
                return Some(Self::HunkHeader(None));
            }

            let mut tokens = header.split_whitespace();
            let old_range = tokens.next()?.strip_prefix('-')?;
            let new_range = tokens.next()?.strip_prefix('+')?;

            let (start_line_old, count_old) = old_range.split_once(',').unwrap_or((old_range, "1"));
            let (start_line_new, count_new) = new_range.split_once(',').unwrap_or((new_range, "1"));

            Some(Self::HunkHeader(Some(HunkLocation {
                start_line_old: start_line_old.parse::<u32>().ok()?.saturating_sub(1),
                count_old: count_old.parse().ok()?,
                start_line_new: start_line_new.parse::<u32>().ok()?.saturating_sub(1),
                count_new: count_new.parse().ok()?,
            })))
        } else if let Some(deleted_header) = line.strip_prefix("-") {
            Some(Self::Deletion(deleted_header))
        } else if line.is_empty() {
            Some(Self::Context(""))
        } else if let Some(context) = line.strip_prefix(" ") {
            Some(Self::Context(context))
        } else {
            Some(Self::Addition(line.strip_prefix("+")?))
        }
    }
}

impl<'a> Display for DiffLine<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffLine::OldPath { path } => write!(f, "--- {path}"),
            DiffLine::NewPath { path } => write!(f, "+++ {path}"),
            DiffLine::HunkHeader(Some(hunk_location)) => {
                write!(
                    f,
                    "@@ -{},{} +{},{} @@",
                    hunk_location.start_line_old + 1,
                    hunk_location.count_old,
                    hunk_location.start_line_new + 1,
                    hunk_location.count_new
                )
            }
            DiffLine::HunkHeader(None) => write!(f, "@@ ... @@"),
            DiffLine::Context(content) => write!(f, " {content}"),
            DiffLine::Deletion(content) => write!(f, "-{content}"),
            DiffLine::Addition(content) => write!(f, "+{content}"),
            DiffLine::NoNewlineAtEOF => write!(f, "\\ No newline at end of file"),
            DiffLine::Garbage(line) => write!(f, "{line}"),
        }
    }
}

pub(crate) fn parse_header_path<'a>(strip_prefix: &'static str, header: &'a str) -> Cow<'a, str> {
    if !header.contains(['"', '\\']) {
        let path = header.split_ascii_whitespace().next().unwrap_or(header);
        return Cow::Borrowed(path.strip_prefix(strip_prefix).unwrap_or(path));
    }

    let mut path = String::with_capacity(header.len());
    let mut in_quote = false;
    let mut chars = header.chars().peekable();
    let mut strip_prefix = Some(strip_prefix);

    while let Some(char) = chars.next() {
        if char == '"' {
            in_quote = !in_quote;
        } else if char == '\\' {
            let Some(&next_char) = chars.peek() else {
                break;
            };
            chars.next();
            path.push(next_char);
        } else if char.is_ascii_whitespace() && !in_quote {
            break;
        } else {
            path.push(char);
        }

        if let Some(prefix) = strip_prefix
            && path == prefix
        {
            strip_prefix.take();
            path.clear();
        }
    }

    Cow::Owned(path)
}

fn eat_required_whitespace(header: &str) -> Option<&str> {
    let trimmed = header.trim_ascii_start();

    if trimmed.len() == header.len() {
        None
    } else {
        Some(trimmed)
    }
}
