use super::RepoPath;
use crate::SHORT_SHA_LENGTH;
use gpui::SharedString;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Branch {
    pub is_head: bool,
    pub ref_name: SharedString,
    pub upstream: Option<Upstream>,
    pub most_recent_commit: Option<CommitSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchesScanResult {
    pub branches: Vec<Branch>,
    pub error: Option<SharedString>,
}

impl From<Vec<Branch>> for BranchesScanResult {
    fn from(branches: Vec<Branch>) -> Self {
        Self {
            branches,
            error: None,
        }
    }
}

impl Branch {
    pub fn name(&self) -> &str {
        self.ref_name
            .as_ref()
            .strip_prefix("refs/heads/")
            .or_else(|| self.ref_name.as_ref().strip_prefix("refs/remotes/"))
            .unwrap_or(self.ref_name.as_ref())
    }

    pub fn is_remote(&self) -> bool {
        self.ref_name.starts_with("refs/remotes/")
    }

    pub fn remote_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split("/").next())
    }

    pub fn tracking_status(&self) -> Option<UpstreamTrackingStatus> {
        self.upstream
            .as_ref()
            .and_then(|upstream| upstream.tracking.status())
    }

    pub fn priority_key(&self) -> (bool, Option<i64>) {
        (
            self.is_head,
            self.most_recent_commit
                .as_ref()
                .map(|commit| commit.commit_timestamp),
        )
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Upstream {
    pub ref_name: SharedString,
    pub tracking: UpstreamTracking,
}

impl Upstream {
    pub fn is_remote(&self) -> bool {
        self.remote_name().is_some()
    }

    pub fn remote_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split("/").next())
    }

    pub fn stripped_ref_name(&self) -> Option<&str> {
        self.ref_name.strip_prefix("refs/remotes/")
    }

    pub fn branch_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split_once('/').map(|(_, name)| name))
    }
}

#[derive(Clone, Copy, Default)]
pub struct CommitOptions {
    pub amend: bool,
    pub signoff: bool,
    pub allow_empty: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum UpstreamTracking {
    /// Remote ref not present in local repository.
    Gone,
    /// Remote ref present in local repository (fetched from remote).
    Tracked(UpstreamTrackingStatus),
}

impl From<UpstreamTrackingStatus> for UpstreamTracking {
    fn from(status: UpstreamTrackingStatus) -> Self {
        UpstreamTracking::Tracked(status)
    }
}

impl UpstreamTracking {
    pub fn is_gone(&self) -> bool {
        matches!(self, UpstreamTracking::Gone)
    }

    pub fn status(&self) -> Option<UpstreamTrackingStatus> {
        match self {
            UpstreamTracking::Gone => None,
            UpstreamTracking::Tracked(status) => Some(*status),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteCommandOutput {
    pub stdout: String,
    pub stderr: String,
}

impl RemoteCommandOutput {
    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct UpstreamTrackingStatus {
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: SharedString,
    pub subject: SharedString,
    /// This is a unix timestamp
    pub commit_timestamp: i64,
    pub author_name: SharedString,
    pub has_parent: bool,
}

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub struct CommitDetails {
    pub sha: SharedString,
    pub message: SharedString,
    pub commit_timestamp: i64,
    pub author_email: SharedString,
    pub author_name: SharedString,
}

#[derive(Debug)]
pub struct CommitDiff {
    pub files: Vec<CommitFile>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileHistoryChangedFileSets {
    pub file_sets: Vec<Vec<RepoPath>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommitFileStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug)]
pub struct CommitFile {
    pub path: RepoPath,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    pub is_binary: bool,
}

impl CommitFile {
    pub fn status(&self) -> CommitFileStatus {
        match (&self.old_text, &self.new_text) {
            (None, Some(_)) => CommitFileStatus::Added,
            (Some(_), None) => CommitFileStatus::Deleted,
            _ => CommitFileStatus::Modified,
        }
    }
}

impl CommitDetails {
    pub fn short_sha(&self) -> SharedString {
        self.sha[..SHORT_SHA_LENGTH].to_string().into()
    }
}

/// Detects if content is binary by checking for NUL bytes in the first 8000 bytes.
/// This matches git's binary detection heuristic.
pub fn is_binary_content(content: &[u8]) -> bool {
    let check_len = content.len().min(8000);
    content[..check_len].contains(&0)
}
