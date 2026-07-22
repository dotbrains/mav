use super::*;

/// A commit has no split point matching the requested kind. This is an
/// expected outcome when filtering by kind, so such commits are skipped
/// rather than treated as failures.
#[derive(Debug)]
pub struct NoMatchingSplitPointError {
    kind: SplitPointKind,
}

impl std::fmt::Display for NoMatchingSplitPointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no split point found matching {}", self.kind)
    }
}

impl std::error::Error for NoMatchingSplitPointError {}

/// `ep split-commit` CLI args.
#[derive(Debug, Args, Clone)]
pub struct SplitCommitArgs {
    /// Split point (float 0.0-1.0 for fraction, integer for index, or one of: fim, same-file-near, same-file-far, cross-file; append :<index-or-fraction> to validate a specific split)
    #[arg(long, short = 's')]
    pub split_point: Option<String>,

    /// Random seed for reproducibility
    #[arg(long)]
    pub seed: Option<u64>,

    /// Pretty-print JSON output
    #[arg(long, short = 'p')]
    pub pretty: bool,

    /// Number of samples to generate per commit (samples random split points)
    #[arg(long, short = 'n')]
    pub num_samples: Option<usize>,
}

/// Input format for annotated commits (JSON Lines).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AnnotatedCommit {
    /// Repository path (e.g., "repos/mav")
    pub repo: String,
    /// Repository URL (e.g., "https://github.com/mav-industries/mav")
    pub repo_url: String,
    /// Commit SHA
    pub commit_sha: String,
    /// Chronologically reordered commit diff
    pub reordered_commit: String,
    /// Original commit diff
    pub original_commit: String,
    /// Whether diff stats match between original and reordered
    pub diff_stats_match: bool,
}

/// Cursor position in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorPosition {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub line_length: usize,
}

impl std::fmt::Display for CursorPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Represents a split commit with source and target patches.
#[derive(Debug, Clone)]
pub struct SplitCommit {
    pub source_patch: String,
    pub target_patch: String,
}

/// Split point specification for evaluation generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitPointKind {
    Fim,
    SameFileNear,
    SameFileFar,
    CrossFile,
}

impl std::fmt::Display for SplitPointKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitPointKind::Fim => write!(f, "fim"),
            SplitPointKind::SameFileNear => write!(f, "same-file-near"),
            SplitPointKind::SameFileFar => write!(f, "same-file-far"),
            SplitPointKind::CrossFile => write!(f, "cross-file"),
        }
    }
}

impl std::str::FromStr for SplitPointKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "fim" => Ok(Self::Fim),
            "same-file-near" => Ok(Self::SameFileNear),
            "same-file-far" => Ok(Self::SameFileFar),
            "cross-file" => Ok(Self::CrossFile),
            _ => anyhow::bail!(
                "invalid split point kind '{value}' (expected fim, same-file-near, same-file-far, or cross-file)"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SplitPoint {
    /// Fraction of total edits (0.0 to 1.0)
    Fraction(f64),
    /// Absolute index
    Index(usize),
    /// Random split point matching the requested kind.
    Kind(SplitPointKind),
    /// Explicit split point that must match the requested kind.
    KindWithSplit {
        kind: SplitPointKind,
        split_point: SplitPointValue,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SplitPointValue {
    Fraction(f64),
    Index(usize),
}

pub(super) fn parse_split_point_value(value: &str) -> Result<SplitPointValue> {
    if value.contains('.') {
        value
            .parse::<f64>()
            .map(SplitPointValue::Fraction)
            .with_context(|| format!("invalid split point fraction '{value}'"))
    } else {
        value
            .parse::<usize>()
            .map(SplitPointValue::Index)
            .with_context(|| format!("invalid split point index '{value}'"))
    }
}

pub(super) fn parse_split_point(value: &str) -> Result<SplitPoint> {
    if let Some((kind, split_point)) = value.split_once(':') {
        let kind = kind.parse::<SplitPointKind>()?;
        anyhow::ensure!(
            !split_point.is_empty(),
            "missing split point after kind '{kind}:'"
        );
        return Ok(SplitPoint::KindWithSplit {
            kind,
            split_point: parse_split_point_value(split_point)?,
        });
    }

    if let Ok(kind) = value.parse::<SplitPointKind>() {
        return Ok(SplitPoint::Kind(kind));
    }

    match parse_split_point_value(value)? {
        SplitPointValue::Fraction(value) => Ok(SplitPoint::Fraction(value)),
        SplitPointValue::Index(value) => Ok(SplitPoint::Index(value)),
    }
}
