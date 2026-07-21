use super::*;

pub(super) struct GitJob {
    pub(super) id: JobId,
    pub(super) job: Box<dyn FnOnce(RepositoryState, &mut AsyncApp) -> Task<()>>,
    pub(super) key: Option<GitJobKey>,
}

#[derive(PartialEq, Eq)]
pub(super) enum GitJobKey {
    WriteIndex(Vec<RepoPath>),
    ReloadBufferDiffBases,
    RefreshStatuses,
    ReloadGitState,
}
