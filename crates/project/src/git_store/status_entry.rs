use super::*;

impl StatusEntry {
    pub(super) fn to_proto(&self) -> proto::StatusEntry {
        let simple_status = match self.status {
            FileStatus::Ignored | FileStatus::Untracked => proto::GitStatus::Added as i32,
            FileStatus::Unmerged { .. } => proto::GitStatus::Conflict as i32,
            FileStatus::Tracked(TrackedStatus {
                index_status,
                worktree_status,
            }) => tracked_status_to_proto(if worktree_status != StatusCode::Unmodified {
                worktree_status
            } else {
                index_status
            }),
        };

        proto::StatusEntry {
            repo_path: self.repo_path.to_proto(),
            simple_status,
            status: Some(status_to_proto(self.status)),
            diff_stat_added: self.diff_stat.map(|ds| ds.added),
            diff_stat_deleted: self.diff_stat.map(|ds| ds.deleted),
        }
    }
}

impl TryFrom<proto::StatusEntry> for StatusEntry {
    type Error = anyhow::Error;

    fn try_from(value: proto::StatusEntry) -> Result<Self, Self::Error> {
        let repo_path = RepoPath::from_proto(&value.repo_path).context("invalid repo path")?;
        let status = status_from_proto(value.simple_status, value.status)?;
        let diff_stat = match (value.diff_stat_added, value.diff_stat_deleted) {
            (Some(added), Some(deleted)) => Some(DiffStat { added, deleted }),
            _ => None,
        };
        Ok(Self {
            repo_path,
            status,
            diff_stat,
        })
    }
}

impl sum_tree::Item for StatusEntry {
    type Summary = PathSummary<GitSummary>;

    fn summary(&self, _: <Self::Summary as sum_tree::Summary>::Context<'_>) -> Self::Summary {
        PathSummary {
            max_path: self.repo_path.as_ref().clone(),
            item_summary: self.status.summary(),
        }
    }
}

impl sum_tree::KeyedItem for StatusEntry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.repo_path.as_ref().clone())
    }
}
