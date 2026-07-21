use super::*;

impl GitStore {
    pub(super) async fn handle_create_checkpoint(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCreateCheckpoint>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitCreateCheckpointResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let checkpoint = repository_handle
            .update(&mut cx, |repository, _| repository.checkpoint())
            .await??;

        Ok(proto::GitCreateCheckpointResponse {
            commit_sha: checkpoint.commit_sha.as_bytes().to_vec(),
        })
    }

    pub(super) async fn handle_create_archive_checkpoint(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCreateArchiveCheckpoint>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitCreateArchiveCheckpointResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let (staged_commit_sha, unstaged_commit_sha) = repository_handle
            .update(&mut cx, |repository, _| {
                repository.create_archive_checkpoint()
            })
            .await??;

        Ok(proto::GitCreateArchiveCheckpointResponse {
            staged_commit_sha,
            unstaged_commit_sha,
        })
    }

    pub(super) async fn handle_restore_checkpoint(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRestoreCheckpoint>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let checkpoint = GitRepositoryCheckpoint {
            commit_sha: Oid::from_bytes(&envelope.payload.commit_sha)?,
        };

        repository_handle
            .update(&mut cx, |repository, _| {
                repository.restore_checkpoint(checkpoint)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_restore_archive_checkpoint(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRestoreArchiveCheckpoint>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let staged_commit_sha = envelope.payload.staged_commit_sha;
        let unstaged_commit_sha = envelope.payload.unstaged_commit_sha;

        repository_handle
            .update(&mut cx, |repository, _| {
                repository.restore_archive_checkpoint(staged_commit_sha, unstaged_commit_sha)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_compare_checkpoints(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCompareCheckpoints>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitCompareCheckpointsResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let left = GitRepositoryCheckpoint {
            commit_sha: Oid::from_bytes(&envelope.payload.left_commit_sha)?,
        };
        let right = GitRepositoryCheckpoint {
            commit_sha: Oid::from_bytes(&envelope.payload.right_commit_sha)?,
        };

        let equal = repository_handle
            .update(&mut cx, |repository, _| {
                repository.compare_checkpoints(left, right)
            })
            .await??;

        Ok(proto::GitCompareCheckpointsResponse { equal })
    }

    pub(super) async fn handle_diff_checkpoints(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitDiffCheckpoints>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitDiffCheckpointsResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let base = GitRepositoryCheckpoint {
            commit_sha: Oid::from_bytes(&envelope.payload.base_commit_sha)?,
        };
        let target = GitRepositoryCheckpoint {
            commit_sha: Oid::from_bytes(&envelope.payload.target_commit_sha)?,
        };

        let diff = repository_handle
            .update(&mut cx, |repository, _| {
                repository.diff_checkpoints(base, target)
            })
            .await??;

        Ok(proto::GitDiffCheckpointsResponse { diff })
    }

    pub(super) async fn handle_load_commit_diff(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::LoadCommitDiff>,
        mut cx: AsyncApp,
    ) -> Result<proto::LoadCommitDiffResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let commit_diff = repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.load_commit_diff(envelope.payload.commit)
            })
            .await??;
        Ok(proto::LoadCommitDiffResponse {
            files: commit_diff
                .files
                .into_iter()
                .map(|file| proto::CommitFile {
                    path: file.path.to_proto(),
                    old_text: file.old_text,
                    new_text: file.new_text,
                    is_binary: file.is_binary,
                })
                .collect(),
        })
    }
}
