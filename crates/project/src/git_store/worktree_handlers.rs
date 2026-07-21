use super::*;

impl GitStore {
    pub(super) async fn handle_get_worktrees(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitGetWorktrees>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitWorktreesResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let worktrees = repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.worktrees()
            })
            .await??;

        Ok(proto::GitWorktreesResponse {
            worktrees: worktrees
                .into_iter()
                .map(|worktree| worktree_to_proto(&worktree))
                .collect::<Vec<_>>(),
        })
    }

    pub(super) async fn handle_create_worktree(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCreateWorktree>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let directory = PathBuf::from(envelope.payload.directory);
        let name = envelope.payload.name;
        let commit = envelope.payload.commit;
        let use_existing_branch = envelope.payload.use_existing_branch;
        let target = if name.is_empty() {
            CreateWorktreeTarget::Detached { base_sha: commit }
        } else if use_existing_branch {
            CreateWorktreeTarget::ExistingBranch { branch_name: name }
        } else {
            CreateWorktreeTarget::NewBranch {
                branch_name: name,
                base_sha: commit,
            }
        };

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.create_worktree(target, directory)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_remove_worktree(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRemoveWorktree>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let path = PathBuf::from(envelope.payload.path);
        let force = envelope.payload.force;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.remove_worktree(path, force)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_rename_worktree(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRenameWorktree>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let old_path = PathBuf::from(envelope.payload.old_path);
        let new_path = PathBuf::from(envelope.payload.new_path);

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.rename_worktree(old_path, new_path)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_worktree_created_at(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitWorktreeCreatedAt>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitWorktreeCreatedAtResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let worktree_path = PathBuf::from(envelope.payload.worktree_path);

        let created_at = repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.worktree_created_at(worktree_path)
            })
            .await??;

        Ok(proto::GitWorktreeCreatedAtResponse {
            created_at: created_at.map(Into::into),
        })
    }

    pub(super) async fn handle_get_head_sha(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitGetHeadSha>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitGetHeadShaResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let head_sha = repository_handle
            .update(&mut cx, |repository_handle, _| repository_handle.head_sha())
            .await??;

        Ok(proto::GitGetHeadShaResponse { sha: head_sha })
    }
}
